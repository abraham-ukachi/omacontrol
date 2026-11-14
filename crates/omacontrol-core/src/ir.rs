//! # Infrared 🔴
//!
//! The part of OmaControl that will, one day, drive anything with an IR receiver:
//! TVs first, then air conditioners (fixed and portable).
//!
//! A remote button is a **pattern**: alternating on/off durations in microseconds,
//! sent on a carrier (usually 38 kHz). This module turns protocol codes into patterns,
//! and the phone's IR blaster (Android's `ConsumerIrManager`) plays them.
//!
//! | No. | Protocol | Carrier | Used by |
//! |:----|:---------|:--------|:--------|
//! | 1 | NEC | 38 kHz | LG, many Asian brands, set-top boxes |
//! | 2 | Samsung32 | 38 kHz | Samsung TVs |
//! | 3 | Sony SIRC (12-bit) | 40 kHz | Sony TVs |
//! | 4 | Pronto hex | any | raw codes from IR databases (and most AC remotes) |

use crate::tv::Brand;
use crate::{Error, Key, Result};

/// One IR signal, ready for the blaster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    /// Carrier frequency in Hz.
    pub carrier: u32,
    /// Mark, space, mark, space... in microseconds.
    pub pulses: Vec<u32>,
}

/// Something that can send IR (the phone's blaster, or a test double).
pub trait IrTransmitter: Send + Sync {
    /// `false` when the phone has no IR blaster.
    fn has_emitter(&self) -> bool;
    fn transmit(&self, pattern: &Pattern) -> Result<()>;
}

fn bits_lsb(pulses: &mut Vec<u32>, byte: u8, count: u8, mark: u32, zero: u32, one: u32) {
    for i in 0..count {
        pulses.push(mark);
        pulses.push(if byte >> i & 1 == 1 { one } else { zero });
    }
}

/// NEC: 9 ms leader, 4.5 ms space, then address, !address, command, !command (LSB first).
pub fn nec(address: u8, command: u8) -> Pattern {
    let mut p = vec![9000, 4500];
    for b in [address, !address, command, !command] {
        bits_lsb(&mut p, b, 8, 560, 560, 1690);
    }
    p.push(560);
    Pattern { carrier: 38_000, pulses: p }
}

/// Samsung32: like NEC with a 4.5 ms leader and the address sent twice.
pub fn samsung32(address: u8, command: u8) -> Pattern {
    let mut p = vec![4500, 4500];
    for b in [address, address, command, !command] {
        bits_lsb(&mut p, b, 8, 560, 560, 1690);
    }
    p.push(560);
    Pattern { carrier: 38_000, pulses: p }
}

/// Sony SIRC 12-bit: 7-bit command, 5-bit address, sent 3 times (45 ms frames).
pub fn sony12(address: u8, command: u8) -> Pattern {
    let mut frame = vec![2400, 600];
    for i in 0..7 {
        frame.push(if command >> i & 1 == 1 { 1200 } else { 600 });
        frame.push(600);
    }
    for i in 0..5 {
        frame.push(if address >> i & 1 == 1 { 1200 } else { 600 });
        frame.push(600);
    }
    let used: u32 = frame.iter().sum();
    *frame.last_mut().expect("frame is not empty") += 45_000u32.saturating_sub(used);
    let pulses = frame.iter().copied().cycle().take(frame.len() * 3).collect();
    Pattern { carrier: 40_000, pulses }
}

/// Parses a raw Pronto hex code (`0000 006D 0022 0002 0157 00AC ...`).
pub fn pronto(hex: &str) -> Result<Pattern> {
    let words: Vec<u32> = hex
        .split_whitespace()
        .map(|w| u32::from_str_radix(w, 16))
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| Error::Protocol("not a Pronto hex code".into()))?;
    if words.len() < 4 || words[0] != 0 || words[1] == 0 {
        return Err(Error::Protocol("only raw (0000) Pronto codes are supported".into()));
    }
    let carrier = (1_000_000.0 / (words[1] as f64 * 0.241_246)).round() as u32;
    let period_us = 1_000_000.0 / carrier as f64;
    let pairs = (words[2] + words[3]) as usize;
    let body = &words[4..];
    if body.len() < pairs * 2 {
        return Err(Error::Protocol("the Pronto code is shorter than it says".into()));
    }
    let pulses = body[..pairs * 2].iter().map(|&n| (n as f64 * period_us).round() as u32).collect();
    Ok(Pattern { carrier, pulses })
}

/// The IR codes of a TV brand.
///
/// > NOTE: these are the common codes published in public IR databases (IRDB, LIRC).
/// > Most models of each brand answer them, but a few need another code set.
pub fn tv_code(brand: Brand, key: Key) -> Option<Pattern> {
    match brand {
        Brand::Samsung => samsung_tv(key).map(|c| samsung32(0x07, c)),
        Brand::Lg => lg_tv(key).map(|c| nec(0x04, c)),
        Brand::Sony => sony_tv(key).map(|c| sony12(0x01, c)),
        _ => None,
    }
}

fn samsung_tv(key: Key) -> Option<u8> {
    Some(match key {
        Key::Power => 0x02,
        Key::Input => 0x01,
        Key::Mute => 0x0F,
        Key::VolumeUp => 0x07,
        Key::VolumeDown => 0x0B,
        Key::ChannelUp => 0x12,
        Key::ChannelDown => 0x10,
        Key::Up => 0x60,
        Key::Down => 0x61,
        Key::Left => 0x65,
        Key::Right => 0x62,
        Key::Ok => 0x68,
        Key::Back => 0x58,
        Key::Menu => 0x1A,
        Key::Home => 0x79,
        Key::Digit(d) => [0x11, 0x04, 0x05, 0x06, 0x08, 0x09, 0x0A, 0x0C, 0x0D, 0x0E][d as usize % 10],
        Key::PlayPause => 0x47,
        Key::Rewind => 0x45,
        Key::FastForward => 0x48,
        Key::Dash => return None,
    })
}

fn lg_tv(key: Key) -> Option<u8> {
    Some(match key {
        Key::Power => 0x08,
        Key::Input => 0x0B,
        Key::Mute => 0x09,
        Key::VolumeUp => 0x02,
        Key::VolumeDown => 0x03,
        Key::ChannelUp => 0x00,
        Key::ChannelDown => 0x01,
        Key::Up => 0x40,
        Key::Down => 0x41,
        Key::Left => 0x07,
        Key::Right => 0x06,
        Key::Ok => 0x44,
        Key::Back => 0x28,
        Key::Menu => 0x43,
        Key::Home => 0x7C,
        Key::Digit(d) => 0x10 + d % 10,
        Key::PlayPause => 0xB0,
        Key::Rewind => 0x8F,
        Key::FastForward => 0x8E,
        Key::Dash => 0x4C,
    })
}

fn sony_tv(key: Key) -> Option<u8> {
    Some(match key {
        Key::Power => 21,
        Key::Input => 37,
        Key::Mute => 20,
        Key::VolumeUp => 18,
        Key::VolumeDown => 19,
        Key::ChannelUp => 16,
        Key::ChannelDown => 17,
        Key::Up => 116,
        Key::Down => 117,
        Key::Left => 52,
        Key::Right => 51,
        Key::Ok => 101,
        Key::Menu => 96,
        Key::Digit(0) => 9,
        Key::Digit(d) => d - 1,
        Key::Back | Key::Home | Key::PlayPause | Key::Rewind | Key::FastForward | Key::Dash => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nec_frame_shape() {
        let p = nec(0x04, 0x08);
        assert_eq!(p.carrier, 38_000);
        assert_eq!(p.pulses.len(), 2 + 32 * 2 + 1);
        assert_eq!(&p.pulses[..2], &[9000, 4500]);
        // address 0x04 LSB first: 0,0,1 -> third bit's space is long.
        assert_eq!(p.pulses[2 + 2 * 2 + 1], 1690);
    }

    #[test]
    fn samsung_power_is_e0e040bf() {
        // The famous Samsung power code 0xE0E040BF is address 0x07 twice, command 0x02.
        let p = samsung32(0x07, 0x02);
        let mut value: u32 = 0;
        for (i, pair) in p.pulses[2..66].chunks(2).enumerate() {
            if pair[1] == 1690 {
                value |= 1 << i;
            }
        }
        assert_eq!(value.reverse_bits(), 0xE0E040BF);
    }

    #[test]
    fn sony_repeats_three_frames_of_45ms() {
        let p = sony12(1, 21);
        assert_eq!(p.carrier, 40_000);
        assert_eq!(p.pulses.len(), 26 * 3);
        assert_eq!(p.pulses[..26].iter().sum::<u32>(), 45_000);
    }

    #[test]
    fn parses_pronto() {
        let p = pronto("0000 006D 0002 0000 0157 00AC 0015 0016").unwrap();
        assert_eq!(p.carrier, 38_029);
        assert_eq!(p.pulses.len(), 4);
        assert!(pronto("0100 006D 0000 0000").is_err());
    }

    #[test]
    fn every_tv_brand_has_power() {
        for b in [Brand::Samsung, Brand::Lg, Brand::Sony] {
            assert!(tv_code(b, Key::Power).is_some());
        }
    }
}
