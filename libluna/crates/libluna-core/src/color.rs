//! Kolor w przestrzeni sRGB z kanałem alfa.
//!
//! Kanały są przechowywane jako `f32` w zakresie `0.0..=1.0`, **bez premnożenia**
//! (non-premultiplied). Premnożenie, jeśli backend go wymaga, robi warstwa rysowania.

/// Kolor RGBA w sRGB, kanały `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
    pub const BLACK: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const WHITE: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    /// Nieprzezroczysty kolor z kanałów `0.0..=1.0`.
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Kolor z kanałów `0.0..=1.0`, z jawną alfą.
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Z kanałów 8-bitowych (np. `#3584e4` → `from_rgba8(0x35, 0x84, 0xe4, 255)`).
    pub const fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Zapis do kanałów 8-bitowych, z przycięciem do zakresu.
    pub fn to_rgba8(&self) -> [u8; 4] {
        [to_u8(self.r), to_u8(self.g), to_u8(self.b), to_u8(self.a)]
    }

    /// Ten sam kolor z inną alfą.
    pub const fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// Ten sam kolor (RGB) z alfą pomnożoną przez `factor` (0.0..=1.0 zostaje).
    pub const fn scale_alpha(self, factor: f32) -> Self {
        Self {
            a: self.a * factor,
            ..self
        }
    }

    /// Liniowa interpolacja kanałów (`t = 0` → `self`, `t = 1` → `other`).
    ///
    /// Interpolacja jest w sRGB (wystarczająca dla animacji UI); docelowo można
    /// przełączyć na przestrzeń liniową.
    pub fn lerp(self, other: Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Color {
            r: mix(self.r, other.r),
            g: mix(self.g, other.g),
            b: mix(self.b, other.b),
            a: mix(self.a, other.a),
        }
    }

    /// Kolor w formacie `#rrggbb`, `#rrggbbaa`, `#rgb` lub `#rgba`.
    pub fn from_hex(hex: &str) -> Option<Color> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        if !hex.is_ascii() {
            return None;
        }
        let byte = |s: &str| u8::from_str_radix(s, 16).ok();
        match hex.len() {
            3 => {
                let r = byte(&hex[0..1])?;
                let g = byte(&hex[1..2])?;
                let b = byte(&hex[2..3])?;
                Some(Color::from_rgba8(r * 17, g * 17, b * 17, 255))
            }
            4 => {
                let r = byte(&hex[0..1])?;
                let g = byte(&hex[1..2])?;
                let b = byte(&hex[2..3])?;
                let a = byte(&hex[3..4])?;
                Some(Color::from_rgba8(r * 17, g * 17, b * 17, a * 17))
            }
            6 => {
                let r = byte(&hex[0..2])?;
                let g = byte(&hex[2..4])?;
                let b = byte(&hex[4..6])?;
                Some(Color::from_rgba8(r, g, b, 255))
            }
            8 => {
                let r = byte(&hex[0..2])?;
                let g = byte(&hex[2..4])?;
                let b = byte(&hex[4..6])?;
                let a = byte(&hex[6..8])?;
                Some(Color::from_rgba8(r, g, b, a))
            }
            _ => None,
        }
    }
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba8_roundtrip() {
        let c = Color::from_rgba8(0x35, 0x84, 0xe4, 0xff);
        assert_eq!(c.to_rgba8(), [0x35, 0x84, 0xe4, 0xff]);
    }

    #[test]
    fn hex_parsing() {
        assert_eq!(Color::from_hex("#ffffff"), Some(Color::WHITE));
        assert_eq!(
            Color::from_hex("3584e4"),
            Some(Color::from_rgba8(0x35, 0x84, 0xe4, 255))
        );
        assert_eq!(Color::from_hex("#fff"), Some(Color::WHITE));
        assert_eq!(
            Color::from_hex("#00000080"),
            Some(Color::from_rgba8(0, 0, 0, 0x80))
        );
        assert_eq!(Color::from_hex("#12345"), None);
        assert_eq!(Color::from_hex("nie-hex"), None);
    }

    #[test]
    fn lerp_midpoint() {
        let m = Color::BLACK.lerp(Color::WHITE, 0.5);
        assert!((m.r - 0.5).abs() < 1e-6);
        assert!((m.a - 1.0).abs() < 1e-6);
    }

    #[test]
    fn lerp_clamps_t() {
        assert_eq!(Color::BLACK.lerp(Color::WHITE, -1.0), Color::BLACK);
        assert_eq!(Color::BLACK.lerp(Color::WHITE, 2.0), Color::WHITE);
    }

    #[test]
    fn alpha_helpers() {
        let c = Color::WHITE.scale_alpha(0.5).with_alpha(0.25);
        assert!((c.a - 0.25).abs() < 1e-6);
    }
}
