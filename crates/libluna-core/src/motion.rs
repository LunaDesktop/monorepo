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

/// Animowany skalar: płynnie przechodzi do celu `to` w zadanym czasie.
///
/// Sterowany z aplikacji: woła się [`Animation::advance`] o `dt` (sekundy) i bieżący czas
/// trwania, a wartość odczytuje przez [`Animation::value`]. Zero zależności od pętli zdarzeń.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Animation {
    from: f64,
    to: f64,
    progress: f64,
    easing: Easing,
}

impl Default for Animation {
    fn default() -> Self {
        Self {
            from: 0.0,
            to: 0.0,
            progress: 1.0,
            easing: Easing::EaseInOut,
        }
    }
}

impl Animation {
    /// Animacja z podaną krzywą (stan początkowy: 0, bez ruchu).
    pub fn new(easing: Easing) -> Self {
        Self {
            easing,
            ..Self::default()
        }
    }

    /// Bieżąca (wyświetlana) wartość.
    pub fn value(&self) -> f64 {
        self.easing.lerp(self.from, self.to, self.progress)
    }

    /// Wartość docelowa.
    pub fn target(&self) -> f64 {
        self.to
    }

    pub fn progress(&self) -> f64 {
        self.progress
    }

    pub fn easing(&self) -> Easing {
        self.easing
    }

    pub fn set_easing(&mut self, easing: Easing) {
        self.easing = easing;
    }

    /// Rozpoczyna płynne przejście do `to` (od bieżącej wartości).
    pub fn animate_to(&mut self, to: f64) {
        self.from = self.value();
        self.to = to;
        self.progress = 0.0;
    }

    /// Ustawia wartość natychmiast (bez animacji).
    pub fn jump_to(&mut self, value: f64) {
        self.from = value;
        self.to = value;
        self.progress = 1.0;
    }

    pub fn is_animating(&self) -> bool {
        self.progress < 1.0
    }

    /// Postęp animacji z krokiem `dt` przy czasie `duration`; zwraca `true`, gdy trwa dalej.
    pub fn advance(&mut self, dt: f64, duration: f64) -> bool {
        if !self.is_animating() {
            return false;
        }
        if duration <= 0.0 {
            self.progress = 1.0;
        } else {
            self.progress = (self.progress + dt / duration).min(1.0);
        }
        self.is_animating()
    }
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

    #[test]
    fn animation_reaches_target_and_stops() {
        let mut a = Animation::new(Easing::Linear);
        assert!(!a.is_animating());
        assert_eq!(a.value(), 0.0);
        a.animate_to(1.0);
        assert!(a.is_animating());
        assert!(a.advance(0.5, 1.0)); // połowa czasu
        assert!((a.value() - 0.5).abs() < 1e-9);
        assert!(!a.advance(0.5, 1.0)); // koniec
        assert_eq!(a.value(), 1.0);
        assert!(!a.is_animating());
    }

    #[test]
    fn animation_jump_is_instant() {
        let mut a = Animation::new(Easing::EaseInOut);
        a.jump_to(0.75);
        assert_eq!(a.value(), 0.75);
        assert!(!a.is_animating());
        a.animate_to(0.0);
        assert_eq!(a.value(), 0.75); // start z bieżącej wartości
    }

    #[test]
    fn animation_zero_duration_completes() {
        let mut a = Animation::new(Easing::Linear);
        a.animate_to(5.0);
        // Zerowy czas trwania: pierwszy krok domyka animację.
        assert!(!a.advance(0.0, 0.0));
        assert_eq!(a.value(), 5.0);
    }
}
