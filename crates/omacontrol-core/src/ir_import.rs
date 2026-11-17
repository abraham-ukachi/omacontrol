//! # Importing IR codes 📥
//!
//! Phones can only send IR, so a key nobody has a protocol for gets its code pasted in. Four
//! formats are understood, the ones IR databases and other remotes hand out:
//!
//! | No. | Format | Looks like |
//! |:----|:-------|:-----------|
//! | 1 | Pronto hex (raw `0000`) | `0000 006D 0022 0002 0157 00AC …` |
//! | 2 | Raw timings (µs, mark first) | `38000: 9000 4500 560 560 …`, `+9000 -4500 …` |
//! | 3 | Flipper Zero `.ir` file (Flipper-IRDB) | `name: Power` / `type: parsed` / `protocol: NEC` … |
//! | 4 | LIRC (`lircd.conf` raw codes, or `mode2` pulse/space lines) | `begin raw_codes` / `name KEY_POWER` … |
//!
//! Each signal becomes a **code string** that [`crate::ac::custom_frames`] plays back:
//! `NEC 00FF40BF`, `SAMSUNG 0702`, `SONY 0115`, a Pronto hex string, or
//! `RAW <carrier> <mark> <space> …`. Those strings are what a device saves per key.

use crate::ir::{self, Pattern};
use crate::{Error, Result};

/// The carrier assumed when a raw code doesn't say (almost every remote uses 38 kHz).
pub const DEFAULT_CARRIER: u32 = 38_000;
/// Phones' IR blasters refuse anything much longer than this (about 2 s of pulses).
const MAX_TOTAL_US: u64 = 2_000_000;
const MAX_PULSES: usize = 1024;

/// One imported signal: its name in the source (empty for a bare code) and its code string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub name: String,
    pub code: String,
}

fn bad(why: impl Into<String>) -> Error {
    Error::Protocol(why.into())
}

/// Checks pulses the way a phone's blaster would take them.
fn checked(carrier: u32, pulses: Vec<u32>) -> Result<Pattern> {
    if !(20_000..=60_000).contains(&carrier) {
        return Err(bad(format!("carrier {carrier} Hz is outside 20–60 kHz")));
    }
    if pulses.len() < 4 {
        return Err(bad("too few timings for an IR signal"));
    }
    if pulses.len() > MAX_PULSES {
        return Err(bad(format!("{} timings is more than a phone can send ({MAX_PULSES})", pulses.len())));
    }
    if pulses.contains(&0) {
        return Err(bad("a timing of 0 µs"));
    }
    let total: u64 = pulses.iter().map(|&p| u64::from(p)).sum();
    if total > MAX_TOTAL_US {
        return Err(bad("the signal is longer than 2 seconds"));
    }
    Ok(Pattern { carrier, pulses })
}

/// The `RAW <carrier> <timings…>` code string for a pattern.
pub fn raw_code(pattern: &Pattern) -> String {
    let mut s = format!("RAW {}", pattern.carrier);
    for p in &pattern.pulses {
        s.push(' ');
        s.push_str(&p.to_string());
    }
    s
}

/// Plays a `RAW <carrier> <timings…>` code string back.
pub fn raw_pattern(code: &str) -> Result<Pattern> {
    let rest = code.strip_prefix("RAW ").ok_or_else(|| bad("not a raw code"))?;
    let mut numbers = rest.split_whitespace().map(|w| w.parse::<u32>());
    let carrier = numbers
        .next()
        .and_then(|n| n.ok())
        .ok_or_else(|| bad("the raw code has no carrier"))?;
    let pulses = numbers
        .collect::<std::result::Result<Vec<u32>, _>>()
        .map_err(|_| bad("the raw code has a timing that isn't a number"))?;
    checked(carrier, pulses)
}

/// Parses raw timings: whitespace or comma separated microseconds, mark first, optionally
/// signed (`+9000 -4500`), optionally led by the carrier (`38000:` or `freq=38000`).
pub fn parse_raw(text: &str) -> Result<Pattern> {
    let mut carrier = DEFAULT_CARRIER;
    let mut body = text.trim();
    for prefix in ["freq=", "frequency=", "frequency:", "carrier=", "carrier:"] {
        if let Some(rest) = body.strip_prefix(prefix) {
            let (number, rest) = rest.trim_start().split_once(|c: char| c.is_whitespace() || c == ',' || c == ';').unwrap_or((rest, ""));
            carrier = number.trim().parse().map_err(|_| bad("the carrier isn't a number"))?;
            body = rest;
            break;
        }
    }
    if let Some((head, rest)) = body.split_once(':') {
        carrier = head.trim().parse().map_err(|_| bad("the carrier before ':' isn't a number"))?;
        body = rest;
    }
    let pulses = body
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .filter(|w| !w.is_empty())
        .map(|w| w.trim_start_matches(['+', '-']).parse::<u32>())
        .collect::<std::result::Result<Vec<u32>, _>>()
        .map_err(|_| bad("not a list of timings in microseconds"))?;
    // A trailing space is meaningless to a blaster; an odd count is fine (it ends on a mark).
    checked(carrier, pulses)
}

fn hex_bytes(text: &str) -> Result<Vec<u8>> {
    text.split_whitespace()
        .map(|w| u8::from_str_radix(w, 16))
        .collect::<std::result::Result<Vec<u8>, _>>()
        .map_err(|_| bad(format!("'{text}' isn't a list of hex bytes")))
}

/// A Flipper `parsed` signal as a code string we can play.
fn flipper_parsed(protocol: &str, address: &str, command: &str) -> Result<String> {
    let a = hex_bytes(address)?;
    let c = hex_bytes(command)?;
    let byte = |v: &[u8], i: usize| v.get(i).copied().unwrap_or(0);
    match protocol.to_ascii_lowercase().as_str() {
        "nec" => Ok(format!("NEC {:08X}", ir::nec_code(byte(&a, 0), byte(&c, 0)))),
        "samsung32" => Ok(format!("SAMSUNG {:02X}{:02X}", byte(&a, 0), byte(&c, 0))),
        "sirc" => {
            if byte(&c, 0) > 0x7F || byte(&a, 0) > 0x1F {
                return Err(bad("SIRC codes are 7-bit commands on 5-bit addresses"));
            }
            Ok(format!("SONY {:02X}{:02X}", byte(&a, 0), byte(&c, 0)))
        }
        // Everything else becomes raw timings, which we build for the protocols we know.
        "necext" => Ok(raw_code(&nec_ext(
            u16::from_le_bytes([byte(&a, 0), byte(&a, 1)]),
            u16::from_le_bytes([byte(&c, 0), byte(&c, 1)]),
        ))),
        other => Err(bad(format!(
            "the {other} protocol isn't supported yet: export the signal as raw from the Flipper"
        ))),
    }
}

/// NEC with a 16-bit address and a 16-bit command (Flipper's `NECext`), LSB first.
fn nec_ext(address: u16, command: u16) -> Pattern {
    let mut p = vec![9000, 4500];
    for word in [address, command] {
        for i in 0..16 {
            p.push(560);
            p.push(if word >> i & 1 == 1 { 1690 } else { 560 });
        }
    }
    p.push(560);
    Pattern { carrier: 38_000, pulses: p }
}

/// Parses a Flipper Zero `.ir` file: every signal, in order.
pub fn parse_flipper(text: &str) -> Result<Vec<Imported>> {
    #[derive(Default)]
    struct Signal {
        name: String,
        kind: String,
        protocol: String,
        address: String,
        command: String,
        frequency: Option<u32>,
        data: String,
    }
    fn finish(s: Signal, out: &mut Vec<Imported>, skipped: &mut Vec<String>) {
        if s.name.is_empty() {
            return;
        }
        let code = match s.kind.as_str() {
            "parsed" => flipper_parsed(&s.protocol, &s.address, &s.command),
            "raw" => parse_raw(&s.data).and_then(|p| {
                checked(s.frequency.unwrap_or(DEFAULT_CARRIER), p.pulses).map(|p| raw_code(&p))
            }),
            other => Err(bad(format!("unknown signal type '{other}'"))),
        };
        match code {
            Ok(code) => out.push(Imported { name: s.name, code }),
            Err(e) => skipped.push(format!("{}: {e}", s.name)),
        }
    }
    let mut out = Vec::new();
    let mut skipped = Vec::new();
    let mut current: Option<Signal> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = value.trim().to_string();
        match key.trim() {
            "name" => {
                if let Some(s) = current.take() {
                    finish(s, &mut out, &mut skipped);
                }
                current = Some(Signal { name: value, ..Signal::default() });
            }
            k => {
                let Some(s) = current.as_mut() else { continue };
                match k {
                    "type" => s.kind = value.to_ascii_lowercase(),
                    "protocol" => s.protocol = value,
                    "address" => s.address = value,
                    "command" => s.command = value,
                    "frequency" => s.frequency = value.parse().ok(),
                    "data" => s.data = value,
                    _ => {}
                }
            }
        }
    }
    if let Some(s) = current.take() {
        finish(s, &mut out, &mut skipped);
    }
    if out.is_empty() {
        return Err(bad(match skipped.first() {
            Some(why) => format!("no signal could be imported ({why})"),
            None => "no signals in this Flipper file".into(),
        }));
    }
    Ok(out)
}

/// Parses LIRC: a `lircd.conf` with `begin raw_codes` (every named code), or `mode2` output
/// (`pulse 9000` / `space 4500` lines, one signal).
pub fn parse_lirc(text: &str) -> Result<Vec<Imported>> {
    let mut carrier = DEFAULT_CARRIER;
    let mut out = Vec::new();
    let mut in_raw = false;
    let mut name: Option<String> = None;
    let mut numbers: Vec<u32> = Vec::new();
    let mut mode2: Vec<u32> = Vec::new();
    let flush = |name: &mut Option<String>, numbers: &mut Vec<u32>, out: &mut Vec<Imported>, carrier: u32| -> Result<()> {
        if let Some(n) = name.take() {
            let pattern = checked(carrier, std::mem::take(numbers)).map_err(|e| bad(format!("{n}: {e}")))?;
            out.push(Imported { name: n, code: raw_code(&pattern) });
        }
        Ok(())
    };
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut words = line.split_whitespace();
        let first = words.next().unwrap_or("");
        match first {
            "frequency" => {
                if let Some(f) = words.next().and_then(|w| w.parse().ok()) {
                    carrier = f;
                }
            }
            "begin" if line.contains("raw_codes") => in_raw = true,
            "end" if line.contains("raw_codes") => {
                flush(&mut name, &mut numbers, &mut out, carrier)?;
                in_raw = false;
            }
            "name" if in_raw => {
                flush(&mut name, &mut numbers, &mut out, carrier)?;
                name = words.next().map(str::to_string);
            }
            "pulse" | "space" if !in_raw => {
                let n = words.next().and_then(|w| w.parse().ok()).ok_or_else(|| bad("a mode2 line without a number"))?;
                // A leading space is the gap before the signal; skip it.
                if !(first == "space" && mode2.is_empty()) {
                    mode2.push(n);
                }
            }
            _ if in_raw && name.is_some() => {
                for w in line.split_whitespace() {
                    numbers.push(w.parse().map_err(|_| bad(format!("'{w}' in a raw code isn't a number")))?);
                }
            }
            _ => {}
        }
    }
    flush(&mut name, &mut numbers, &mut out, carrier)?;
    if !mode2.is_empty() {
        let pattern = checked(carrier, mode2)?;
        out.push(Imported { name: String::new(), code: raw_code(&pattern) });
    }
    if out.is_empty() {
        return Err(bad("no raw codes in this LIRC text (only raw_codes and mode2 are supported)"));
    }
    Ok(out)
}

/// Imports whatever was pasted: Pronto, raw timings, a Flipper `.ir` file or LIRC text.
/// Malformed or unsupported input is an error that says why.
pub fn import(text: &str) -> Result<Vec<Imported>> {
    let text = text.trim();
    if text.is_empty() {
        return Err(bad("nothing to import"));
    }
    if text.lines().any(|l| l.trim_start().starts_with("Filetype:") || l.trim_start().starts_with("name:")) {
        return parse_flipper(text);
    }
    if text.contains("begin remote") || text.contains("raw_codes") || text.lines().any(|l| {
        let l = l.trim_start();
        l.starts_with("pulse ") || l.starts_with("space ")
    }) {
        return parse_lirc(text);
    }
    // One of our own code strings (`NEC 00FF40BF`, `RAW 38000 …`) goes back in as it is.
    if text.starts_with("NEC ") || text.starts_with("SAMSUNG ") || text.starts_with("SONY ") || text.starts_with("RAW ") {
        crate::ac::custom_frames(text)?;
        return Ok(vec![Imported { name: String::new(), code: text.to_string() }]);
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    let pronto_like = words.len() >= 4 && words.iter().all(|w| w.len() == 4 && w.chars().all(|c| c.is_ascii_hexdigit()));
    if pronto_like {
        let normalized = words.join(" ").to_ascii_uppercase();
        ir::pronto(&normalized)?;
        return Ok(vec![Imported { name: String::new(), code: normalized }]);
    }
    let pattern = parse_raw(text)?;
    Ok(vec![Imported { name: String::new(), code: raw_code(&pattern) }])
}

/// The AC key an imported signal's name stands for (`Power`, `TEMP+`, `Cool_hi`…), if any.
pub fn ac_key_for(name: &str) -> Option<&'static str> {
    let n: String = name.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '+' || *c == '-').collect();
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    Some(if has(&["power", "onoff", "pwr"]) || n == "off" || n == "on" {
        "power"
    } else if has(&["tempup", "temp+", "up", "plus"]) {
        "warmer"
    } else if has(&["tempdn", "tempdown", "temp-", "down", "minus"]) {
        "cooler"
    } else if has(&["mode"]) {
        "mode"
    } else if has(&["fan", "speed"]) {
        "fan"
    } else if has(&["timer"]) {
        "timer"
    } else if has(&["sleep"]) {
        "sleep"
    } else if has(&["swing", "flap", "louver"]) {
        "swing"
    } else {
        return None;
    })
}

/// Imports codes for an AC's keys. Named signals (a Flipper file, LIRC) land on the keys their
/// names stand for; a bare code, or a file where no name matches, goes to `key`.
pub fn ac_import(text: &str, key: &str) -> Result<std::collections::BTreeMap<String, String>> {
    let imported = import(text)?;
    let mut keys = std::collections::BTreeMap::new();
    for signal in &imported {
        if let Some(k) = ac_key_for(&signal.name) {
            keys.entry(k.to_string()).or_insert_with(|| signal.code.clone());
        }
    }
    if !keys.contains_key(key) {
        let unnamed = imported.iter().find(|s| ac_key_for(&s.name).is_none());
        match unnamed.or(if keys.is_empty() { imported.first() } else { None }) {
            Some(signal) if imported.len() == 1 || keys.is_empty() => {
                keys.insert(key.to_string(), signal.code.clone());
            }
            _ => {}
        }
    }
    if keys.is_empty() {
        return Err(bad("none of the codes is named like an AC key"));
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLIPPER: &str = "Filetype: IR signals file
Version: 1
#
name: Power
type: parsed
protocol: NEC
address: 00 00 00 00
command: 40 00 00 00
#
name: Temp_up
type: raw
frequency: 38000
duty_cycle: 0.330000
data: 9024 4512 579 552 579 1683 579
#
name: Mode
type: parsed
protocol: RC5
address: 00 00 00 00
command: 01 00 00 00
";

    #[test]
    fn flipper_files_import_parsed_and_raw_signals() {
        let codes = parse_flipper(FLIPPER).unwrap();
        assert_eq!(codes.len(), 2, "the RC5 signal is skipped, not fatal");
        assert_eq!(codes[0], Imported { name: "Power".into(), code: "NEC 00FF40BF".into() });
        assert_eq!(codes[1].code, "RAW 38000 9024 4512 579 552 579 1683 579");
        assert!(crate::ac::custom_frames(&codes[0].code).is_ok());
        assert_eq!(raw_pattern(&codes[1].code).unwrap().pulses.len(), 7);
        assert_eq!(import(FLIPPER).unwrap().len(), 2);
        assert!(parse_flipper("name: X\ntype: parsed\nprotocol: RC6\naddress: 00\ncommand: 01\n").is_err());
    }

    #[test]
    fn necext_and_sirc_become_playable_codes() {
        let ext = flipper_parsed("NECext", "86 6B 00 00", "12 ED 00 00").unwrap();
        let p = raw_pattern(&ext).unwrap();
        assert_eq!(p.pulses.len(), 2 + 64 + 1);
        assert_eq!(flipper_parsed("SIRC", "01 00 00 00", "15 00 00 00").unwrap(), "SONY 0115");
        assert!(flipper_parsed("SIRC", "01 00 00 00", "95 00 00 00").is_err());
    }

    #[test]
    fn raw_timings_take_signs_commas_and_a_carrier() {
        let p = parse_raw("+9000 -4500 +560 -560 +560").unwrap();
        assert_eq!((p.carrier, p.pulses.len()), (38_000, 5));
        let p = parse_raw("36000: 9000, 4500, 560, 560").unwrap();
        assert_eq!(p.carrier, 36_000);
        let p = parse_raw("freq=40000 2400 600 1200 600").unwrap();
        assert_eq!(p.carrier, 40_000);
        assert!(parse_raw("9000 4500 abc 560").is_err());
        assert!(parse_raw("9000 4500").is_err(), "too short");
        assert!(parse_raw("9000 0 560 560").is_err(), "zero timing");
        assert!(parse_raw("200000: 9000 4500 560 560").is_err(), "carrier out of range");
    }

    #[test]
    fn lirc_raw_codes_and_mode2_import() {
        let conf = "begin remote
  name  AC
  flags RAW_CODES
  frequency 38000
  begin raw_codes
    name KEY_POWER
      9000 4500 560 560
      560 1690 560
    name KEY_MODE
      9000 4500 560 1690 560
  end raw_codes
end remote";
        let codes = import(conf).unwrap();
        assert_eq!(codes.len(), 2);
        assert_eq!(codes[0].name, "KEY_POWER");
        assert_eq!(codes[0].code, "RAW 38000 9000 4500 560 560 560 1690 560");
        assert_eq!(ac_key_for(&codes[1].name), Some("mode"));

        let mode2 = "space 1000000\npulse 9000\nspace 4500\npulse 560\nspace 560\npulse 560";
        let codes = import(mode2).unwrap();
        assert_eq!(codes[0].code, "RAW 38000 9000 4500 560 560 560");
        assert!(parse_lirc("begin remote\nend remote").is_err());
    }

    #[test]
    fn pronto_and_our_own_codes_import_as_they_are() {
        let codes = import("0000 006d 0002 0000 0157 00ac 0015 0016").unwrap();
        assert_eq!(codes[0].code, "0000 006D 0002 0000 0157 00AC 0015 0016");
        assert!(import("0000 006D 0009 0000 0157").is_err(), "shorter than it says");
        assert_eq!(import("NEC 00FF40BF").unwrap()[0].code, "NEC 00FF40BF");
        assert!(import("NEC 00FF40BE").is_err(), "bad complement");
        assert!(import("").is_err());
        assert!(import("hello world").is_err());
    }

    #[test]
    fn signal_names_map_to_ac_keys() {
        assert_eq!(ac_key_for("Power"), Some("power"));
        assert_eq!(ac_key_for("TEMP+"), Some("warmer"));
        assert_eq!(ac_key_for("Temp_dn"), Some("cooler"));
        assert_eq!(ac_key_for("Fan_speed"), Some("fan"));
        assert_eq!(ac_key_for("Swing"), Some("swing"));
        assert_eq!(ac_key_for("Timer"), Some("timer"));
        assert_eq!(ac_key_for("Eco"), None);
        // Flipper-IRDB's whole-state AC buttons are not the − / + keys.
        assert_eq!(ac_key_for("Cool_hi"), None);
    }

    #[test]
    fn ac_imports_map_names_and_bare_codes() {
        let keys = ac_import(FLIPPER, "power").unwrap();
        assert_eq!(keys.get("power").map(String::as_str), Some("NEC 00FF40BF"));
        assert!(keys.contains_key("warmer"));
        let bare = ac_import("0000 006D 0002 0000 0157 00AC 0015 0016", "swing").unwrap();
        assert_eq!(bare.len(), 1);
        assert!(bare.contains_key("swing"));
        // One unnamed-looking signal in a file: it goes to the key being imported.
        let one = ac_import("name: Eco\ntype: parsed\nprotocol: NEC\naddress: 00 00 00 00\ncommand: 41 00 00 00\n", "sleep").unwrap();
        assert_eq!(one.get("sleep").map(String::as_str), Some("NEC 00FF41BE"));
    }

    #[test]
    fn too_long_signals_are_refused() {
        let long = vec!["10000"; 300].join(" ");
        assert!(parse_raw(&long).is_err());
    }
}
