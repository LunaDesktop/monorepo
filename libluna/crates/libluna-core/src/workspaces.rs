//! Układ przestrzeni roboczych (`~/.config/luna/workspaces.conf`).
//!
//! Kompozytor Selene czyta ten plik przy starcie, a powłoka może zmienić nazwy
//! w locie przez `luna-control`. Liczba przestrzeni jest stała w ramach sesji
//! (protokół `ext_workspace_v1` nie tworzy ich dynamicznie), więc zmiana liczby
//! jest stosowana przy następnym uruchomieniu Selene.

use std::path::{Path, PathBuf};

use crate::config::{clean_value, config_file, parse_pairs, save_atomic};

/// Minimalna i maksymalna liczba przestrzeni.
pub const MIN_WORKSPACES: usize = 1;
pub const MAX_WORKSPACES: usize = 8;

/// Liczba i nazwy przestrzeni roboczych.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceLayout {
    count: usize,
    names: Vec<String>,
    /// Włączanie przestrzeni przez najechanie/przeciągnięcie do dolnego rogu ekranu.
    corner_switch: bool,
    /// Otwieranie podglądu (Super+Tab) przez lewy górny róg ekranu.
    overview_corner: bool,
}

impl Default for WorkspaceLayout {
    fn default() -> Self {
        Self {
            count: 4,
            names: Vec::new(),
            corner_switch: false,
            overview_corner: true,
        }
    }
}

impl WorkspaceLayout {
    pub fn new(count: usize) -> Self {
        Self {
            count: count.clamp(MIN_WORKSPACES, MAX_WORKSPACES),
            names: Vec::new(),
            corner_switch: false,
            overview_corner: true,
        }
    }

    pub fn count(&self) -> usize {
        self.count
    }

    /// Czy dolne rogi ekranu przełączają przestrzenie (opcja użytkownika).
    pub fn corner_switch(&self) -> bool {
        self.corner_switch
    }

    pub fn set_corner_switch(&mut self, enabled: bool) {
        self.corner_switch = enabled;
    }

    /// Czy lewy górny róg otwiera podgląd okien i przestrzeni (Super+Tab).
    pub fn overview_corner(&self) -> bool {
        self.overview_corner
    }

    pub fn set_overview_corner(&mut self, enabled: bool) {
        self.overview_corner = enabled;
    }

    pub fn set_count(&mut self, count: usize) {
        self.count = count.clamp(MIN_WORKSPACES, MAX_WORKSPACES);
        self.names.truncate(self.count);
    }

    /// Nazwa przestrzeni `index` (albo numer, gdy użytkownik jej nie ustawił).
    pub fn name(&self, index: usize) -> String {
        self.names
            .get(index)
            .filter(|name| !name.is_empty())
            .cloned()
            .unwrap_or_else(|| (index + 1).to_string())
    }

    pub fn set_name(&mut self, index: usize, name: Option<&str>) {
        if index >= self.count {
            return;
        }
        if self.names.len() <= index {
            self.names.resize(index + 1, String::new());
        }
        self.names[index] = name
            .and_then(clean_value)
            .filter(|value| value.len() <= 64)
            .unwrap_or_default();
    }

    /// Wszystkie nazwy (puste = numer).
    pub fn names(&self) -> Vec<String> {
        (0..self.count).map(|i| self.name(i)).collect()
    }

    pub fn config_path() -> Option<PathBuf> {
        config_file("workspaces.conf")
    }

    pub fn load() -> Self {
        Self::config_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn parse(text: &str) -> Self {
        let mut layout = Self::default();
        for (key, value) in parse_pairs(text) {
            if key == "count" {
                if let Ok(count) = value.parse::<usize>() {
                    layout.set_count(count);
                }
            } else if key == "corner_switch" {
                layout.set_corner_switch(!matches!(value.as_str(), "false" | "0" | "no"));
            } else if key == "overview_corner" {
                layout.set_overview_corner(!matches!(value.as_str(), "false" | "0" | "no"));
            } else if let Some(index) = key
                .strip_prefix("name")
                .and_then(|rest| rest.parse::<usize>().ok())
                .filter(|index| *index >= 1)
            {
                layout.set_name(index - 1, Some(&value));
            }
        }
        layout
    }

    pub fn render(&self) -> String {
        let mut out = String::from(
            "# Luna — przestrzenie robocze\n\
             # count: 1..8. nameN: własna nazwa (puste = numer).\n",
        );
        out.push_str(&format!("count = {}\n", self.count));
        out.push_str(&format!("corner_switch = {}\n", self.corner_switch));
        out.push_str(&format!("overview_corner = {}\n", self.overview_corner));
        for index in 0..self.count {
            if let Some(name) = self.names.get(index).filter(|name| !name.is_empty()) {
                out.push_str(&format!("name{} = {name}\n", index + 1));
            }
        }
        out
    }

    pub fn save_local(&self, path: &Path) -> std::io::Result<()> {
        save_atomic(path, &self.render())
    }

    pub fn save(&self) -> std::io::Result<()> {
        match Self::config_path() {
            Some(path) => self.save_local(&path),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_count_and_names() {
        let layout = WorkspaceLayout::parse(
            "count = 3\ncorner_switch = true\noverview_corner = false\nname1 = Kod\nname3 = Gry\n",
        );
        assert_eq!(layout.count(), 3);
        assert!(layout.corner_switch());
        assert!(!layout.overview_corner());
        assert_eq!(layout.name(0), "Kod");
        assert_eq!(layout.name(1), "2");
        assert_eq!(layout.name(2), "Gry");
        assert_eq!(WorkspaceLayout::parse(&layout.render()), layout);
    }

    #[test]
    fn count_is_clamped_and_truncates_names() {
        let mut layout = WorkspaceLayout::parse("count = 5\nname4 = X\n");
        assert_eq!(layout.name(3), "X");
        layout.set_count(2);
        assert_eq!(layout.count(), 2);
        assert_eq!(layout.names(), vec!["1", "2"]);
        layout.set_count(99);
        assert_eq!(layout.count(), MAX_WORKSPACES);
    }

    #[test]
    fn blank_names_fall_back_to_numbers() {
        let mut layout = WorkspaceLayout::new(2);
        layout.set_name(0, Some("  "));
        assert_eq!(layout.name(0), "1");
    }
}
