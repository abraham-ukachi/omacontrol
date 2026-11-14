//! # Air conditioners ❄️
//!
//! An AC remote doesn't send "button pressed": every press sends the **whole state**
//! (power, mode, fan, temperature...), so the AC always ends up matching the remote.
//! IR is one way, so OmaControl keeps that state itself ([`AcState`]) and saves it.
//!
//! Most brands (Cecotec, Comfee, Kaysun, Hisense, Toshiba, Gree, TCL...) are rebadged units
//! that speak one of a handful of protocols. Phones can only send IR, not receive it,
//! so setup simply tries each protocol until the AC beeps ([`AcProtocol::ALL`]).
//!
//! | No. | Protocol | Frame | Common brands |
//! |:----|:---------|:------|:--------------|
//! | 1 | Midea | 48 bits, sent twice (normal, inverted) | Midea, Comfee, Cecotec, Kaysun, Pioneer, Danby |
//! | 2 | Coolix | 24 bits, each byte followed by its inverse | Midea (older), Beko, Toshiba, Carrier, Electrolux |
//! | 3 | Gree | 8 bytes in two blocks | Gree, Cooper & Hunter, Amana, Soleus Air, EKOKAI |
//! | 4 | TCL112 | 14 bytes | TCL, Daewoo, Teknopoint, many portables |
//!
//! > NOTE: the frame layouts come from the open-source IRremoteESP8266 library (LGPL-2.1,
//! > layouts and timings only; no code copied), which documents them from real remotes.

use serde::{Deserialize, Serialize};

use crate::ir::Pattern;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AcMode {
    #[default]
    Cool,
    Fan,
    Dry,
    Heat,
    Auto,
}

impl AcMode {
    /// The modes, in the order the mode button cycles through them (like a Cecotec panel).
    pub const CYCLE: [AcMode; 4] = [AcMode::Cool, AcMode::Fan, AcMode::Dry, AcMode::Heat];

    pub fn label(self) -> &'static str {
        match self {
            AcMode::Cool => "cool",
            AcMode::Fan => "fan",
            AcMode::Dry => "dry",
            AcMode::Heat => "heat",
            AcMode::Auto => "auto",
        }
    }

    pub fn next(self) -> AcMode {
        let i = Self::CYCLE.iter().position(|&m| m == self).map_or(0, |i| i + 1);
        Self::CYCLE[i % Self::CYCLE.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AcFan {
    Auto,
    #[default]
    Low,
    Medium,
    High,
}

impl AcFan {
    pub const CYCLE: [AcFan; 3] = [AcFan::Low, AcFan::Medium, AcFan::High];

    pub fn label(self) -> &'static str {
        match self {
            AcFan::Auto => "auto",
            AcFan::Low => "low",
            AcFan::Medium => "med",
            AcFan::High => "high",
        }
    }

    pub fn next(self) -> AcFan {
        let i = Self::CYCLE.iter().position(|&f| f == self).map_or(0, |i| i + 1);
        Self::CYCLE[i % Self::CYCLE.len()]
    }
}

/// What the AC should be doing. Saved with the device, so the remote survives restarts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcState {
    pub power: bool,
    pub mode: AcMode,
    pub fan: AcFan,
    /// Degrees Celsius.
    pub temp: u8,
    pub swing: bool,
    pub sleep: bool,
}

impl Default for AcState {
    fn default() -> Self {
        AcState { power: false, mode: AcMode::Cool, fan: AcFan::Low, temp: 22, swing: false, sleep: false }
    }
}

pub const MIN_TEMP: u8 = 17;
pub const MAX_TEMP: u8 = 30;

impl AcState {
    pub fn warmer(&mut self) {
        self.temp = (self.temp + 1).min(MAX_TEMP);
    }
    pub fn cooler(&mut self) {
        self.temp = self.temp.saturating_sub(1).max(MIN_TEMP);
    }
}

/// What one tap changes. Most taps just resend the whole state; a few protocols
/// use one-off "toggle" frames for swing and sleep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcChange {
    /// Power, mode, fan or temperature changed: send the state.
    State,
    /// The swing button.
    Swing,
    /// The sleep button.
    Sleep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AcProtocol {
    Midea,
    Coolix,
    Gree,
    Tcl112,
}

impl AcProtocol {
    /// Setup tries them in this order: the most common in European portables first.
    pub const ALL: [AcProtocol; 4] = [AcProtocol::Midea, AcProtocol::Coolix, AcProtocol::Gree, AcProtocol::Tcl112];

    pub fn label(self) -> &'static str {
        match self {
            AcProtocol::Midea => "Midea",
            AcProtocol::Coolix => "Coolix",
            AcProtocol::Gree => "Gree",
            AcProtocol::Tcl112 => "TCL",
        }
    }

    /// The IR frames for one tap, in the order to send them.
    pub fn frames(self, state: &AcState, change: AcChange) -> Vec<Pattern> {
        match self {
            AcProtocol::Midea => midea_frames(state, change),
            AcProtocol::Coolix => coolix_frames(state, change),
            AcProtocol::Gree => vec![gree(&gree_state(state))],
            AcProtocol::Tcl112 => vec![tcl112(&tcl112_state(state))],
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Shared encoding

struct Timing {
    hdr_mark: u32,
    hdr_space: u32,
    bit_mark: u32,
    one: u32,
    zero: u32,
}

fn push_byte(p: &mut Vec<u32>, t: &Timing, byte: u8, msb_first: bool) {
    for i in 0..8 {
        let bit = if msb_first { byte >> (7 - i) & 1 } else { byte >> i & 1 };
        p.push(t.bit_mark);
        p.push(if bit == 1 { t.one } else { t.zero });
    }
}

fn clamp_temp(t: u8) -> u8 {
    t.clamp(MIN_TEMP, MAX_TEMP)
}

// ---------------------------------------------------------------------------------------------
// Midea (48 bits: header 0xA1, power/mode/fan, temperature, timers, checksum)

const MIDEA: Timing = Timing { hdr_mark: 4480, hdr_space: 4480, bit_mark: 560, one: 1680, zero: 560 };
const MIDEA_GAP: u32 = 5600;
/// The swing toggle, a one-off "special" frame.
const MIDEA_SWING_TOGGLE: u64 = 0xA201_FFFF_FF7C;

/// The 48-bit Midea state (Celsius).
pub fn midea_state(s: &AcState) -> u64 {
    let mode: u64 = match s.mode {
        AcMode::Cool => 0,
        AcMode::Dry => 1,
        AcMode::Auto => 2,
        AcMode::Heat => 3,
        AcMode::Fan => 4,
    };
    let fan: u64 = match s.fan {
        AcFan::Auto => 0,
        AcFan::Low => 1,
        AcFan::Medium => 2,
        AcFan::High => 3,
    };
    let byte4 = (s.power as u64) << 7 | (s.sleep as u64) << 6 | fan << 3 | mode;
    let byte3 = 0x40 | (clamp_temp(s.temp) - MIN_TEMP) as u64;
    let raw = 0xA1 << 40 | byte4 << 32 | byte3 << 24 | 0xFF << 16 | 0xFF << 8;
    raw | midea_checksum(raw) as u64
}

/// Two's complement of the sum of the five data bytes, each bit-reversed.
pub fn midea_checksum(raw: u64) -> u8 {
    let sum = (1..6).fold(0u8, |acc, i| acc.wrapping_add(((raw >> (8 * i)) as u8).reverse_bits()));
    0u8.wrapping_sub(sum).reverse_bits()
}

/// A Midea message: the frame, then the same frame inverted.
pub fn midea(raw: u64) -> Pattern {
    let mut p = Vec::with_capacity(2 * (2 + 96 + 2));
    for data in [raw, !raw] {
        p.push(MIDEA.hdr_mark);
        p.push(MIDEA.hdr_space);
        for i in (0..6).rev() {
            push_byte(&mut p, &MIDEA, (data >> (8 * i)) as u8, true);
        }
        p.push(MIDEA.bit_mark);
        p.push(MIDEA_GAP);
    }
    Pattern { carrier: 38_000, pulses: p }
}

fn midea_frames(s: &AcState, change: AcChange) -> Vec<Pattern> {
    match change {
        AcChange::Swing => vec![midea(MIDEA_SWING_TOGGLE)],
        _ => vec![midea(midea_state(s))],
    }
}

// ---------------------------------------------------------------------------------------------
// Coolix (24 bits: 0xB2, fan + sensor, temperature + mode)

const COOLIX: Timing = Timing { hdr_mark: 4692, hdr_space: 4416, bit_mark: 552, one: 1656, zero: 552 };
const COOLIX_GAP: u32 = 5244;
pub const COOLIX_OFF: u32 = 0xB2_7BE0;
pub const COOLIX_SWING: u32 = 0xB2_6BE0;
pub const COOLIX_SLEEP: u32 = 0xB2_E003;
/// Coolix scrambles its temperatures (Gray-code-ish), 17 °C to 30 °C.
const COOLIX_TEMPS: [u8; 14] = [0b0000, 0b0001, 0b0011, 0b0010, 0b0110, 0b0111, 0b0101, 0b0100, 0b1100, 0b1101, 0b1001, 0b1000, 0b1010, 0b1011];

/// The 24-bit Coolix state (only meaningful while the power is on; off is [`COOLIX_OFF`]).
pub fn coolix_state(s: &AcState) -> u32 {
    if !s.power {
        return COOLIX_OFF;
    }
    // Fan mode is "dry" with a special temperature code.
    let (mode, temp) = match s.mode {
        AcMode::Cool => (0b00, COOLIX_TEMPS[(clamp_temp(s.temp) - MIN_TEMP) as usize]),
        AcMode::Dry => (0b01, COOLIX_TEMPS[(clamp_temp(s.temp) - MIN_TEMP) as usize]),
        AcMode::Auto => (0b10, COOLIX_TEMPS[(clamp_temp(s.temp) - MIN_TEMP) as usize]),
        AcMode::Heat => (0b11, COOLIX_TEMPS[(clamp_temp(s.temp) - MIN_TEMP) as usize]),
        AcMode::Fan => (0b01, 0b1110),
    };
    // Dry and auto only accept "auto0"; the others take a real speed.
    let fan: u32 = match (s.mode, s.fan) {
        (AcMode::Dry | AcMode::Auto, _) => 0b000,
        (_, AcFan::Auto) => 0b101,
        (_, AcFan::Low) => 0b100,
        (_, AcFan::Medium) => 0b010,
        (_, AcFan::High) => 0b001,
    };
    let sensor_ignore = 0b11111;
    0xB2 << 16 | (fan << 5 | sensor_ignore) << 8 | (temp as u32) << 4 | mode << 2
}

/// A Coolix message, sent twice.
pub fn coolix(raw: u32) -> Pattern {
    let mut frame = vec![COOLIX.hdr_mark, COOLIX.hdr_space];
    for i in (0..3).rev() {
        let byte = (raw >> (8 * i)) as u8;
        push_byte(&mut frame, &COOLIX, byte, true);
        push_byte(&mut frame, &COOLIX, !byte, true);
    }
    frame.push(COOLIX.bit_mark);
    frame.push(COOLIX_GAP);
    let pulses = frame.iter().copied().cycle().take(frame.len() * 2).collect();
    Pattern { carrier: 38_000, pulses }
}

fn coolix_frames(s: &AcState, change: AcChange) -> Vec<Pattern> {
    match change {
        AcChange::Swing => vec![coolix(COOLIX_SWING)],
        AcChange::Sleep => vec![coolix(COOLIX_SLEEP)],
        AcChange::State => vec![coolix(coolix_state(s))],
    }
}

// ---------------------------------------------------------------------------------------------
// Gree (8 bytes in two blocks of 4, LSB first, 4-bit checksum)

const GREE: Timing = Timing { hdr_mark: 9000, hdr_space: 4500, bit_mark: 620, one: 1600, zero: 540 };
const GREE_MSG_SPACE: u32 = 19_980;

/// The 8-byte Gree state (YAW1F remote, light on).
pub fn gree_state(s: &AcState) -> [u8; 8] {
    let mode = match s.mode {
        AcMode::Auto => 0,
        AcMode::Cool => 1,
        AcMode::Dry => 2,
        AcMode::Fan => 3,
        AcMode::Heat => 4,
    };
    let fan = match (s.mode, s.fan) {
        (AcMode::Dry, _) => 1,
        (_, AcFan::Auto) => 0,
        (_, AcFan::Low) => 1,
        (_, AcFan::Medium) => 2,
        (_, AcFan::High) => 3,
    };
    let temp = if s.mode == AcMode::Auto { 25 } else { s.temp.clamp(16, 30) };
    let mut b = [0u8; 8];
    b[0] = (s.sleep as u8) << 7 | (s.swing as u8) << 6 | fan << 4 | (s.power as u8) << 3 | mode;
    b[1] = temp - 16;
    // Light on, and the "model A" bit the YAW1F remote sets while on.
    b[2] = 0x20 | (s.power as u8) << 6;
    b[3] = 0x50;
    b[4] = if s.swing { 0x01 } else { 0x00 };
    b[5] = 0x20;
    b[7] = gree_checksum(&b) << 4;
    b
}

/// 10 + the low nibbles of bytes 0-3 + the high nibbles of bytes 4-6, mod 16.
pub fn gree_checksum(b: &[u8; 8]) -> u8 {
    let low: u32 = b[..4].iter().map(|x| (x & 0x0F) as u32).sum();
    let high: u32 = b[4..7].iter().map(|x| (x >> 4) as u32).sum();
    ((10 + low + high) & 0x0F) as u8
}

pub fn gree(state: &[u8; 8]) -> Pattern {
    let mut p = vec![GREE.hdr_mark, GREE.hdr_space];
    for &byte in &state[..4] {
        push_byte(&mut p, &GREE, byte, false);
    }
    // Block footer: 0b010, LSB first.
    for bit in [0, 1, 0] {
        p.push(GREE.bit_mark);
        p.push(if bit == 1 { GREE.one } else { GREE.zero });
    }
    p.push(GREE.bit_mark);
    p.push(GREE_MSG_SPACE);
    for &byte in &state[4..] {
        push_byte(&mut p, &GREE, byte, false);
    }
    p.push(GREE.bit_mark);
    p.push(GREE_MSG_SPACE);
    Pattern { carrier: 38_000, pulses: p }
}

// ---------------------------------------------------------------------------------------------
// TCL112 (14 bytes, LSB first, byte-sum checksum)

const TCL: Timing = Timing { hdr_mark: 3000, hdr_space: 1650, bit_mark: 500, one: 1050, zero: 325 };
const TCL_GAP: u32 = 100_000;

pub fn tcl112_state(s: &AcState) -> [u8; 14] {
    let mut b = [0x23, 0xCB, 0x26, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    b[5] = (s.power as u8) << 2 | 0x20 /* light */;
    b[6] = match s.mode {
        AcMode::Heat => 1,
        AcMode::Dry => 2,
        AcMode::Cool => 3,
        AcMode::Fan => 7,
        AcMode::Auto => 8,
    };
    b[7] = 31 - s.temp.clamp(16, 31);
    let fan = match (s.mode, s.fan) {
        (AcMode::Fan, AcFan::Auto) => 0b101,
        (_, AcFan::Auto) => 0b000,
        (_, AcFan::Low) => 0b010,
        (_, AcFan::Medium) => 0b011,
        (_, AcFan::High) => 0b101,
    };
    let swing = if s.swing { 0b111 } else { 0b000 };
    b[8] = swing << 3 | fan;
    if s.sleep {
        // TCL calls it "quiet" / night: the lowest fan.
        b[8] = swing << 3 | 0b001;
    }
    b[13] = b[..13].iter().fold(0u8, |a, &x| a.wrapping_add(x));
    b
}

pub fn tcl112(state: &[u8; 14]) -> Pattern {
    let mut p = vec![TCL.hdr_mark, TCL.hdr_space];
    for &byte in state {
        push_byte(&mut p, &TCL, byte, false);
    }
    p.push(TCL.bit_mark);
    p.push(TCL_GAP);
    Pattern { carrier: 38_000, pulses: p }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(mode: AcMode, fan: AcFan, temp: u8) -> AcState {
        AcState { power: true, mode, fan, temp, swing: false, sleep: false }
    }

    #[test]
    fn midea_checksums_match_real_remotes() {
        assert_eq!(midea_checksum(0xA182_6FFF_FF00), 0x62);
        assert_eq!(midea_checksum(0xA181_77FF_FF00), 0x70);
    }

    #[test]
    fn midea_cool_low_17_and_30() {
        assert_eq!(midea_state(&on(AcMode::Cool, AcFan::Low, 17)), 0xA188_40FF_FF56);
        assert_eq!(midea_state(&on(AcMode::Cool, AcFan::Low, 30)), 0xA188_4DFF_FF5D);
    }

    #[test]
    fn midea_sends_the_frame_then_its_inverse() {
        let p = midea(0xA188_40FF_FF56);
        assert_eq!(p.pulses.len(), 2 * (2 + 96 + 2));
        assert_eq!(&p.pulses[..2], &[4480, 4480]);
        // First bit of 0xA1 is 1, first bit of its inverse (0x5E) is 0.
        assert_eq!(p.pulses[3], 1680);
        assert_eq!(p.pulses[100 + 3], 560);
    }

    #[test]
    fn coolix_default_state_matches() {
        // On, auto, 25 °C, sensor ignored.
        assert_eq!(coolix_state(&on(AcMode::Auto, AcFan::Auto, 25)), 0xB2_1FC8);
        assert_eq!(coolix_state(&AcState::default()), COOLIX_OFF);
    }

    #[test]
    fn coolix_frames_have_inverted_bytes_and_repeat() {
        let p = coolix(0xB2_1FC8);
        assert_eq!(p.pulses.len(), 2 * (2 + 96 + 2));
    }

    #[test]
    fn gree_matches_a_real_state() {
        // IRremoteESP8266: power on, cool, 21 °C, fan 2, light off, turbo, sleep, x-fan
        // gives A9 05 D0 50 00 20 00 A0. We keep the light on and no turbo/x-fan,
        // so only bytes 2 and 7 differ.
        let s = AcState { power: true, mode: AcMode::Cool, fan: AcFan::Medium, temp: 21, swing: false, sleep: true };
        let b = gree_state(&s);
        assert_eq!(b[0], 0xA9);
        assert_eq!(b[1], 0x05);
        assert_eq!(b[2], 0x60);
        let mut real = [0xA9, 0x05, 0xD0, 0x50, 0x00, 0x20, 0x00, 0x00];
        assert_eq!(gree_checksum(&real), 0x0A);
        real[7] = 0xA0;
        assert_eq!(gree(&real).pulses.len(), 2 + 64 + 6 + 2 + 64 + 2);
    }

    #[test]
    fn tcl_cool_24() {
        let b = tcl112_state(&on(AcMode::Cool, AcFan::Auto, 24));
        assert_eq!(&b[..9], &[0x23, 0xCB, 0x26, 0x01, 0x00, 0x24, 0x03, 0x07, 0x00]);
        assert_eq!(b[13], b[..13].iter().fold(0u8, |a, &x| a.wrapping_add(x)));
    }

    #[test]
    fn every_frame_fits_the_blaster() {
        // Android's ConsumerIrManager refuses patterns longer than 2 seconds.
        let s = on(AcMode::Heat, AcFan::High, 26);
        for p in AcProtocol::ALL {
            for c in [AcChange::State, AcChange::Swing, AcChange::Sleep] {
                for f in p.frames(&s, c) {
                    assert!(f.pulses.iter().sum::<u32>() < 2_000_000);
                    assert_eq!(f.pulses.len() % 2, 0);
                }
            }
        }
    }

    #[test]
    fn panel_cycles() {
        assert_eq!(AcMode::Cool.next(), AcMode::Fan);
        assert_eq!(AcMode::Heat.next(), AcMode::Cool);
        assert_eq!(AcFan::High.next(), AcFan::Low);
        let mut s = AcState { temp: 30, ..Default::default() };
        s.warmer();
        assert_eq!(s.temp, 30);
    }
}
