//! Finds TVs on the local network with SSDP (the `M-SEARCH` every smart TV answers), and
//! Android TV / Google TV with mDNS (`_androidtvremote2._tcp`, what Google's remote looks for).

use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use crate::tv::{Brand, Device};
use crate::Result;

const SSDP: &str = "239.255.255.250:1900";

/// The search targets that make each brand answer.
const TARGETS: [&str; 4] = [
    "urn:samsung.com:device:RemoteControlReceiver:1",
    "urn:lge-com:service:webos-second-screen:1",
    "roku:ecp",
    "urn:dial-multiscreen-org:service:dial:1",
];

/// Searches for `wait`, and returns every TV that answered (one per IP).
pub fn discover(wait: Duration) -> Result<Vec<Device>> {
    let mdns = std::thread::spawn(move || android_tvs(wait).unwrap_or_default());
    let mut devices = ssdp(wait)?;
    for (ip, name) in mdns.join().unwrap_or_default() {
        match devices.iter_mut().find(|d| d.ip == ip) {
            // The remote service is the truth: a DIAL-only guess becomes an Android TV.
            Some(d) => d.brand = Brand::AndroidTv,
            None => devices.push(Device::new(&name, "", Brand::AndroidTv, &ip)),
        }
    }
    devices.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(devices)
}

const MDNS: &str = "224.0.0.251:5353";
const ATV_SERVICE: &str = "_androidtvremote2._tcp.local";

/// Asks the network who runs the Android TV Remote service; returns (IP, the TV's name).
fn android_tvs(wait: Duration) -> Result<Vec<(String, String)>> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(Duration::from_millis(250)))?;
    let to: SocketAddr = MDNS.parse().expect("valid mDNS address");
    socket.send_to(&mdns_query(ATV_SERVICE), to)?;
    let mut found: HashMap<String, String> = HashMap::new();
    let started = Instant::now();
    let mut buf = [0u8; 4096];
    while started.elapsed() < wait {
        let Ok((n, from)) = socket.recv_from(&mut buf) else { continue };
        if let Some(name) = mdns_instance(&buf[..n], ATV_SERVICE) {
            found.insert(from.ip().to_string(), name);
        }
    }
    Ok(found.into_iter().collect())
}

/// A one-question mDNS query: PTR for `service`, asking for a unicast answer.
pub(crate) fn mdns_query(service: &str) -> Vec<u8> {
    let mut q = vec![0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in service.split('.') {
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.extend_from_slice(&[0, 0, 12, 0x80, 1]);
    q
}

/// Reads a DNS name at `pos` (with compression); returns the labels.
fn dns_name(msg: &[u8], mut pos: usize) -> Option<(Vec<String>, usize)> {
    let mut labels = Vec::new();
    let mut end = None;
    for _ in 0..64 {
        let len = *msg.get(pos)? as usize;
        if len == 0 {
            return Some((labels, end.unwrap_or(pos + 1)));
        }
        if len & 0xC0 == 0xC0 {
            let ptr = ((len & 0x3F) << 8) | *msg.get(pos + 1)? as usize;
            end.get_or_insert(pos + 2);
            pos = ptr;
            continue;
        }
        labels.push(String::from_utf8_lossy(msg.get(pos + 1..pos + 1 + len)?).into_owned());
        pos += 1 + len;
    }
    None
}

/// The instance name (`Freebox Player`) of the first PTR answer for `service` in an mDNS reply.
pub(crate) fn mdns_instance(msg: &[u8], service: &str) -> Option<String> {
    if msg.len() < 12 || msg[2] & 0x80 == 0 {
        return None;
    }
    let count = |i: usize| u16::from_be_bytes([msg[i], msg[i + 1]]) as usize;
    let (questions, records) = (count(4), count(6) + count(8) + count(10));
    let mut pos = 12;
    for _ in 0..questions {
        pos = dns_name(msg, pos)?.1 + 4;
    }
    for _ in 0..records {
        let (owner, next) = dns_name(msg, pos)?;
        let kind = u16::from_be_bytes([*msg.get(next)?, *msg.get(next + 1)?]);
        let len = u16::from_be_bytes([*msg.get(next + 8)?, *msg.get(next + 9)?]) as usize;
        let data = next + 10;
        if kind == 12 && owner.join(".").eq_ignore_ascii_case(service) {
            let (target, _) = dns_name(msg, data)?;
            return target.first().cloned();
        }
        pos = data + len;
    }
    None
}

/// The SSDP half of [`discover`].
fn ssdp(wait: Duration) -> Result<Vec<Device>> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(Duration::from_millis(250)))?;
    let to: SocketAddr = SSDP.parse().expect("valid SSDP address");
    for st in TARGETS {
        let msg = format!("M-SEARCH * HTTP/1.1\r\nHOST: {SSDP}\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: {st}\r\n\r\n");
        socket.send_to(msg.as_bytes(), to)?;
    }

    let mut found: HashMap<String, (Brand, String)> = HashMap::new();
    let started = Instant::now();
    let mut buf = [0u8; 2048];
    while started.elapsed() < wait {
        let Ok((n, from)) = socket.recv_from(&mut buf) else { continue };
        let reply = String::from_utf8_lossy(&buf[..n]);
        let brand = classify(&reply);
        let ip = from.ip().to_string();
        let location = header(&reply, "location").unwrap_or_default();
        // A specific brand wins over the generic DIAL answer of the same TV.
        let keep = match found.get(&ip) {
            Some((Brand::Unknown | Brand::AndroidTv, _)) | None => true,
            Some(_) => false,
        };
        if keep && brand != Brand::Unknown {
            found.insert(ip, (brand, location));
        }
    }

    let mut devices: Vec<Device> = found
        .into_iter()
        .map(|(ip, (brand, location))| {
            let (name, model) = describe(&location).unwrap_or_else(|| (brand.label().to_string(), String::new()));
            Device::new(&name, &model, brand, &ip)
        })
        .collect();
    devices.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(devices)
}

/// Reads one header of an SSDP reply (case-insensitive).
fn header(reply: &str, name: &str) -> Option<String> {
    reply.lines().find_map(|line| {
        let (k, v) = line.split_once(':')?;
        k.trim().eq_ignore_ascii_case(name).then(|| v.trim().to_string())
    })
}

/// Tells the brand from the SSDP reply.
pub(crate) fn classify(reply: &str) -> Brand {
    let r = reply.to_ascii_lowercase();
    if r.contains("samsung") {
        Brand::Samsung
    } else if r.contains("webos") || r.contains("lge") {
        Brand::Lg
    } else if r.contains("roku") {
        Brand::Roku
    } else if r.contains("dial-multiscreen") || r.contains("android") {
        // Most DIAL answers that are not Samsung, LG or Roku are Android / Google TVs.
        Brand::AndroidTv
    } else {
        Brand::Unknown
    }
}

/// Fetches the UPnP description for the friendly name and model.
fn describe(location: &str) -> Option<(String, String)> {
    if location.is_empty() {
        return None;
    }
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(2)).build();
    let xml = agent.get(location).call().ok()?.into_string().ok()?;
    let name = tag(&xml, "friendlyName")?;
    let model = tag(&xml, "modelName").unwrap_or_default();
    Some((name, model))
}

fn tag(xml: &str, name: &str) -> Option<String> {
    let start = xml.find(&format!("<{name}>"))? + name.len() + 2;
    let end = xml[start..].find(&format!("</{name}>"))? + start;
    Some(xml[start..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_replies() {
        assert_eq!(classify("ST: roku:ecp\r\nServer: Roku/9.0"), Brand::Roku);
        assert_eq!(classify("ST: urn:lge-com:service:webos-second-screen:1"), Brand::Lg);
        assert_eq!(classify("SERVER: SHP, UPnP/1.0, Samsung UPnP SDK/1.0"), Brand::Samsung);
        assert_eq!(classify("ST: urn:dial-multiscreen-org:service:dial:1"), Brand::AndroidTv);
    }

    #[test]
    fn mdns_query_and_answer() {
        let q = mdns_query(ATV_SERVICE);
        assert_eq!(&q[..12], &[0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(q[12], 17); // "_androidtvremote2"
        assert_eq!(&q[q.len() - 4..], &[0, 12, 0x80, 1]);

        // A reply: header (response, 0 questions, 1 answer), then the PTR record whose data
        // is "Freebox Player" + a pointer back to the service name at offset 12.
        let mut r = vec![0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 0];
        r.extend_from_slice(&q[12..q.len() - 4]); // the owner name, at offset 12
        let mut rdata = vec![14];
        rdata.extend_from_slice(b"Freebox Player");
        rdata.extend_from_slice(&[0xC0, 12]);
        r.extend_from_slice(&[0, 12, 0, 1, 0, 0, 0x11, 0x94]);
        r.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        r.extend_from_slice(&rdata);
        assert_eq!(mdns_instance(&r, ATV_SERVICE).as_deref(), Some("Freebox Player"));
        assert_eq!(mdns_instance(&r, "_other._tcp.local"), None);
        assert_eq!(mdns_instance(&q, ATV_SERVICE), None); // a query isn't an answer
    }

    #[test]
    fn reads_headers_and_tags() {
        assert_eq!(header("HTTP/1.1 200 OK\r\nLOCATION: http://1.2.3.4/d.xml\r\n", "location").as_deref(), Some("http://1.2.3.4/d.xml"));
        assert_eq!(tag("<root><friendlyName> Living Room </friendlyName></root>", "friendlyName").as_deref(), Some("Living Room"));
    }
}
