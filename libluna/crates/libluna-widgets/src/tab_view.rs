//! `TabView` — model stron zakładek: dodawanie, zamykanie i wybór aktywnej.
//!
//! Sam nic nie rysuje — zawartość aktywnej strony rysuje aplikacja. Do paska zakładek
//! służy [`TabView::tab_bar`], który zwraca gotowy [`TabBar`] z bieżącym wyborem.

use crate::tab_bar::{Tab, TabBar};

/// Zbiór stron (zakładek) z aktywną stroną.
#[derive(Debug, Clone, Default)]
pub struct TabView {
    tabs: Vec<Tab>,
    selected: usize,
}

impl TabView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.selected)
    }

    pub fn selected_title(&self) -> Option<&str> {
        self.selected_tab().map(|t| t.title.as_str())
    }

    /// Dodaje zakładkę i zwraca jej indeks (pierwsza staje się aktywna).
    pub fn add(&mut self, tab: Tab) -> usize {
        self.tabs.push(tab);
        self.tabs.len() - 1
    }

    /// Dodaje zakładkę z etykietą.
    pub fn add_titled(&mut self, title: impl Into<String>) -> usize {
        self.add(Tab::new(title))
    }

    /// Wybiera zakładkę (poza zakresem jest przycinana).
    pub fn select(&mut self, index: usize) {
        if !self.tabs.is_empty() {
            self.selected = index.min(self.tabs.len() - 1);
        }
    }

    /// Zakładka następna (z zawijaniem).
    pub fn select_next(&mut self) {
        if !self.tabs.is_empty() {
            self.selected = (self.selected + 1) % self.tabs.len();
        }
    }

    /// Zakładka poprzednia (z zawijaniem).
    pub fn select_prev(&mut self) {
        if !self.tabs.is_empty() {
            self.selected = (self.selected + self.tabs.len() - 1) % self.tabs.len();
        }
    }

    /// Zamyka zakładkę; pilnuje, żeby wybór pozostał poprawny. Zwraca `true`, jeśli zamknięto.
    pub fn close(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.selected = 0;
        } else if index < self.selected {
            self.selected -= 1;
        } else if index == self.selected {
            self.selected = self.selected.min(self.tabs.len() - 1);
        }
        true
    }

    /// Pasek zakładek do narysowania (z bieżącym wyborem).
    pub fn tab_bar(&self) -> TabBar {
        let mut bar = TabBar::with_tabs(self.tabs.clone());
        bar.set_selected(self.selected);
        bar
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_select() {
        let mut v = TabView::new();
        assert_eq!(v.add_titled("A"), 0);
        assert_eq!(v.add_titled("B"), 1);
        assert_eq!(v.selected(), 0);
        v.select(1);
        assert_eq!(v.selected_title(), Some("B"));
        v.select(99);
        assert_eq!(v.selected(), 1); // przycięte
    }

    #[test]
    fn next_prev_wrap() {
        let mut v = TabView::new();
        v.add_titled("A");
        v.add_titled("B");
        v.add_titled("C");
        v.select(2);
        v.select_next();
        assert_eq!(v.selected(), 0);
        v.select_prev();
        assert_eq!(v.selected(), 2);
    }

    #[test]
    fn close_before_selection_shifts_it() {
        let mut v = TabView::new();
        v.add_titled("A");
        v.add_titled("B");
        v.add_titled("C");
        v.select(2);
        assert!(v.close(0));
        assert_eq!(v.selected(), 1);
        assert_eq!(v.selected_title(), Some("C"));
    }

    #[test]
    fn close_selected_picks_neighbour() {
        let mut v = TabView::new();
        v.add_titled("A");
        v.add_titled("B");
        v.add_titled("C");
        v.select(2); // ostatnia
        v.close(2);
        assert_eq!(v.selected(), 1); // cofnięcie do sąsiada
        assert_eq!(v.selected_title(), Some("B"));
    }

    #[test]
    fn close_last_empties() {
        let mut v = TabView::new();
        v.add_titled("A");
        assert!(v.close(0));
        assert!(v.is_empty());
        assert_eq!(v.selected(), 0);
        assert_eq!(v.selected_title(), None);
        assert!(!v.close(0)); // już pusto
    }

    #[test]
    fn tab_bar_reflects_state() {
        let mut v = TabView::new();
        v.add(Tab::new("A").closable(true));
        v.add_titled("B");
        v.select(1);
        let bar = v.tab_bar();
        assert_eq!(bar.len(), 2);
        assert_eq!(bar.selected(), 1);
        assert!(bar.tabs()[0].closable);
    }
}
