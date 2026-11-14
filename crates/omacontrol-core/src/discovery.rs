//! Finds TVs on the local network with SSDP (the `M-SEARCH` every smart TV answers).

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
    fn reads_headers_and_tags() {
        assert_eq!(header("HTTP/1.1 200 OK\r\nLOCATION: http://1.2.3.4/d.xml\r\n", "location").as_deref(), Some("http://1.2.3.4/d.xml"));
        assert_eq!(tag("<root><friendlyName> Living Room </friendlyName></root>", "friendlyName").as_deref(), Some("Living Room"));
    }
}
