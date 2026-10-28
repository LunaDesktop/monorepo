//! Wykrywanie uszkodzeń (damage) na podstawie różnicy klatek.
//!
//! Pełny damage tracking (raportowany przez widgety) jest osobnym etapem. Ten
//! moduł daje bezpieczny pierwszy krok: porównuje dwa bufory RGBA8888 i zwraca
//! **najmniejszy prostokąt obejmujący wszystkie zmienione piksele**. Platforma
//! używa go, żeby:
//!
//! - pominąć `wl_surface.commit`, gdy klatka jest identyczna z poprzednią,
//! - zawęzić `wl_surface.damage_buffer` do realnie zmienionego obszaru.
//!
//! To nie zastępuje pamięci poprzedniej klatki po stronie aplikacji — bufor
//! renderera nadal jest budowany od zera. Oszczędza jednak pracę kompozytora
//! (upload/kompozycja) i eliminuje puste commity przy timerach.

/// Prostokąt uszkodzenia w pikselach fizycznych.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DamageRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl DamageRect {
    pub fn full(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }
}

/// Zwraca najmniejszy prostokąt różniący się między `prev` i `next`.
///
/// Oba bufory muszą mieć `width * height * 4` bajtów w kolejności RGBA8888.
/// `None` = klatki są identyczne (nic nie trzeba wysyłać). Gdy rozmiary się nie
/// zgadzają (np. po `resize`), zwracany jest pełny prostokąt.
pub fn diff_bounds(prev: &[u8], next: &[u8], width: u32, height: u32) -> Option<DamageRect> {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 {
        return None;
    }
    let row = w * 4;
    let total = row * h;
    if prev.len() != total || next.len() != total {
        return Some(DamageRect::full(width, height));
    }

    let mut min_x = w;
    let mut max_x = 0usize;
    let mut min_y = h;
    let mut max_y = 0usize;

    for y in 0..h {
        let start = y * row;
        let a = &prev[start..start + row];
        let b = &next[start..start + row];
        // `memcmp` całego wiersza — najtańszy przypadek: wiersz bez zmian.
        if a == b {
            continue;
        }
        let mut first = None;
        let mut last = 0usize;
        for x in 0..w {
            let i = x * 4;
            if a[i..i + 4] != b[i..i + 4] {
                first.get_or_insert(x);
                last = x;
            }
        }
        if let Some(first) = first {
            min_x = min_x.min(first);
            max_x = max_x.max(last);
            min_y = min_y.min(y);
            max_y = y;
        }
    }

    if min_x > max_x {
        None
    } else {
        Some(DamageRect {
            x: min_x as u32,
            y: min_y as u32,
            width: (max_x - min_x + 1) as u32,
            height: (max_y - min_y + 1) as u32,
        })
    }
}

/// Jak [`diff_bounds`], ale porównuje **tylko** podany prostokąt.
///
/// Używane po odczycie z GPU ograniczonym do damage: reszta bufora jest z założenia
/// identyczna z poprzednią klatką, więc przeglądanie jej to strata czasu.
pub fn diff_bounds_in(
    prev: &[u8],
    next: &[u8],
    width: u32,
    height: u32,
    region: DamageRect,
) -> Option<DamageRect> {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 {
        return None;
    }
    let total = w * 4 * h;
    if prev.len() != total || next.len() != total {
        return Some(DamageRect::full(width, height));
    }
    let x0 = (region.x as usize).min(w);
    let y0 = (region.y as usize).min(h);
    let x1 = (region.x as usize + region.width as usize).min(w);
    let y1 = (region.y as usize + region.height as usize).min(h);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let row = w * 4;
    let mut min_x = x1;
    let mut max_x = x0;
    let mut min_y = y1;
    let mut max_y = y0;
    for y in y0..y1 {
        let start = y * row;
        let a = &prev[start + x0 * 4..start + x1 * 4];
        let b = &next[start + x0 * 4..start + x1 * 4];
        if a == b {
            continue;
        }
        for x in x0..x1 {
            let i = start + x * 4;
            if prev[i..i + 4] != next[i..i + 4] {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x > max_x {
        None
    } else {
        Some(DamageRect {
            x: min_x as u32,
            y: min_y as u32,
            width: (max_x - min_x + 1) as u32,
            height: (max_y - min_y + 1) as u32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(width: u32, height: u32, fill: u8) -> Vec<u8> {
        vec![fill; (width * height * 4) as usize]
    }

    #[test]
    fn identical_frames_have_no_damage() {
        let a = frame(4, 3, 7);
        assert_eq!(diff_bounds(&a, &a, 4, 3), None);
        let region = DamageRect {
            x: 0,
            y: 0,
            width: 4,
            height: 3,
        };
        assert_eq!(diff_bounds_in(&a, &a, 4, 3, region), None);
    }

    #[test]
    fn one_pixel_returns_minimal_rect() {
        let a = frame(4, 3, 0);
        let mut b = a.clone();
        let i = ((1 * 4 + 2) * 4) as usize; // x=2, y=1
        b[i..i + 4].copy_from_slice(&[1, 2, 3, 4]);
        assert_eq!(
            diff_bounds(&a, &b, 4, 3),
            Some(DamageRect {
                x: 2,
                y: 1,
                width: 1,
                height: 1
            })
        );
    }

    #[test]
    fn bounds_cover_all_changed_pixels() {
        let a = frame(8, 5, 0);
        let mut b = a.clone();
        for (x, y) in [(1, 1), (6, 3)] {
            let i = ((y * 8 + x) * 4) as usize;
            b[i..i + 4].copy_from_slice(&[9, 9, 9, 9]);
        }
        assert_eq!(
            diff_bounds(&a, &b, 8, 5),
            Some(DamageRect {
                x: 1,
                y: 1,
                width: 6,
                height: 3
            })
        );
    }

    #[test]
    fn size_change_forces_full_damage_and_zero_is_empty() {
        let a = frame(4, 4, 0);
        let b = frame(5, 4, 0);
        assert_eq!(diff_bounds(&a, &b, 5, 4), Some(DamageRect::full(5, 4)));
        assert_eq!(diff_bounds(&[], &[], 0, 0), None);
    }

    #[test]
    fn region_diff_ignores_changes_outside() {
        let a = frame(8, 8, 0);
        let mut b = a.clone();
        // Zmiana poza regionem (x=0,y=0)…
        b[0..4].copy_from_slice(&[7, 7, 7, 7]);
        let region = DamageRect {
            x: 4,
            y: 4,
            width: 4,
            height: 4,
        };
        assert_eq!(diff_bounds_in(&a, &b, 8, 8, region), None);
        // …i zmiana w środku regionu (x=5,y=6).
        let i = ((6 * 8 + 5) * 4) as usize;
        b[i..i + 4].copy_from_slice(&[1, 2, 3, 4]);
        assert_eq!(
            diff_bounds_in(&a, &b, 8, 8, region),
            Some(DamageRect {
                x: 5,
                y: 6,
                width: 1,
                height: 1,
            })
        );
    }
}
