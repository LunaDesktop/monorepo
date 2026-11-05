//! `NavList` — menu nawigacji (sekcje + wybieralne pozycje) do panelu bocznego.
//!
//! Widget immediate. Pozycje są wybieralne; nagłówki sekcji pełnią rolę podpisów.
//! Etykiety podaje aplikacja (już przetłumaczone) — widget nie zna i18n.

use libluna_core::style::weight;
use libluna_core::{IconTheme, Point, Rect, Theme};

use crate::ctx::PaintCtx;
use crate::icon::{paint_icon, paint_icon_themed, IconName};

/// Element menu nawigacji.
#[derive(Debug, Clone)]
pub enum NavEntry {
    /// Nagłówek sekcji (nieklikalny).
    Section(String),
    /// Pozycja wybieralna.
    Item(String),
}

/// Wiersz rozłożony w prostokącie listy (wynik [`NavList::rows`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavRow {
    /// Indeks elementu w [`NavList::entries`].
    pub entry: usize,
    /// Prostokąt wiersza.
    pub rect: Rect,
    /// Czy pozycja jest wybieralna (a więc klikalna).
    pub selectable: bool,
}

/// Menu nawigacji.
#[derive(Debug, Clone, Default)]
pub struct NavList {
    entries: Vec<NavEntry>,
    icons: Vec<Option<IconName>>,
    selected: Option<usize>,
    hovered: Option<usize>,
    /// Wymuś monochromatyczne ikony (np. w panelach bocznych aplikacji).
    monochrome: bool,
}

impl NavList {
    pub fn new(entries: Vec<NavEntry>) -> Self {
        Self {
            entries,
            icons: Vec::new(),
            selected: None,
            hovered: None,
            monochrome: false,
        }
    }

    /// Rysuje ikony wierszy jako monochromatyczne symbole, niezależnie od
    /// globalnego zestawu ikon. Używane przez panele boczne aplikacji.
    pub fn monochrome(mut self, on: bool) -> Self {
        self.monochrome = on;
        self
    }

    /// Ustawia ikony wierszy (indeks zgodny z [`NavList::entries`]).
    /// Pozycje bez ikony zachowują kropkę jak dotychczas.
    pub fn set_icons(&mut self, mut icons: Vec<Option<IconName>>) {
        icons.resize(self.entries.len(), None);
        self.icons = icons;
    }

    pub fn entries(&self) -> &[NavEntry] {
        &self.entries
    }

    /// Indeks wybranej pozycji (albo `None`).
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn selected_label(&self) -> Option<&str> {
        match self.selected.and_then(|i| self.entries.get(i)) {
            Some(NavEntry::Item(label)) => Some(label),
            _ => None,
        }
    }

    /// Ustawia wybór; ignoruje indeksy wskazujące na nagłówki sekcji.
    pub fn set_selected(&mut self, index: Option<usize>) {
        self.selected = match index {
            Some(i) if matches!(self.entries.get(i), Some(NavEntry::Item(_))) => Some(i),
            _ => None,
        };
    }

    pub fn set_hovered(&mut self, hovered: Option<usize>) {
        self.hovered = hovered;
    }

    /// Wysokość potrzebna na całą listę (piksele logiczne).
    pub fn height(&self, theme: &Theme) -> f64 {
        let m = theme.metrics.nav;
        let body: f64 = self
            .entries
            .iter()
            .map(|e| match e {
                NavEntry::Section(_) => m.section_h,
                NavEntry::Item(_) => m.item_h,
            })
            .sum();
        m.pad_v + body + m.pad_v
    }

    /// Rozkłada wiersze od góry `rect`.
    pub fn rows(&self, rect: Rect, theme: &Theme) -> Vec<NavRow> {
        let m = theme.metrics.nav;
        let mut rows = Vec::with_capacity(self.entries.len());
        let mut y = rect.y() + m.pad_v;
        for (i, entry) in self.entries.iter().enumerate() {
            let (h, selectable) = match entry {
                NavEntry::Section(_) => (m.section_h, false),
                NavEntry::Item(_) => (m.item_h, true),
            };
            rows.push(NavRow {
                entry: i,
                rect: Rect::from_xywh(rect.x(), y, rect.width(), h),
                selectable,
            });
            y += h;
        }
        rows
    }

    /// Indeks wybieralnej pozycji pod punktem `p` (nagłówki zwracają `None`).
    pub fn index_at(&self, rect: Rect, p: Point, theme: &Theme) -> Option<usize> {
        self.rows(rect, theme)
            .into_iter()
            .find(|row| row.selectable && row.rect.contains(p))
            .map(|row| row.entry)
    }

    /// Obsługuje klik: ustawia wybór. Zwraca `true`, jeśli wybór się zmienił.
    pub fn click_at(&mut self, rect: Rect, p: Point, theme: &Theme) -> bool {
        match self.index_at(rect, p, theme) {
            Some(i) if self.selected != Some(i) => {
                self.selected = Some(i);
                true
            }
            _ => false,
        }
    }

    /// Rysuje listę w prostokącie `rect`.
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let radius = theme.radius;
        let colors = theme.colors;
        let caption = theme.typography.caption.size;
        let body = theme.typography.body.size;
        let pad_x = theme.metrics.nav.pad_x;

        for row in self.rows(rect, ctx.theme) {
            match &self.entries[row.entry] {
                NavEntry::Section(title) => {
                    ctx.label_left(
                        row.rect.x() + pad_x + 8.0,
                        row.rect.center().y,
                        title,
                        caption,
                        weight::BOLD,
                        colors.dim_text,
                    );
                }
                NavEntry::Item(label) => {
                    let item = Rect::from_xywh(
                        row.rect.x() + pad_x,
                        row.rect.y(),
                        (row.rect.width() - 2.0 * pad_x).max(0.0),
                        row.rect.height(),
                    );
                    let selected = self.selected == Some(row.entry);
                    if selected || self.hovered == Some(row.entry) {
                        ctx.scene
                            .fill_rounded_rect(item, radius.small, colors.shade);
                    }
                    let icon = self.icons.get(row.entry).copied().flatten();
                    let label_x = if let Some(icon) = icon {
                        let icon_rect =
                            Rect::from_xywh(item.x() + 6.0, item.center().y - 9.0, 18.0, 18.0);
                        let color = if selected { colors.accent } else { colors.text };
                        if self.monochrome {
                            paint_icon_themed(ctx, icon, icon_rect, color, IconTheme::Monochrome);
                        } else {
                            paint_icon(ctx, icon, icon_rect, color);
                        }
                        item.x() + 30.0
                    } else {
                        let dot = Rect::from_xywh(item.x() + 8.0, item.center().y - 4.0, 8.0, 8.0);
                        let dot_color = if selected {
                            colors.accent
                        } else {
                            colors.dim_text
                        };
                        ctx.scene.fill_rounded_rect(dot, 4.0, dot_color);
                        item.x() + 26.0
                    };

                    let fg = colors.text;
                    let weight = if selected {
                        weight::MEDIUM
                    } else {
                        weight::NORMAL
                    };
                    ctx.label_left(label_x, item.center().y, label, body, weight, fg);
                }
            }
        }
    }

    /// Ustawia podświetlenie na podstawie pozycji wskaźnika (wygodne w `PointerMotion`).
    pub fn hover_at(&mut self, rect: Rect, p: Option<Point>, theme: &Theme) {
        self.hovered = p.and_then(|p| self.index_at(rect, p, theme));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> NavList {
        NavList::new(vec![
            NavEntry::Item("Overview".into()),
            NavEntry::Item("Account".into()),
            NavEntry::Section("Advanced".into()),
            NavEntry::Item("Network".into()),
        ])
    }

    fn area() -> Rect {
        Rect::from_xywh(0.0, 0.0, 200.0, 400.0)
    }

    #[test]
    fn rows_have_expected_heights() {
        let theme = Theme::light();
        let m = theme.metrics.nav;
        let l = list();
        let rows = l.rows(area(), &theme);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].rect.height(), m.item_h);
        assert!(!rows[2].selectable);
        assert_eq!(
            l.height(&theme),
            m.pad_v * 2.0 + 3.0 * m.item_h + m.section_h
        );
    }

    #[test]
    fn index_at_skips_sections() {
        let theme = Theme::light();
        let l = list();
        let rows = l.rows(area(), &theme);
        // Środek nagłówka sekcji → brak trafienia.
        let section_p = rows[2].rect.center();
        assert_eq!(l.index_at(area(), section_p, &theme), None);
        // Środek pierwszej pozycji → trafienie w indeks 0.
        assert_eq!(l.index_at(area(), rows[0].rect.center(), &theme), Some(0));
    }

    #[test]
    fn click_sets_selection_and_reports_change() {
        let theme = Theme::light();
        let mut l = list();
        let rows = l.rows(area(), &theme);
        assert!(l.click_at(area(), rows[3].rect.center(), &theme));
        assert_eq!(l.selected(), Some(3));
        assert_eq!(l.selected_label(), Some("Network"));
        assert!(!l.click_at(area(), rows[3].rect.center(), &theme));
    }

    #[test]
    fn cannot_select_a_section() {
        let mut l = list();
        l.set_selected(Some(2));
        assert_eq!(l.selected(), None);
    }

    #[test]
    fn icons_align_with_entries_and_pad_missing() {
        let mut l = list();
        l.set_icons(vec![Some(IconName::Home), None, Some(IconName::Archive)]);
        assert_eq!(l.icons.len(), 4);
        assert_eq!(l.icons[0], Some(IconName::Home));
        assert_eq!(l.icons[2], Some(IconName::Archive));
        assert_eq!(l.icons[3], None);
    }
}
