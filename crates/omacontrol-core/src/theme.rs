//! Omarchy themes, painted the Omacalc way.
//!
//! A theme is only three colors (background, foreground, accent). Every other
//! color of the UI is a mix of the first two, exactly like Omacalc's `CalcButton.qml`:
//!
//! | No. | Role | Mix of the foreground into the background |
//! |:----|:-----|:------------------------------------------|
//! | 1 | key (number) | 5%, with a 13% border |
//! | 2 | key (operator) | 16%, no border |
//! | 3 | divider | 16% |
//! | 4 | muted text | 50% |
//! | 5 | primary key (`OK`, `=`) | inverted: foreground fill, background text |

use crate::themes_gen::BUILT_IN;

/// An RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Parses `#rrggbb` (case-insensitive).
    pub fn hex(s: &str) -> Option<Rgb> {
        let s = s.strip_prefix('#')?;
        if s.len() != 6 {
            return None;
        }
        let v = u32::from_str_radix(s, 16).ok()?;
        Some(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }

    /// Mixes `tint` into `self` by `amount` (0.0 to 1.0), like Omacalc's `mixColors`.
    pub fn mix(self, tint: Rgb, amount: f32) -> Rgb {
        let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * amount).round() as u8;
        Rgb(m(self.0, tint.0), m(self.1, tint.1), m(self.2, tint.2))
    }

    /// `0xAARRGGBB`, the format Slint's `Color::from_argb_encoded` takes.
    pub fn argb(self) -> u32 {
        0xff00_0000 | (self.0 as u32) << 16 | (self.1 as u32) << 8 | self.2 as u32
    }
}

/// One Omarchy theme.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: String,
    pub background: Rgb,
    pub foreground: Rgb,
    pub accent: Rgb,
    pub dark: bool,
}

impl Theme {
    /// Every theme that ships with Omarchy, sorted by name.
    pub fn all() -> Vec<Theme> {
        BUILT_IN
            .iter()
            .map(|(name, bg, fg, ac, dark)| Theme {
                name: name.to_string(),
                background: Rgb::hex(bg).expect("valid background"),
                foreground: Rgb::hex(fg).expect("valid foreground"),
                accent: Rgb::hex(ac).expect("valid accent"),
                dark: *dark,
            })
            .collect()
    }

    /// Finds a theme by name, falling back to Tokyo Night (Omarchy's default).
    pub fn named(name: &str) -> Theme {
        let all = Theme::all();
        all.iter()
            .find(|t| t.name.eq_ignore_ascii_case(name))
            .or_else(|| all.iter().find(|t| t.name == "Tokyo Night"))
            .cloned()
            .expect("Tokyo Night is built in")
    }

    pub fn key(&self) -> Rgb {
        self.background.mix(self.foreground, 0.05)
    }
    pub fn key_border(&self) -> Rgb {
        self.background.mix(self.foreground, 0.13)
    }
    pub fn operator(&self) -> Rgb {
        self.background.mix(self.foreground, 0.16)
    }
    pub fn divider(&self) -> Rgb {
        self.background.mix(self.foreground, 0.16)
    }
    pub fn muted(&self) -> Rgb {
        self.background.mix(self.foreground, 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokyo_night_is_the_default() {
        let t = Theme::named("does not exist");
        assert_eq!(t.name, "Tokyo Night");
        assert_eq!(t.background, Rgb(0x1a, 0x1b, 0x26));
    }

    #[test]
    fn mixing_matches_omacalc() {
        let black = Rgb(0, 0, 0);
        let white = Rgb(255, 255, 255);
        assert_eq!(black.mix(white, 0.5), Rgb(128, 128, 128));
        assert_eq!(black.mix(white, 0.0), black);
        assert_eq!(white.argb(), 0xffffffff);
    }

    #[test]
    fn every_theme_parses() {
        assert!(Theme::all().len() >= 20);
    }
}
