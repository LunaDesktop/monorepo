//! `Picture` — obraz z dopasowaniem treści (odpowiednik GTK4 `GtkPicture`).
//!
//! Widget immediate: [`Picture::dest_rect`] liczy prostokąt docelowy z zachowaniem
//! proporcji wg [`ContentFit`], a [`Picture::paint`] rysuje obraz (przy `Cover` przycina
//! do obszaru).

use std::sync::Arc;

use libluna_core::Rect;
use libluna_render::ImageData;

use crate::ctx::PaintCtx;

/// Sposób dopasowania obrazu do obszaru.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentFit {
    /// Rozciągnij (ignoruje proporcje).
    Fill,
    /// Zmieść cały obraz (zachowuje proporcje, może zostawić pasy).
    #[default]
    Contain,
    /// Wypełnij obszar (zachowuje proporcje, przycina nadmiar).
    Cover,
    /// Dopasuj szerokość.
    FitWidth,
    /// Dopasuj wysokość.
    FitHeight,
}

/// Obraz z dopasowaniem.
#[derive(Debug, Clone, Default)]
pub struct Picture {
    image: Option<Arc<ImageData>>,
    fit: ContentFit,
}

impl Picture {
    pub fn new() -> Self {
        Self::default()
    }

    /// Obraz z domyślnym dopasowaniem `Contain`.
    pub fn with_image(image: Arc<ImageData>) -> Self {
        Self {
            image: Some(image),
            fit: ContentFit::Contain,
        }
    }

    pub fn set_image(&mut self, image: Arc<ImageData>) {
        self.image = Some(image);
    }

    pub fn set_fit(&mut self, fit: ContentFit) {
        self.fit = fit;
    }

    pub fn image(&self) -> Option<&Arc<ImageData>> {
        self.image.as_ref()
    }

    pub fn fit(&self) -> ContentFit {
        self.fit
    }

    /// Prostokąt, w którym obraz zostanie narysowany (z zachowaniem proporcji wg `fit`).
    pub fn dest_rect(&self, area: Rect) -> Rect {
        let Some(img) = &self.image else {
            return area;
        };
        let (iw, ih) = (img.width as f64, img.height as f64);
        if iw <= 0.0 || ih <= 0.0 {
            return area;
        }
        let (aw, ah) = (area.width(), area.height());
        let c = area.center();
        match self.fit {
            ContentFit::Fill => area,
            ContentFit::Contain | ContentFit::Cover => {
                let scale = if self.fit == ContentFit::Contain {
                    (aw / iw).min(ah / ih)
                } else {
                    (aw / iw).max(ah / ih)
                };
                let (w, h) = (iw * scale, ih * scale);
                Rect::from_xywh(c.x - w / 2.0, c.y - h / 2.0, w, h)
            }
            ContentFit::FitWidth => {
                let h = ih * (aw / iw);
                Rect::from_xywh(area.x(), c.y - h / 2.0, aw, h)
            }
            ContentFit::FitHeight => {
                let w = iw * (ah / ih);
                Rect::from_xywh(c.x - w / 2.0, area.y(), w, ah)
            }
        }
    }

    /// Rysuje obraz w obszarze `area`.
    pub fn paint(&self, ctx: &mut PaintCtx, area: Rect) {
        let Some(img) = &self.image else {
            return;
        };
        let r = self.dest_rect(area);
        let clip = self.fit == ContentFit::Cover;
        if clip {
            ctx.scene.push_clip(area);
        }
        ctx.scene.image(r, img.clone());
        if clip {
            ctx.scene.pop_clip();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_core::Theme;
    use libluna_render::{Scene, TextEngine};

    fn img(w: u32, h: u32) -> Arc<ImageData> {
        Arc::new(ImageData::empty(w, h))
    }

    #[test]
    fn fill_uses_whole_area() {
        let p = Picture {
            image: Some(img(100, 50)),
            fit: ContentFit::Fill,
        };
        assert_eq!(
            p.dest_rect(Rect::from_xywh(0.0, 0.0, 200.0, 200.0)).width(),
            200.0
        );
    }

    #[test]
    fn contain_preserves_aspect_and_fits_inside() {
        let p = Picture {
            image: Some(img(200, 100)), // 2:1
            fit: ContentFit::Contain,
        };
        let area = Rect::from_xywh(0.0, 0.0, 200.0, 200.0);
        let r = p.dest_rect(area);
        assert!((r.width() / r.height() - 2.0).abs() < 1e-6);
        assert!(r.width() <= area.width() + 1e-6 && r.height() <= area.height() + 1e-6);
    }

    #[test]
    fn fit_width_matches_area_width() {
        let p = Picture {
            image: Some(img(200, 100)),
            fit: ContentFit::FitWidth,
        };
        let area = Rect::from_xywh(0.0, 0.0, 300.0, 100.0);
        let r = p.dest_rect(area);
        assert!((r.width() - 300.0).abs() < 1e-6);
        assert!((r.height() - 150.0).abs() < 1e-6);
    }

    #[test]
    fn paints_without_window() {
        let theme = Theme::light();
        let text = TextEngine::none();
        let mut scene = Scene::new();
        let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
        Picture::with_image(img(32, 32)).paint(&mut ctx, Rect::from_xywh(0.0, 0.0, 64.0, 64.0));
        assert_eq!(scene.len(), 1);
    }
}
