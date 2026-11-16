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

/// The state setup sends to find the protocol: on, cool, 22 °C, low fan.
pub const TEST_STATE: AcState = AcState { power: true, mode: AcMode::Cool, fan: AcFan::Low, temp: 22, swing: false, sleep: false };

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
    Lg,
    Electra,
    Samsung,
    Kelvinator,
    Haier,
    Daikin,
}

impl AcProtocol {
    /// Setup tries them in this order: the most common in European portables first.
    pub const ALL: [AcProtocol; 10] = [
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

    pub fn label(self) -> &'static str {
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

/// A custom IR code, pasted as Pronto or captured by the manual NEC scan.
///
/// Phones can't listen to the remote, so a pasted code is the only way to drive an AC we have
/// no protocol for: the power key sends exactly this code.
pub fn custom_frames(code: &str) -> crate::Result<Vec<Pattern>> {
    if let Some(hex) = code.strip_prefix("NEC ") {
        let value = u32::from_str_radix(hex, 16)
            .map_err(|_| crate::Error::Protocol("not a NEC code".into()))?;
        let [address, inverted_address, command, inverted_command] = value.to_be_bytes();
        if inverted_address != !address || inverted_command != !command {
            return Err(crate::Error::Protocol("invalid NEC complement bytes".into()));
        }
        return Ok(vec![crate::ir::nec(address, command)]);
    }
    crate::ir::pronto(code).map(|pattern| vec![pattern])
}

/// The address and command of a `NEC XXXXXXXX` code, when its complement bytes check out.
pub fn nec_parts(code: &str) -> Option<(u8, u8)> {
    let value = u32::from_str_radix(code.strip_prefix("NEC ")?, 16).ok()?;
    let [address, inverted_address, command, inverted_command] = value.to_be_bytes();
    (inverted_address == !address && inverted_command == !command).then_some((address, command))
}

/// The keys of the AC remote that can be learned once the power key is a NEC code.
pub const NEC_LEARNABLE_KEYS: [&str; 6] = ["mode", "fan", "cooler", "warmer", "sleep", "swing"];

/// Candidates for learning one more key of a per-key NEC remote: every other command on the
/// power key's address, starting just after the power command (remotes number their keys
/// close together), skipping commands already taken by other keys.
pub fn nec_learn_candidates(address: u8, power: u8, known: &[u8]) -> Vec<(u8, u8)> {
    (1..=u8::MAX)
        .map(|offset| power.wrapping_add(offset))
        .filter(|command| !known.contains(command))
        .map(|command| (address, command))
        .collect()
}

/// A deterministic, bounded NEC address/command sweep for manual AC discovery.
///
/// The first candidate is the reported `00FF40BF` code. The remaining candidates cover
/// every command byte at addresses `00` and `04`, then the first 163 commands at `08`.
/// This is an OmaControl-defined search set, not a reproduction of another app's database.
pub const NEC_AUTOTEST_CANDIDATE_COUNT: usize = 675;

pub fn nec_autotest_candidates() -> Vec<(u8, u8)> {
    let mut candidates = Vec::with_capacity(NEC_AUTOTEST_CANDIDATE_COUNT);
    candidates.push((0x00, 0x40));
    for address in [0x00, 0x04, 0x08] {
        for command in 0..=u8::MAX {
            if (address, command) == (0x00, 0x40) {
                continue;
            }
            candidates.push((address, command));
            if candidates.len() == NEC_AUTOTEST_CANDIDATE_COUNT {
                return candidates;
            }
        }
    }
    candidates
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

// ---------------------------------------------------------------------------------------------
// LG (28 bits: signature 0x88, power, mode, temperature, fan, 4-bit checksum)

const LG: Timing = Timing { hdr_mark: 8500, hdr_space: 4250, bit_mark: 550, one: 1600, zero: 550 };
const LG_GAP: u32 = 39_750;

/// The 28-bit LG state. "Off" is its own code (`0x88C0051`), with the temperature cleared.
pub fn lg_state(s: &AcState) -> u32 {
    let mode: u32 = match s.mode {
        AcMode::Cool => 0,
        AcMode::Dry => 1,
        AcMode::Fan => 2,
        AcMode::Auto => 3,
        AcMode::Heat => 4,
    };
    let fan: u32 = match s.fan {
        AcFan::Auto => 5,
        AcFan::Low => 1,
        AcFan::Medium => 2,
        AcFan::High => 4,
    };
    // The temperature field counts up from 16 °C as 1, and "off" clears it.
    let (power, temp) = if s.power { (0u32, (s.temp.clamp(16, 30) - 15) as u32) } else { (3u32, 0) };
    let raw = 0x88u32 << 20 | power << 18 | mode << 12 | temp << 8 | fan << 4;
    raw | lg_checksum(raw) as u32
}

/// Sum of the six nibbles above the checksum, mod 16.
pub fn lg_checksum(raw: u32) -> u8 {
    (1..=6).fold(0u8, |acc, i| acc.wrapping_add(((raw >> (4 * i)) & 0xF) as u8)) & 0x0F
}

pub fn lg(raw: u32) -> Pattern {
    let mut p = vec![LG.hdr_mark, LG.hdr_space];
    for i in (0..28).rev() {
        p.push(LG.bit_mark);
        p.push(if raw >> i & 1 == 1 { LG.one } else { LG.zero });
    }
    p.push(LG.bit_mark);
    p.push(LG_GAP);
    Pattern { carrier: 38_000, pulses: p }
}

// ---------------------------------------------------------------------------------------------
// Electra (13 bytes: 0xC3, swing + temperature, fan, mode, power, light, a byte-sum check)

const ELECTRA: Timing = Timing { hdr_mark: 9166, hdr_space: 4470, bit_mark: 646, one: 1647, zero: 547 };
const ELECTRA_GAP: u32 = 100_000;
/// Electra stores the temperature 8 °C below its real value.
const ELECTRA_TEMP_DELTA: u8 = 8;

pub fn electra_state(s: &AcState) -> [u8; 13] {
    let mode = match s.mode {
        AcMode::Auto => 0b000,
        AcMode::Cool => 0b001,
        AcMode::Dry => 0b010,
        AcMode::Heat => 0b100,
        AcMode::Fan => 0b110,
    };
    let fan = match s.fan {
        AcFan::Auto => 0b101,
        AcFan::Low => 0b011,
        AcFan::Medium => 0b010,
        AcFan::High => 0b001,
    };
    let mut b = [0u8; 13];
    b[0] = 0xC3;
    // Vertical swing sits in the low bits: "000" swings, "111" parks the vane.
    b[1] = ((s.temp.clamp(16, 32) - ELECTRA_TEMP_DELTA) << 3) | if s.swing { 0b000 } else { 0b111 };
    b[4] = fan << 5;
    b[5] = (s.sleep as u8) << 7; // quiet
    b[6] = mode << 5;
    b[9] = (s.power as u8) << 5;
    // The remote only toggles the display light, so we keep it off.
    b[11] = 0x08;
    b[12] = b[..12].iter().fold(0u8, |a, &x| a.wrapping_add(x));
    b
}

pub fn electra(state: &[u8; 13]) -> Pattern {
    let mut p = vec![ELECTRA.hdr_mark, ELECTRA.hdr_space];
    for &byte in state {
        push_byte(&mut p, &ELECTRA, byte, false);
    }
    p.push(ELECTRA.bit_mark);
    p.push(ELECTRA_GAP);
    Pattern { carrier: 38_000, pulses: p }
}

// ---------------------------------------------------------------------------------------------
// Samsung (14 bytes in two 7-byte sections, each with its own bit-count checksum)

const SAMSUNG: Timing = Timing { hdr_mark: 690, hdr_space: 17_844, bit_mark: 586, one: 1432, zero: 436 };
const SAMSUNG_SECTION_MARK: u32 = 3086;
const SAMSUNG_SECTION_SPACE: u32 = 8864;
const SAMSUNG_SECTION_GAP: u32 = 2886;
const SAMSUNG_GAP: u32 = 100_000;

pub fn samsung_state(s: &AcState) -> [u8; 14] {
    let mode = match s.mode {
        AcMode::Auto => 0,
        AcMode::Cool => 1,
        AcMode::Dry => 2,
        AcMode::Fan => 3,
        AcMode::Heat => 4,
    };
    let fan = match s.fan {
        AcFan::Auto => 0,
        AcFan::Low => 2,
        AcFan::Medium => 4,
        AcFan::High => 5,
    };
    let power: u8 = if s.power { 0b11 } else { 0b00 };
    // The message Samsung's own remote starts from.
    let mut b: [u8; 14] = [0x02, 0x92, 0x0F, 0x00, 0x00, 0x00, 0xF0, 0x01, 0x02, 0xAE, 0x71, 0x00, 0x15, 0xF0];
    b[6] = (b[6] & 0xCF) | power << 4;
    b[13] = (b[13] & 0xCF) | power << 4;
    b[11] = (b[11] & 0x0F) | ((s.temp.clamp(16, 30) - 16) << 4);
    b[12] = fan << 1 | mode << 4;
    for start in [0usize, 7] {
        let sum = samsung_section_checksum(&b[start..start + 7]);
        b[start + 1] = (b[start + 1] & 0x0F) | (sum & 0x0F) << 4;
        b[start + 2] = (b[start + 2] & 0xF0) | (sum >> 4);
    }
    b
}

/// The bit count of a section, inverted: the whole first byte, the low nibble of the second,
/// the high nibble of the third, and the last four bytes.
pub fn samsung_section_checksum(section: &[u8]) -> u8 {
    let mut sum = section[0].count_ones() as u8;
    sum = sum.wrapping_add((section[1] & 0x0F).count_ones() as u8);
    sum = sum.wrapping_add((section[2] >> 4).count_ones() as u8);
    sum = sum.wrapping_add(section[3..].iter().map(|b| b.count_ones() as u8).sum::<u8>());
    !sum
}

pub fn samsung(state: &[u8; 14]) -> Pattern {
    let mut p = vec![SAMSUNG.hdr_mark, SAMSUNG.hdr_space];
    for (i, section) in state.chunks(7).enumerate() {
        p.push(SAMSUNG_SECTION_MARK);
        p.push(SAMSUNG_SECTION_SPACE);
        for &byte in section {
            push_byte(&mut p, &SAMSUNG, byte, false);
        }
        p.push(SAMSUNG.bit_mark);
        p.push(if i == 0 { SAMSUNG_SECTION_GAP } else { SAMSUNG_GAP });
    }
    Pattern { carrier: 38_000, pulses: p }
}

// ---------------------------------------------------------------------------------------------
// Kelvinator (16 bytes in two blocks, 4-bit nibble checksums, a 3-bit command footer)

const KELVINATOR: Timing = Timing { hdr_mark: 9010, hdr_space: 4505, bit_mark: 680, one: 1530, zero: 510 };
const KELVINATOR_GAP: u32 = 19_975;
const KELVINATOR_LONG_GAP: u32 = 39_950;

pub fn kelvinator_state(s: &AcState) -> [u8; 16] {
    let mode = match s.mode {
        AcMode::Auto => 0,
        AcMode::Cool => 1,
        AcMode::Dry => 2,
        AcMode::Fan => 3,
        AcMode::Heat => 4,
    };
    let fan = match s.fan {
        AcFan::Auto => 0,
        AcFan::Low => 1,
        AcFan::Medium => 3,
        AcFan::High => 5,
    };
    // Auto and dry always run at 25 °C, and don't show it.
    let temp = if matches!(s.mode, AcMode::Auto | AcMode::Dry) { 25 } else { s.temp.clamp(16, 30) };
    let mut b = [0u8; 16];
    b[0] = mode | (s.power as u8) << 3 | fan.min(3) << 4 | (s.swing as u8) << 6;
    b[1] = temp - 16;
    b[3] = 0x50;
    b[4] = if s.swing { 0b0001 } else { 0 }; // swing: auto
    b[11] = 0x70;
    b[12] = (s.sleep as u8) << 7; // quiet
    b[14] = fan << 4;
    b[7] = kelvinator_checksum(&b[..8]) << 4;
    b[15] = kelvinator_checksum(&b[8..]) << 4;
    b
}

/// 10, plus the low nibbles of the first four bytes and the high nibbles of the next three,
/// trimmed to four bits.
pub fn kelvinator_checksum(block: &[u8]) -> u8 {
    let low: u32 = block[..4].iter().map(|b| (b & 0x0F) as u32).sum();
    let high: u32 = block[4..7].iter().map(|b| (b >> 4) as u32).sum();
    ((10 + low + high) & 0x0F) as u8
}

pub fn kelvinator(state: &[u8; 16]) -> Pattern {
    let mut p = Vec::new();
    for block in [&state[..8], &state[8..]] {
        p.push(KELVINATOR.hdr_mark);
        p.push(KELVINATOR.hdr_space);
        for &byte in &block[..4] {
            push_byte(&mut p, &KELVINATOR, byte, false);
        }
        // Every command block ends with the three bits 0b010.
        for bit in [0u8, 1, 0] {
            p.push(KELVINATOR.bit_mark);
            p.push(if bit == 1 { KELVINATOR.one } else { KELVINATOR.zero });
        }
        p.push(KELVINATOR.bit_mark);
        p.push(KELVINATOR_GAP);
        for &byte in &block[4..] {
            push_byte(&mut p, &KELVINATOR, byte, false);
        }
        p.push(KELVINATOR.bit_mark);
        p.push(KELVINATOR_LONG_GAP);
    }
    Pattern { carrier: 38_000, pulses: p }
}


// ---------------------------------------------------------------------------------------------
// Haier (14 bytes: YRW02 layout, MSB first, one byte-sum checksum)

const HAIER: Timing = Timing { hdr_mark: 3000, hdr_space: 4300, bit_mark: 520, one: 1650, zero: 650 };
const HAIER_GAP: u32 = 150_000;

/// The 14-byte Haier state (model 0xA6).
pub fn haier_state(s: &AcState) -> [u8; 14] {
    let mode: u8 = match s.mode {
        AcMode::Auto => 0,
        AcMode::Cool => 1,
        AcMode::Dry => 2,
        AcMode::Heat => 4,
        AcMode::Fan => 6,
    };
    let fan: u8 = match s.fan {
        AcFan::High => 1,
        AcFan::Medium => 2,
        AcFan::Low => 3,
        AcFan::Auto => 5,
    };
    let temp = s.temp.clamp(16, 30);
    let mut b = [0u8; 14];
    b[0] = 0xA6;
    b[1] = ((temp - 16) << 4) | if s.swing { 0xC } else { 0x0 };
    b[3] = 0x02; // health on, no timers
    b[4] = (s.power as u8) << 6;
    b[5] = fan << 5;
    b[7] = mode << 5;
    b[8] = (s.sleep as u8) << 7;
    b[12] = 0x05; // power button
    b[13] = b[..13].iter().fold(0u8, |a, &x| a.wrapping_add(x));
    b
}

pub fn haier(state: &[u8; 14]) -> Pattern {
    let mut p = vec![HAIER.hdr_mark, HAIER.hdr_space];
    for &byte in state {
        push_byte(&mut p, &HAIER, byte, true);
    }
    p.push(HAIER.bit_mark);
    p.push(HAIER_GAP);
    Pattern { carrier: 38_000, pulses: p }
}

// ---------------------------------------------------------------------------------------------
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
    fn nec_autotest_has_675_unique_candidates_and_starts_with_reported_code() {
        let candidates = nec_autotest_candidates();
        assert_eq!(candidates.len(), NEC_AUTOTEST_CANDIDATE_COUNT);
        assert_eq!(candidates[0], (0x00, 0x40));
        let unique: std::collections::HashSet<_> = candidates.iter().copied().collect();
        assert_eq!(unique.len(), candidates.len());
        assert_eq!(
            custom_frames("NEC 00FF40BF").unwrap(),
            vec![crate::ir::nec(0x00, 0x40)]
        );
        assert!(custom_frames("NEC 00FF40BE").is_err());
    }

    #[test]
    fn nec_learning_skips_power_and_known_keys() {
        assert_eq!(nec_parts("NEC 00FF40BF"), Some((0x00, 0x40)));
        assert_eq!(nec_parts("NEC 00FF40BE"), None);
        assert_eq!(nec_parts("0000 006D"), None);
        let candidates = nec_learn_candidates(0x00, 0x40, &[0x41]);
        assert_eq!(candidates.len(), 254);
        assert_eq!(candidates[0], (0x00, 0x42));
        assert!(!candidates.iter().any(|&(_, c)| c == 0x40 || c == 0x41));
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
