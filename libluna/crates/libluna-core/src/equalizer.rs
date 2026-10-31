//! Ustawienia korektora graficznego (model + zapis), wspólne dla powłoki i ustawień.
//!
//! Tutaj mieszka **tylko** model i format plików — sterowanie PipeWire (uruchamianie
//! łańcucha filtrów, wybór wyjścia) jest w `luna::equalizer`, bo wymaga `wpctl`.
//!
//! Plik `~/.config/luna/equalizer.conf`:
//!
//! ```text
//! enabled = true
//! preset  = rock
//! preamp  = -1.5
//! band0   = 4.0
//! …
//! band9   = 2.0
//! ```

use crate::config;

/// Liczba pasm korektora.
pub const BANDS: usize = 10;

/// Etykiety pasm (spójne z wtyczką LADSPA `luna-eq`).
/// Nazwy są **identyczne** z portami sterującymi wtyczki LADSPA `luna-eq` —
/// `filter-chain` dopasowuje `control` po nazwie portu, więc rozjazd oznacza
/// ciche zignorowanie ustawienia.
pub const BAND_LABELS: [&str; BANDS] = [
    "31 Hz", "62 Hz", "125 Hz", "250 Hz", "500 Hz", "1 kHz", "2 kHz", "4 kHz", "8 kHz", "16 kHz",
];

/// Maksymalne wzmocnienie pasma i przedwzmacniacza (dB).
pub const MAX_GAIN_DB: f32 = 12.0;

/// Nazwa pliku konfiguracji PipeWire generowanego dla łańcucha korektora.
pub const CHAIN_FILE: &str = "luna-equalizer.conf";

/// Nazwa pliku ustawień w `~/.config/luna`.
pub const CONFIG_FILE: &str = "equalizer.conf";

/// Gotowe krzywe korektora.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Flat,
    Bass,
    Treble,
    Vocal,
    Rock,
    /// Wartości ustawione ręcznie (nie odpowiadają żadnej krzywej).
    Custom,
}

impl Preset {
    /// Krzywe pokazywane w UI (bez `Custom` — to stan wykryty, nie wybór).
    pub const ALL: [Preset; 5] = [
        Preset::Flat,
        Preset::Bass,
        Preset::Vocal,
        Preset::Rock,
        Preset::Treble,
    ];

    /// Nazwa zapisywana w pliku konfiguracji.
    pub fn key(self) -> &'static str {
        match self {
            Preset::Flat => "flat",
            Preset::Bass => "bass",
            Preset::Treble => "treble",
            Preset::Vocal => "vocal",
            Preset::Rock => "rock",
            Preset::Custom => "custom",
        }
    }

    /// Nazwa do zapisu (`Custom` nie ma własnej krzywej).
    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "flat" => Some(Preset::Flat),
            "bass" => Some(Preset::Bass),
            "treble" => Some(Preset::Treble),
            "vocal" => Some(Preset::Vocal),
            "rock" => Some(Preset::Rock),
            "custom" => Some(Preset::Custom),
            _ => None,
        }
    }

    /// Wzmocnienia pasm (dB).
    pub fn bands(self) -> [f32; BANDS] {
        match self {
            Preset::Flat | Preset::Custom => [0.0; BANDS],
            Preset::Bass => [6.0, 5.0, 4.0, 3.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            Preset::Treble => [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 5.0, 6.0],
            Preset::Vocal => [-2.0, -2.0, -1.0, 0.0, 2.0, 3.0, 3.0, 2.0, 0.0, -1.0],
            Preset::Rock => [5.0, 4.0, 2.0, 0.0, -1.0, 0.0, 1.0, 2.0, 3.0, 4.0],
        }
    }
}

/// Stan korektora.
#[derive(Debug, Clone, PartialEq)]
pub struct Equalizer {
    pub enabled: bool,
    pub preset: Preset,
    /// Wzmocnienie przed korekcją (dB) — chroni przed przesterowaniem po podbiciu pasm.
    pub preamp: f32,
    pub bands: [f32; BANDS],
}

impl Default for Equalizer {
    fn default() -> Self {
        Self {
            enabled: false,
            preset: Preset::Flat,
            preamp: 0.0,
            bands: Preset::Flat.bands(),
        }
    }
}

impl Equalizer {
    /// Krzywa, której odpowiadają obecne pasma (albo `Custom`).
    pub fn detected_preset(&self) -> Preset {
        Preset::ALL
            .into_iter()
            .find(|preset| {
                preset
                    .bands()
                    .iter()
                    .zip(self.bands.iter())
                    .all(|(a, b)| (a - b).abs() < 0.05)
            })
            .unwrap_or(Preset::Custom)
    }

    /// Ustawia krzywą i jej pasma (preamp zostaje).
    pub fn set_preset(&mut self, preset: Preset) {
        self.preset = preset;
        if preset != Preset::Custom {
            self.bands = preset.bands();
        }
    }

    /// Ustawia pojedyncze pasmo (dB, przycinane do ±[`MAX_GAIN_DB`]).
    pub fn set_band(&mut self, index: usize, gain_db: f32) {
        if index >= BANDS {
            return;
        }
        self.bands[index] = if gain_db.is_finite() {
            gain_db.clamp(-MAX_GAIN_DB, MAX_GAIN_DB)
        } else {
            0.0
        };
        self.preset = self.detected_preset();
    }

    /// Ustawia przedwzmacniacz (dB, przycinany).
    pub fn set_preamp(&mut self, gain_db: f32) {
        self.preamp = if gain_db.is_finite() {
            gain_db.clamp(-MAX_GAIN_DB, MAX_GAIN_DB)
        } else {
            0.0
        };
    }

    /// Najwyższe podbicie pasma — podpowiedź dla UI („dodaj przedwzmacniacz -X dB”).
    pub fn max_boost(&self) -> f32 {
        self.bands.iter().copied().fold(0.0, f32::max)
    }

    /// Wczytuje ustawienia; brak pliku = wartość domyślna (korektor wyłączony).
    pub fn load() -> Self {
        let Some(path) = config::config_file(CONFIG_FILE) else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        Self::parse(&text)
    }

    /// Zapisuje ustawienia atomowo.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = config::config_file(CONFIG_FILE) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "brak katalogu konfiguracji",
            ));
        };
        config::save_atomic(&path, &self.to_text())
    }

    /// Tekst pliku konfiguracji.
    pub fn to_text(&self) -> String {
        let mut text = String::from("# Korektor Luny — zapisuje centrum ustawień.\n");
        text.push_str(&format!("enabled = {}\n", self.enabled));
        text.push_str(&format!("preset = {}\n", self.detected_preset().key()));
        text.push_str(&format!("preamp = {:.2}\n", self.preamp));
        for (i, gain) in self.bands.iter().enumerate() {
            text.push_str(&format!("band{i} = {gain:.2}\n"));
        }
        text
    }

    /// Parsuje tekst pliku (nieznane klucze i uszkodzone wartości są pomijane).
    pub fn parse(text: &str) -> Self {
        let mut eq = Self::default();
        for (key, value) in config::parse_pairs(text) {
            match key.as_str() {
                "enabled" => eq.enabled = value == "true" || value == "1",
                "preset" => {
                    if let Some(preset) = Preset::from_key(&value) {
                        eq.preset = preset;
                    }
                }
                "preamp" => {
                    if let Ok(v) = value.parse::<f32>() {
                        eq.set_preamp(v);
                    }
                }
                _ => {
                    if let Some(index) = key
                        .strip_prefix("band")
                        .and_then(|n| n.parse::<usize>().ok())
                    {
                        if let Ok(v) = value.parse::<f32>() {
                            eq.set_band(index, v);
                        }
                    }
                }
            }
        }
        // Krzywa zapisana w pliku jest tylko podpowiedzią — liczą się wartości pasm.
        eq.preset = eq.detected_preset();
        eq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_disabled_and_flat() {
        let eq = Equalizer::default();
        assert!(!eq.enabled);
        assert_eq!(eq.detected_preset(), Preset::Flat);
        assert!(eq.bands.iter().all(|g| *g == 0.0));
    }

    #[test]
    fn presets_round_trip_through_text() {
        for preset in Preset::ALL {
            let mut eq = Equalizer::default();
            eq.enabled = true;
            eq.set_preset(preset);
            let parsed = Equalizer::parse(&eq.to_text());
            assert_eq!(parsed, eq, "{preset:?}");
            assert_eq!(parsed.detected_preset(), preset);
        }
    }

    #[test]
    fn manual_band_becomes_custom() {
        let mut eq = Equalizer::default();
        eq.set_preset(Preset::Rock);
        eq.set_band(0, -1.0);
        assert_eq!(eq.detected_preset(), Preset::Custom);
        assert_eq!(eq.preset, Preset::Custom);
        // Powrót do wartości z krzywej znów ją rozpoznaje.
        eq.set_band(0, 5.0);
        assert_eq!(eq.detected_preset(), Preset::Rock);
    }

    #[test]
    fn gains_are_clamped() {
        let mut eq = Equalizer::default();
        eq.set_band(0, 99.0);
        eq.set_band(1, f32::NAN);
        eq.set_preamp(-99.0);
        assert_eq!(eq.bands[0], MAX_GAIN_DB);
        assert_eq!(eq.bands[1], 0.0);
        assert_eq!(eq.preamp, -MAX_GAIN_DB);
        assert!(eq.to_text().contains("band0 = 12.00"));
    }

    #[test]
    fn ignores_garbage_entries() {
        let eq = Equalizer::parse(
            "enabled = maybe\nband0 = xyz\nband11 = 5\nband2 = 3.5\npreamp = -2\n",
        );
        assert!(!eq.enabled);
        assert_eq!(eq.bands[2], 3.5);
        assert_eq!(eq.preamp, -2.0);
    }

    #[test]
    fn max_boost_helps_ui() {
        let mut eq = Equalizer::default();
        assert_eq!(eq.max_boost(), 0.0);
        eq.set_preset(Preset::Bass);
        assert_eq!(eq.max_boost(), 6.0);
    }
}
