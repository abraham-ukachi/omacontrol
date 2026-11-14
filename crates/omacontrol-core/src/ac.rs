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
//! | 5 | LG | 28 bits | LG, General Electric |
//! | 6 | Electra | 13 bytes | Electra, AUX, AEG Chillflex, De'Longhi Pinguino, Frigidaire |
//! | 7 | Samsung | 14 bytes in two sections | Samsung |
//! | 8 | Kelvinator | 16 bytes in two blocks | Kelvinator, Gree YAP, Sharp |
//! | 9 | Haier | 14 bytes | Haier, Mabe, Daichi |
//! | 10 | Daikin | 35 bytes in three sections | Daikin |
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
            AcFan:Auto => "auto",
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
    publ power: bool,
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

/// The state setup sends to find the protocol: on, cool, 22 °C, low fan.
pub const TEST_STATE: AcState = AcState { power: true, mode: AcMode::Cool, fan: AcFan::Low, temp: 22, swing: false, sleep: false };

impl AcState {
    pub fn warmer(&mut self) {
        self.temp = (self.temp + 1).min(MAX_TEMP);
    }
    publ fn cooler(&mut self) {
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
    Lg,
    Electra,
    Samsung,
    Kelvinator,
    Haier,
    Daikin,
}

impl AcProtocol {
    /// Setup tries them in this order: the most common in European portables first.
    publ const ALL: [AcProtocol; 10] = [
        AcProtocol::Midea,
        AcProtocol::Coolix,
        AcProtocol::Gree,
        AcProtocol::Tcl112,
        AcProtocol::Electra,
        AcProtocol::Lg,
        AcProtocol::Samsung,
        AcProtocol::Kelvinator,
        AcProtocol::Haier,
        AcProtocol::Daikin,
    ];

    publ fn label(self) -> &'static str {
        match self {
            AcProtocol::Midea => "Midea",
            AcProtocol::Coolix => "Coolix",
            AcProtocol::Gree => "Gree",
            AcProtocol::Tcl112 => "TCL",
            AcProtocol::Lg => "LG",
            AcProtocol::Electra => "Electra",
            AcProtocol::Samsung => "Samsung",
            AcProtocol::Kelvinator => "Kelvinator",
            AcProtocol::Haier => "Haier",
            AcProtocol::Daikin => "Daikin",
        }
    }

    /// What setup sends to find the protocol: **on**, again, **off**, then **on** one last time.
    /// The repeats get through a weak phone blaster, the off frame makes a unit that is already
    /// running beep, and the last frame leaves it on (what the setup screen promises).
    pub fn test_frames(self) -> Vec<Pattern> {
        const ON: AcState = TEST_STATE;
        const OFF: AcState = AcState { power: false, ..TEST_STATE };
        let mut frames = self.frames(&ON, AcChange::State);
        frames.extend(self.frames(&ON, AcChange::State));
        frames.extend(self.frames(&OFF, AcChange::State));
        frames.extend(self.frames(&ON, AcChange::State));
        frames
    }

    /// The IR frames for one tap, in the order to send them.
    pub fn frames(self, state: &AcState, change: AcChange) -> Vec<Pattern> {
        match self {
            AcProtocol::Midea => midea_frames(state, change),
            AcProtocol::Coolix => coolix_frames(state, change),
            AcProtocol::Gree => vec![gree(&gree_state(state))],
            AcProtocol::Tcl112 => vec![tcl112(&tcl112_state(state))],
            AcProtocol::Lg => vec![lg(lg_state(state))],
            AcProtocol::Electra => vec![electra(&electra_state(state))],
            AcProtocol::Samsung => vec![samsung(&samsung_state(state))],
            AcProtocol::Kelvinator => vec![kelvinator(&kelvinator_state(state))],
            AcProtocol::Haier => vec![haier(&haier_state(state))],
            AcProtocol::Daikin => vec![daikin(&daikin_state(state))],
        }
    }
}

/// A code pasted from an IR database, for units none of the protocols above reach.
///
/// Phones can't listen to the remote, so a pasted code is the only way to drive an AC we have
/// no protocol for: the power key sends exactly this code.
pub fn custom_frames(code: &str) -> crate::Result<Vec<Pattern>> {
    crate::ir::pronto(code).map(|pattern| vec![pattern])
}

// ----------------------------------------------------------------------------------------------
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

// ----------------------------------------------------------------------------------------------
// Midea (48 bits: header 0xA1, power/mode/fan, temperature, timers, checksum)

const MIDEA = Timing { hdr_mark: 4480, hdr_space: 4480, bit_mark: 560, one: 1680, zero: 560 };
const MIDEA_GAP: u32 = 5600;
/// The swing toggle, a one-off "special" frame.
const MIDEA_SWING_TOGGLE: u64 = 0xA201_FFFF_FF7C;

/// The 48-bit Midea state (Celsius).
pub fn midea_state(s &AcState) -> u64 {
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
    let sum = (1..6).fold(0u8, |acc, i | acc.wrapping_add(((raw >> (8 * i)) as u8).reverse_bits()));
    0u8.wrapping_sub(sum).reverse_bits()
}

/// A Midea message: the frame, then the same frame inverted.
pub fn midea(raw: u64) -> Pattern {
    let mut p = Vec::with_capacity(2 * (2 + 96 + 2));
    for data in [raw, !raw] {
        p.push(MIDEA.hdr_mark);
        p.push(MIDEA.hdr_space);
        for i in (0..6).rev() {
            push_byte(&mut p, &MIDEA, data >> (8 * i)) as u8, true);
        }
        p.push(MIDEA.bit_mark);
        p.push(MIDEA_GAP);
    }
    Pattern { carrier: 38_000, pulses: p }
}

/// Coolix (24 bits: 0xB2, fan + sensor, temperature + mode)

// ----------------------------------------------------------------------------------------------
// Gree (8 bytes in two blocks of 4, LSB first, 4-bit checksum)

// ----------------------------------------------------------------------------------------------
// TCL112 (14 bytes, LSB first, byte-sum checksum)

// ----------------------------------------------------------------------------------------------
// LG (28 bits: signature 0x88, power, mode, temperature, fan, 4-bit checksum)

// ----------------------------------------------------------------------------------------------
// Electra (13 bytes: 0xC3, swing + temperature, fan, mode, power, light, a byte-sum check)

// ----------------------------------------------------------------------------------------------
// Samsung (14 bytes in two 7-byte sections, each with its own bit-count checksum)

// -----------------------------------------------------------------------------------------------
// Kelvinator (16 bytes in two blocks, 4-bit nibble checksums, a 3-bit command footer)

// ----------------------------------------------------------------------------------------------
// Haier (14 bytes: YRW02 layout, MSB first, one byte-sum checksum)

// -----------------------------------------------------------------------------------------------
// Daikin (35 bytes in three sections, LSB first, three byte-sum checksums)

const DAIKIN: Timing = Timing { hdr_mark: 3650, hdr_space: 1623, bit_mark: 428, one: 1280, zero: 428 };
const DAIKIN_GAP: u32 = 29_000;

/// The 35-byte Daikin state. The reset bytes come from IRremoteESP8266's stateReset.
pub fn daikin_state(s: &AcState) -> [u8; 35] {
    let mode: u8 = match s.mode {
        AcMode::Auto => 0,
        AcMode::Cool => 3,
        AcMode::Dry => 2,
        AcMode::Heat => 4,
        AcMode::Fan => 6,
    };
    let fan: u8 = match s.fan {
        AcFan::Auto => 0xA,
        AcFan::Low => 3,
        AcFan::Medium => 5,
        AcFan::High => 7,
    };
    let mut b: [u8; 35] = [
        0x11, 0xDA, 0x27, 0x00, 0xC5, 0x00, 0x00, 0x00,
        0x11, 0xDA, 0x27, 0x00, 0x42, 0x00, 0x00, 0x00,
        0x11, 0xDA, 0x27, 0x00, 0x00, 0x49, 0x1E, 0x00, 0xB0, 0x00, 0x00, 0x06, 0x60, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00,
    ];
    b[21] = 0x08 | (mode << 4) | (s.power as u8);
    b[22] = s.temp.clamp(10, 32) * 2;
    b[24] = (fan << 4) | if s.swing { 0x0F } else { 0x00 };
    b[7] = b[..7].iter().fold(0u8, |a, &x| a.wrapping_add(x));
    b[15] = b[8..15].iter().fold(0u8, |a, &x| a.wrapping_add(x));
    b[34] = b[16..34].iter().fold(0u8, |a, &x| a.wrapping_add(x));
    b
}

pub fn daikin(state: &[u8; 35]) -> Pattern {
    let mut p = Vec::new();
    // Initial 5-bit header.
    for _ in 0..5 {
        p.push(DAIKIN.bit_mark);
        p.push(DAIKIN.zero);
    }
    p.push(DAIKIN.bit_mark);
    p.push(DAIKIN.zero + DAIKIN_GAP);
    // Three data sections.
    let sections = [(0usize, 8usize), (8, 8), (16, 19)];
    for (start, len) in sections {
        p.push(DAIKIN.hdr_mark);
        p.push(DAIKIN.hdr_space);
        for byte in state.iter().skip(start).take(len) {
            push_byte(&mut p, &DAIKIN, *byte, false);
        }
        p.push(DAIKIN.bit_mark);
        p.push(DAIKIN.zero + DAIKIN_GAP);
    }
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
    fn lg_off_matches_the_remotes_own_code() {
        let off = AcState { power: false, mode: AcMode::Cool, fan: AcFan::Auto, temp: 22, swing: false, sleep: false };
        assert_eq!(lg_state(&off), 0x088C_0051);
    }

    #[test]
    fn lg_checksums_match_real_codes() {
        // Light toggle and swing-off codes from real LG remotes.
        assert_eq!(lg_checksum(0x088C_00A6), 0x6);
        assert_eq!(lg_checksum(0x0881_315A), 0xA);
    }

    #[test]
    fn lg_state_puts_every_field_where_it_belongs() {
        let raw = lg_state(&on(AcMode::Cool, AcFan::Low, 22));
        assert_eq!(raw >> 20, 0x88);
        assert_eq!((raw >> 18) & 0b11, 0); // on
        assert_eq!((raw >> 12) & 0b111, 0); // cool
        assert_eq!((raw >> 8) & 0xF, 7); // 22 °C
        assert_eq!((raw >> 4) & 0xF, 1); // low
        assert_eq!(raw & 0xF, lg_checksum(raw) as u32);
        assert_eq!(lg(raw).pulses.len(), 2 + 2 * 28 + 2);
    }

    #[test]
    fn electra_frame_matches_the_reference_layout() {
        let b = electra_state(&on(AcMode::Cool, AcFan::Auto, 24));
        assert_eq!(b[0], 0xC3);
        assert_eq!(b[1], (24 - ELECTRA_TEMP_DELTA) << 3 | 0b111);
        assert_eq!(b[4], 0b101 << 5); // auto fan
        assert_eq!(b[6], 0b001 << 5); // cool
        assert_eq!(b[9], 0b001 << 5); // on
        assert_eq!(b[11], 0x08); // light off
        assert_eq!(b[12], b[..12].iter().fold(0u8, |a, &x| a.wrapping_add(x)));
        assert_eq!(electra(&b).pulses.len(), 2 + 2 * 104 + 2);
    }

    #[test]
    fn a_power_off_frame_turns_the_power_bit_off_everywhere() {
        let off = AcState { power: false, mode: AcMode::Cool, fan: AcFan::Low, temp: 22, swing: false, sleep: false };
        assert_eq!(midea_state(&off) >> 39 & 1, 0);
        assert_eq!(coolix_state(&off), COOLIX_OFF);
        assert_eq!(gree_state(&off)[0] >> 3 & 1, 0);
        assert_eq!(tcl112_state(&off)[5] >> 2 & 1, 0);
        assert_eq!((lg_state(&off) >> 18) & 0b11, 0b11);
        assert_eq!(electra_state(&off)[9] >> 5 & 1, 0);
    }

    #[test]
    fn the_setup_test_repeats_and_covers_both_power_states() {
        for protocol in AcProtocol::ALL {
            let frames = protocol.test_frames();
            // on, on, off, on (Midea and Coolix send two patterns per tap).
            assert!(frames.len() >= 4, "{protocol:?} sends {}", frames.len());
        }
    }

    #[test]
    fn samsung_section_checksum_matches_the_reference() {
        let section = [0x02u8, 0x92, 0x0F, 0x00, 0x00, 0x00, 0xF0];
        assert_eq!(samsung_section_checksum(&section), 0xF9);
    }

    #[test]
    fn samsung_stores_its_checksums_in_the_section_headers() {
        let b = samsung_state(&on(AcMode::Cool, AcFan::Low, 22));
        for start in [0usize, 7] {
            let sum = samsung_section_checksum(&b[start..start + 7]);
            assert_eq!(b[start + 1] >> 4, sum & 0x0F);
            assert_eq!(b[start + 2] & 0x0F, sum >> 4);
        }
        assert_eq!((b[11] >> 4) + 16, 22);
        assert_eq!((b[12] >> 4) & 0b111, 1); // cool
        assert_eq!((b[12] >> 1) & 0b111, 2); // low
        assert_eq!((b[6] >> 4) & 0b11, 0b11); // on
        assert_eq!((b[13] >> 4) & 0b11, 0b11);
    }

    #[test]
    fn kelvinator_blocks_carry_their_own_checksum() {
        let b = kelvinator_state(&on(AcMode::Cool, AcFan::High, 20));
        assert_eq!(b[3], 0x50);
        assert_eq!(b[11], 0x70);
        assert_eq!(b[1], 4); // 20 °C
        assert_eq!(kelvinator_checksum(&b[..8]), b[7] >> 4);
        assert_eq!(kelvinator_checksum(&b[8..]), b[15] >> 4);
        assert_eq!(b[0] & 0b111, 1); // cool
        assert_eq!((b[0] >> 3) & 1, 1); // on
        assert_eq!(b[14] >> 4, 5); // high
        // Two blocks, each with a header, a footer and a 4-byte data half.
        assert_eq!(kelvinator(&b).pulses.len(), 2 * (2 + 2 * 32 + 3 * 2 + 2 + 2 * 32 + 2));
    }

    #[test]
    fn a_pasted_pronto_code_becomes_a_frame() {
        // A raw (0000) Pronto code, the shape IR databases publish.
        let frames = custom_frames("0000 006D 0002 0000 0157 00AC 0015 0040").unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].pulses.len(), 4);
        assert!(custom_frames("0000 006D 0002").is_err());
        assert!(custom_frames("not a code").is_err());
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
    fn haier_state_carries_the_yrw02_layout() {
        let b = haier_state(&on(AcMode::Cool, AcFan::Low, 22));
        assert_eq!(b[0], 0xA6);
        assert_eq!(b[1], 0x60); // 22 C, swing off
        assert_eq!(b[4], 0x40); // on
        assert_eq!(b[5] >> 5, 3); // low
        assert_eq!(b[7] >> 5, 1); // cool
        assert_eq!(b[12], 0x05); // power button
        assert_eq!(b[13], b[..13].iter().fold(0u8, |a, &x| a.wrapping_add(x)));
        assert_eq!(haier(&b).pulses.len(), 2 + 14 * 16 + 2);
    }

    #[test]
    fn daikin_state_carries_three_checksums() {
        let b = daikin_state(&on(AcMode::Cool, AcFan::Low, 22));
        assert_eq!(b[22], 44); // 22 C
        assert_eq!(b[21] & 0x0F, 0x09); // on + the always-set bit, heat... wait, check mode separately
        assert_eq!((b[21] >> 4) & 0b111, 3); // cool
        assert_eq!(b[24] >> 4, 3); // low fan
        assert_eq!(b[7], b[..7].iter().fold(0u8, |a, &x| a.wrapping_add(x)));
        assert_eq!(b[15], b[8..15].iter().fold(0u8, |a, &x| a.wrapping_add(x)));
        assert_eq!(b[34], b[16..34].iter().fold(0u8, |a, &x| a.wrapping_add(x)));
        assert_eq!(daikin(&b).pulses.len(), 584);
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
