//! `Image` — obrazek (GTK4) oparty na wbudowanych ikonach wektorowych.
//!
//! libLuna nie dekoduje plików graficznych (świadomie — bez balastu kodeków); `Image`
//! pokazuje natomiast zestaw ikon rysowanych wektorowo (folder, plik, lupka, krzyżyk,
//! hamburger), skalowanych do zadanego rozmiaru.

use libluna_core::{Rect, Theme};

use crate::ctx::PaintCtx;
use crate::icon;

/// Zestaw wbudowanych ikon [`Image`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageIcon {
    #[default]
    Folder,
    File,
    Search,
    Close,
    Burger,
}

/// Obrazek z wbudowanej ikony wektorowej.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Image {
    icon: ImageIcon,
    size: f64,
}

impl Default for Image {
    fn default() -> Self {
        Self {
            icon: ImageIcon::Folder,
            size: 48.0,
        }
    }
}

impl Image {
    pub fn new(icon: ImageIcon) -> Self {
        Self {
            icon,
            ..Self::default()
        }
    }

    pub fn icon(&self) -> ImageIcon {
        self.icon
    }

    pub fn set_icon(&mut self, icon: ImageIcon) {
        self.icon = icon;
    }

    pub fn size(mut self, size: f64) -> Self {
        self.size = size.max(0.0);
        self
    }

    pub fn width(&self) -> f64 {
        self.size
    }

    pub fn height(&self) -> f64 {
        self.size
    }

    /// Kwadrat obrazka wyśrodkowany w `rect` (o boku `min(rect, size)`).
    pub fn image_rect(&self, rect: Rect) -> Rect {
        let side = self.size.min(rect.width()).min(rect.height()).max(0.0);
        let c = rect.center();
        Rect::from_xywh(c.x - side / 2.0, c.y - side / 2.0, side, side)
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let box_ = self.image_rect(rect);
        let color = theme.colors.text;
        match self.icon {
            ImageIcon::Folder => icon::folder(ctx, color, box_),
            ImageIcon::File => icon::file(ctx, color, box_),
            ImageIcon::Search => icon::search(ctx, color, box_.center(), box_.width() * 0.9),
            ImageIcon::Close => icon::close(ctx, color, box_, box_.width() * 0.22),
            ImageIcon::Burger => icon::burger(ctx, color, box_.center(), box_.width() * 0.34, 1.8),
        }
    }
}

/// Rozmiar obrazka domyślnie ustawiany przez motyw (bok kwadratu).
pub fn default_size(theme: &Theme) -> f64 {
    theme.spacing.xl * 3.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_rect_is_square_and_centered() {
        let img = Image::new(ImageIcon::Folder).size(40.0);
        let rect = Rect::from_xywh(0.0, 0.0, 100.0, 60.0);
        let r = img.image_rect(rect);
        assert_eq!(r.width(), 40.0);
        assert_eq!(r.height(), 40.0);
        assert_eq!(r.center().x, 50.0);
        assert_eq!(r.center().y, 30.0);
    }

    #[test]
    fn image_shrinks_to_fit_small_rect() {
        let img = Image::new(ImageIcon::File).size(80.0);
        let rect = Rect::from_xywh(0.0, 0.0, 30.0, 20.0);
        let r = img.image_rect(rect);
        assert_eq!(r.width(), 20.0);
        assert_eq!(r.height(), 20.0);
    }

    #[test]
    fn size_defaults_from_integer_scale() {
        let theme = Theme::light();
        assert!(default_size(&theme) > 0.0);
    }
}
