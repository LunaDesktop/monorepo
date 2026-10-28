//! Skala ekranu: przeliczanie pikseli logicznych na fizyczne i z powrotem.
//!
//! Cały toolkit operuje na pikselach logicznych; [`Scale`] pojawia się dopiero
//! w warstwie rysowania i obsługi wejścia (Wayland przekazuje współrzędne
//! w pikselach logicznych, a bufor w fizycznych).

/// Współczynnik skalowania (np. `2.0` dla HiDPI, `1.0` dla zwykłego ekranu).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale(f64);

impl Scale {
    /// Skala 1:1.
    pub const ONE: Scale = Scale(1.0);

    pub const fn new(factor: f64) -> Self {
        Self(factor)
    }

    /// Ze skali całkowitej — typowe wartości to 1, 2, 3.
    pub const fn integer(factor: i32) -> Self {
        Self(factor as f64)
    }

    pub const fn factor(&self) -> f64 {
        self.0
    }

    pub fn to_physical(&self, logical: f64) -> f64 {
        logical * self.0
    }

    pub fn to_logical(&self, physical: f64) -> f64 {
        physical / self.0
    }

    /// Rozmiar fizyczny (zaokrąglony w górę), przydatny do alokacji bufora.
    pub fn physical_size(&self, width: f64, height: f64) -> (u32, u32) {
        (
            self.to_physical(width).ceil().max(0.0) as u32,
            self.to_physical(height).ceil().max(0.0) as u32,
        )
    }
}

impl Default for Scale {
    fn default() -> Self {
        Scale::ONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_roundtrip() {
        let s = Scale::integer(2);
        assert_eq!(s.to_physical(10.0), 20.0);
        assert_eq!(s.to_logical(20.0), 10.0);
    }

    #[test]
    fn physical_size_rounds_up() {
        let s = Scale::new(1.5);
        assert_eq!(s.physical_size(10.0, 3.0), (15, 5));
        let s = Scale::new(1.25);
        assert_eq!(s.physical_size(10.0, 10.0), (13, 13));
    }
}
