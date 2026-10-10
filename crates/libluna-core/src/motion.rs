//! `motion` — animacja i interpolacja (własny, lekki mechanizm libLuny).
//!
//! To nie jest klon `GtkStack`/`AdwAnimation`: libLuna daje tylko **matematykę ruchu**
//! (krzywe [`Easing`] i interpolację [`lerp`]), a stan animacji trzyma aplikacja — zgodnie
//! z modelem immediate. Dzięki temu przejścia w `Stack`, odsłanianie `Revealer` czy
//! pojawianie się `Toast` płynnie sterują się prostą postępu `t` (0..1).

/// Krzywa wygładzania (easing) przebiegu animacji.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Easing {
    /// Liniowo (bez wygładzania).
    Linear,
    /// Wolny start (przyspiesza).
    #[default]
    EaseIn,
    /// Szybki start (zwalnia pod koniec).
    EaseOut,
    /// Wolny start i koniec (najbardziej „naturalny”).
    EaseInOut,
}

impl Easing {
    /// Mapuje postęp liniowy `t` (0..1) na wygładzoną wartość (0..1).
    pub fn apply(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t,
            Easing::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
        }
    }

    /// Interpoluje `from` → `to` dla postępu `t` (z wygładzeniem tej krzywej).
    pub fn lerp(self, from: f64, to: f64, t: f64) -> f64 {
        from + (to - from) * self.apply(t)
    }
}

/// Interpolacja liniowa `from` → `to` (bez wygładzania).
pub fn lerp(from: f64, to: f64, t: f64) -> f64 {
    from + (to - from) * t.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_are_preserved() {
        for e in [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
        ] {
            assert!((e.apply(0.0)).abs() < 1e-9);
            assert!((e.apply(1.0) - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn ease_in_lags_and_ease_out_leads_at_midpoint() {
        assert!(Easing::EaseIn.apply(0.5) < 0.5);
        assert!(Easing::EaseOut.apply(0.5) > 0.5);
        assert!((Easing::EaseInOut.apply(0.5) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn progress_is_clamped() {
        assert_eq!(Easing::Linear.apply(-1.0), 0.0);
        assert_eq!(Easing::Linear.apply(2.0), 1.0);
        assert_eq!(lerp(10.0, 20.0, 5.0), 20.0);
    }

    #[test]
    fn lerp_moves_between_values() {
        assert_eq!(lerp(0.0, 100.0, 0.25), 25.0);
        assert_eq!(Easing::Linear.lerp(10.0, 20.0, 0.5), 15.0);
    }
}
