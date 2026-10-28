//! `PathBar` — pasek ścieżki (okruszki): `Domowy › Dokumenty › Projekty ▾`.
//!
//! Widget immediate: trzyma listę elementów ścieżki (etykiety podaje aplikacja, już
//! przetłumaczone). Aplikacja woła [`PathBar::paint`] oraz [`PathBar::index_at`] /
//! [`PathBar::click_at`] i sama decyduje o nawigacji. Gdy ścieżka nie mieści się w szerokości,
//! środkowe elementy zastępuje wielokropek `…` (zostaje początek i bieżący katalog).

use libluna_core::style::weight;
use libluna_core::{Point, Rect, Theme};
use libluna_render::TextEngine;

use crate::ctx::PaintCtx;
use crate::icon;

/// Rozpiętość separatora „›” i wskaźnika „▾”.
const CHEVRON: f64 = 8.0;

/// Pasek ścieżki.
#[derive(Debug, Clone, Default)]
pub struct PathBar {
    components: Vec<String>,
    hovered: Option<usize>,
}

impl PathBar {
    /// Tworzy pasek z listy etykiet (ostatnia to bieżący katalog).
    pub fn new<I, S>(components: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            components: components.into_iter().map(Into::into).collect(),
            hovered: None,
        }
    }

    pub fn components(&self) -> &[String] {
        &self.components
    }

    pub fn len(&self) -> usize {
        self.components.len()
    }

    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    /// Dokłada element na końcu (np. po wejściu do katalogu).
    pub fn push(&mut self, label: impl Into<String>) {
        self.components.push(label.into());
    }

    /// Skraca ścieżkę do `len` elementów (nawigacja „w górę”).
    pub fn truncate(&mut self, len: usize) {
        self.components.truncate(len);
        if self.hovered.is_some_and(|h| h >= self.components.len()) {
            self.hovered = None;
        }
    }

    /// Indeks bieżącego katalogu (ostatni element) albo `None` dla pustego paska.
    pub fn active(&self) -> Option<usize> {
        self.components.len().checked_sub(1)
    }

    pub fn set_hovered(&mut self, hovered: Option<usize>) {
        self.hovered = hovered;
    }

    /// Wysokość paska (z tokenów motywu).
    pub fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.pathbar.height
    }

    fn seg_width(&self, index: usize, text: &TextEngine, theme: &Theme) -> f64 {
        let m = theme.metrics.pathbar;
        let body = theme.typography.body.size;
        let mut w = text
            .measure(&self.components[index], body, weight::NORMAL)
            .width
            + 2.0 * m.pad_x;
        if Some(index) == self.active() {
            w += CHEVRON + m.gap; // miejsce na wskaźnik „▾”.
        }
        w
    }

    /// Prostokąty widocznych elementów; `None` oznacza wielokropek `…`.
    pub fn segments(
        &self,
        rect: Rect,
        text: &TextEngine,
        theme: &Theme,
    ) -> Vec<(Option<usize>, Rect)> {
        let m = theme.metrics.pathbar;
        let n = self.components.len();
        if n == 0 || rect.width() <= 0.0 {
            return Vec::new();
        }
        let sep = 2.0 * m.gap + CHEVRON;
        let total: f64 =
            (0..n).map(|i| self.seg_width(i, text, theme)).sum::<f64>() + (n as f64 - 1.0) * sep;

        let picks: Vec<Option<usize>> = if total <= rect.width() {
            (0..n).map(Some).collect()
        } else {
            // Zostaw początek i bieżący katalog; w środku wielokropek.
            let body = theme.typography.body.size;
            let ell = text.measure("…", body, weight::NORMAL).width + 2.0 * m.pad_x;
            let mut rev = vec![n - 1];
            let mut used = self.seg_width(n - 1, text, theme);
            for i in (0..n - 1).rev() {
                let w = self.seg_width(i, text, theme) + sep;
                if used + w + sep + ell <= rect.width() {
                    used += w;
                    rev.push(i);
                } else {
                    break;
                }
            }
            rev.reverse();
            let mut v: Vec<Option<usize>> = vec![None];
            v.extend(rev.into_iter().map(Some));
            v
        };

        let mut x = rect.x();
        let mut out = Vec::with_capacity(picks.len());
        for (k, pick) in picks.iter().enumerate() {
            if k > 0 {
                x += sep;
            }
            let w = match pick {
                Some(i) => self.seg_width(*i, text, theme),
                None => {
                    text.measure("…", theme.typography.body.size, weight::NORMAL)
                        .width
                        + 2.0 * m.pad_x
                }
            };
            out.push((*pick, Rect::from_xywh(x, rect.y(), w, rect.height())));
            x += w;
        }
        out
    }

    /// Indeks elementu pod punktem `p` (wielokropek i separatory zwracają `None`).
    pub fn index_at(
        &self,
        rect: Rect,
        text: &TextEngine,
        theme: &Theme,
        p: Point,
    ) -> Option<usize> {
        self.segments(rect, text, theme)
            .into_iter()
            .find(|(pick, r)| pick.is_some() && r.contains(p))
            .and_then(|(pick, _)| pick)
    }

    /// Obsługuje klik: zwraca indeks wybranego elementu (nawigację robi aplikacja).
    pub fn click_at(
        &self,
        rect: Rect,
        text: &TextEngine,
        theme: &Theme,
        p: Point,
    ) -> Option<usize> {
        self.index_at(rect, text, theme, p)
    }

    /// Rysuje pasek ścieżki w prostokącie `rect`.
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let m = theme.metrics.pathbar;
        let colors = theme.colors;
        let body = theme.typography.body.size;
        let active = self.active();
        let text = ctx.text;
        let segs = self.segments(rect, text, &theme);

        for (k, (pick, seg)) in segs.iter().enumerate() {
            if k > 0 {
                let cx = seg.x() - m.gap - CHEVRON / 2.0;
                icon::chevron_right(
                    ctx,
                    colors.dim_text,
                    Point::new(cx, seg.center().y),
                    4.0,
                    1.6,
                );
            }
            match pick {
                Some(i) => {
                    let is_active = Some(*i) == active;
                    if self.hovered == Some(*i) && !is_active {
                        ctx.scene
                            .fill_rounded_rect(*seg, theme.radius.small, colors.shade);
                    }
                    let color = if is_active {
                        colors.text
                    } else {
                        colors.dim_text
                    };
                    let weight = if is_active {
                        weight::MEDIUM
                    } else {
                        weight::NORMAL
                    };
                    let shift = if is_active {
                        (CHEVRON + m.gap) / 2.0
                    } else {
                        0.0
                    };
                    ctx.label_center(
                        seg.center().x - shift,
                        seg.center().y,
                        &self.components[*i],
                        body,
                        weight,
                        color,
                    );
                    if is_active {
                        icon::chevron_down(
                            ctx,
                            colors.dim_text,
                            Point::new(seg.right() - m.pad_x - CHEVRON / 2.0, seg.center().y),
                            4.0,
                            1.6,
                        );
                    }
                }
                None => ctx.label_center(
                    seg.center().x,
                    seg.center().y,
                    "…",
                    body,
                    weight::NORMAL,
                    colors.dim_text,
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 400.0, 34.0)
    }

    #[test]
    fn all_components_when_they_fit() {
        let text = TextEngine::system();
        let theme = Theme::light();
        let bar = PathBar::new(["Dom", "Dokumenty", "Projekty"]);
        let segs = bar.segments(rect(), &text, &theme);
        assert_eq!(segs.len(), 3);
        assert!(segs.iter().all(|(p, _)| p.is_some()));
        assert_eq!(bar.active(), Some(2));
    }

    #[test]
    fn index_at_maps_components() {
        let text = TextEngine::system();
        let theme = Theme::light();
        let bar = PathBar::new(["Dom", "Dokumenty", "Projekty"]);
        let segs = bar.segments(rect(), &text, &theme);
        assert_eq!(
            bar.index_at(rect(), &text, &theme, segs[0].1.center()),
            Some(0)
        );
        assert_eq!(
            bar.index_at(rect(), &text, &theme, segs[2].1.center()),
            Some(2)
        );
    }

    #[test]
    fn overflow_inserts_ellipsis_and_keeps_last() {
        let text = TextEngine::system();
        let theme = Theme::light();
        let narrow = Rect::from_xywh(0.0, 0.0, 150.0, 34.0);
        let bar = PathBar::new(["Katalog domowy", "Dokumenty", "Projekty", "libLuna"]);
        let segs = bar.segments(narrow, &text, &theme);
        // Pierwszy widoczny to wielokropek, ostatni to bieżący katalog.
        assert_eq!(segs[0].0, None);
        assert_eq!(segs.last().unwrap().0, Some(3));
    }

    #[test]
    fn empty_bar_has_no_segments() {
        let text = TextEngine::system();
        let theme = Theme::light();
        let bar = PathBar::default();
        assert!(bar.segments(rect(), &text, &theme).is_empty());
        assert_eq!(bar.active(), None);
    }
}
