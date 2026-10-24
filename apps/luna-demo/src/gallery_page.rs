//! Strona „Widgety” w demo — pełna galeria komponentów libLuny.
//!
//! To ta sama zawartość, którą pokazują `docs/images/gallery.png` i
//! `docs/images/new-widgets.png`: `NavigationRail`, `ColumnView`, `Calendar`,
//! przyciski/pola/wybory, `InfoBar`, `MediaControls`, `Picture`, `Expander`,
//! `Paned`, `Assistant`, `Scrollbar` i `Popover`.

use std::sync::{Arc, OnceLock};

use libluna_core::style::weight;
use libluna_core::{Color, Rect};
use libluna_render::ImageData;
use libluna_widgets::{
    Adjustment, Assistant, Avatar, Banner, BannerKind, Calendar, ColorButton, ColorChooser, Column,
    ColumnView, ContentFit, EmojiChooser, Expander, FlowBox, Image, ImageIcon, InfoBar,
    InfoBarKind, LinkButton, MediaControls, MenuButton, NavigationRail, Orientation, PaintCtx,
    Paned, PasswordEntry, Picture, Popover, PopoverPlacement, ProgressBar, Scrollbar, SpinButton,
    Spinner, ToggleButton,
};

/// Symbole rysowane fontem interfejsu (emoji kolorowe wymagają osobnego fontu).
const SYMBOLS: [&str; 12] = ["★", "♥", "♪", "☀", "✓", "✗", "✚", "●", "◆", "■", "▲", "☰"];

/// Rysuje galerię widgetów w podanym prostokącie (współrzędne wewnątrz `area`).
pub fn paint(paint: &mut PaintCtx, area: Rect) {
    let theme = *paint.theme;
    paint.scene.fill_rect(area, theme.colors.window_bg);

    let r = |x: f64, y: f64, w: f64, h: f64| Rect::from_xywh(area.x() + x, area.y() + y, w, h);
    let caption = |paint: &mut PaintCtx, x: f64, y: f64, text: &str| {
        paint.label_left(
            area.x() + x,
            area.y() + y + 8.0,
            text,
            paint.theme.typography.caption.size,
            weight::BOLD,
            paint.theme.colors.dim_text,
        );
    };

    // ── Wiersz 1: nawigacja, tabela, kalendarz ──
    caption(paint, 16.0, 4.0, "NavigationRail / ColumnView / Calendar");

    let rail = NavigationRail::new(["Overview", "Network", "System", "About"]);
    rail.paint(paint, r(16.0, 26.0, 180.0, 180.0));

    let mut table = ColumnView::new(vec![
        Column::new("Name").weight(2.0),
        Column::new("Size"),
        Column::new("Date"),
    ]);
    table.set_rows(5);
    table.set_selected(Some(1));
    let table_rect = r(216.0, 26.0, 420.0, 180.0);
    table.paint(paint, table_rect);
    for (i, name) in [
        "Projects",
        "Report.pdf",
        "Notes.txt",
        "Budget.xlsx",
        "Backup",
    ]
    .iter()
    .enumerate()
    {
        let cell = table.cell_rect(table_rect, &theme, i, 0);
        paint.label_left(
            cell.x() + theme.spacing.md,
            cell.center().y,
            name,
            theme.typography.body.size,
            weight::NORMAL,
            theme.colors.text,
        );
    }

    let calendar = Calendar::new(2024, 1);
    let weekdays: Vec<String> = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    calendar.paint(
        paint,
        r(656.0, 26.0, 288.0, 180.0),
        "January 2024",
        &weekdays,
    );

    // ── Wiersz 2: przyciski, pola, choosery ──
    caption(paint, 16.0, 212.0, "Buttons / Entries / Choosers");

    let toggle = ToggleButton::new("Bold").active(true);
    toggle.paint(paint, r(16.0, 234.0, 90.0, toggle.height(&theme)));
    LinkButton::new("Documentation").paint(paint, r(116.0, 234.0, 130.0, 34.0));
    ColorButton::new(Color::from_rgba8(0x91, 0x41, 0xac, 0xff))
        .paint(paint, r(256.0, 238.0, 28.0, 26.0));
    MenuButton::burger().paint(paint, r(296.0, 234.0, 40.0, 34.0));
    MenuButton::new("Options").paint(paint, r(344.0, 234.0, 110.0, 34.0));
    SpinButton::new(65.0, 0.0, 100.0).paint(paint, r(464.0, 234.0, 110.0, 34.0));

    let mut password = PasswordEntry::new("Password");
    password.insert_str("hunter2");
    password.paint(paint, r(588.0, 234.0, 160.0, 34.0));

    for (i, icon) in [
        ImageIcon::Folder,
        ImageIcon::File,
        ImageIcon::Search,
        ImageIcon::Burger,
    ]
    .into_iter()
    .enumerate()
    {
        Image::new(icon)
            .size(28.0)
            .paint(paint, r(760.0 + i as f64 * 36.0, 234.0, 28.0, 34.0));
    }

    let mut chooser = ColorChooser::new(ColorChooser::default_palette());
    chooser.set_columns(4);
    let ch = chooser.height(220.0, &theme).min(96.0);
    chooser.paint(paint, r(16.0, 276.0, 220.0, ch));

    let symbols: Vec<String> = SYMBOLS.iter().map(|s| s.to_string()).collect();
    let mut emoji = EmojiChooser::new(symbols);
    emoji.set_columns(6);
    emoji.set_selected(1);
    let eh = emoji.height(400.0, &theme).min(96.0);
    emoji.paint(paint, r(256.0, 276.0, 400.0, eh));

    let mut flow = FlowBox::new(56.0);
    flow.set_count(8);
    flow.set_selected(Some(2));
    flow.paint(paint, r(676.0, 276.0, 252.0, 96.0));

    // ── Wiersz 3: InfoBar (4 rodzaje) ──
    caption(
        paint,
        16.0,
        396.0,
        "InfoBar — Info / Question / Warning / Error",
    );

    let bars = [
        (InfoBarKind::Info, "Nowa wersja jest dostępna", Some("OK")),
        (
            InfoBarKind::Question,
            "Zsynchronizować ustawienia?",
            Some("OK"),
        ),
        (InfoBarKind::Warning, "Mało miejsca na dysku", Some("OK")),
        (
            InfoBarKind::Error,
            "Nie udało się zapisać pliku",
            Some("Ponów"),
        ),
    ];
    for (i, (kind, msg, action)) in bars.iter().enumerate() {
        let mut bar = InfoBar::new(*msg).kind(*kind);
        if let Some(action) = action {
            bar = bar.action(*action);
        }
        let (col, row) = (i % 2, i / 2);
        bar.paint(
            paint,
            r(
                16.0 + col as f64 * 462.0,
                418.0 + row as f64 * 56.0,
                450.0,
                52.0,
            ),
        );
    }

    // ── Wiersz 4: MediaControls + Picture ──
    caption(paint, 16.0, 538.0, "MediaControls");
    caption(paint, 640.0, 538.0, "Picture — Contain / Cover / Fill");

    let mut media = MediaControls::new("Aurora", "Luna Ensemble");
    media.set_times("1:42", "4:07");
    media.set_position(0.41);
    media.paint(paint, r(16.0, 560.0, 600.0, 58.0));

    let mut paused = MediaControls::new("Nokturn", "Selene");
    paused.set_playing(false);
    paused.set_times("0:15", "3:20");
    paused.set_position(0.08);
    paused.paint(paint, r(16.0, 624.0, 600.0, 58.0));

    let image = gradient();
    for (i, fit) in [ContentFit::Contain, ContentFit::Cover, ContentFit::Fill]
        .into_iter()
        .enumerate()
    {
        let mut pic = Picture::with_image(image.clone());
        pic.set_fit(fit);
        let rect = r(640.0, 560.0 + i as f64 * 58.0, 288.0, 52.0);
        paint.scene.fill_rect(rect, theme.colors.view_bg);
        pic.paint(paint, rect);
        paint
            .scene
            .stroke_rounded_rect(rect, 0.0, 1.0, theme.colors.border);
    }

    // ── Pasek statusu: Banner + Spinner + ProgressBar + Avatar ──
    Banner::new("Nowa wersja libLuny")
        .kind(BannerKind::Info)
        .description("Wszystkie widgety z galerii są już dostępne.")
        .action("OK")
        .paint(paint, r(16.0, 688.0, 420.0, 52.0));

    let mut spinner = Spinner::new();
    spinner.set_progress(0.35);
    spinner.paint(paint, r(456.0, 702.0, 24.0, 24.0));
    ProgressBar::new()
        .fraction(0.65)
        .text("65%")
        .paint(paint, r(490.0, 710.0, 100.0, 8.0));
    Avatar::new("PL").paint(paint, r(606.0, 696.0, 32.0, 32.0));

    // ── Wiersz 5: Expander / Paned / Assistant + Scrollbar / Popover ──
    caption(
        paint,
        16.0,
        750.0,
        "Expander / Paned / Assistant / Scrollbar / Popover",
    );

    let expander = Expander::new("Advanced options").expanded(true);
    let exp_rect = r(16.0, 772.0, 220.0, 80.0);
    expander.paint(paint, exp_rect);
    let content = expander.content_rect(exp_rect, &theme);
    paint.label_left(
        content.x() + theme.spacing.md,
        content.y() + 16.0,
        "Extra settings…",
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );

    let mut paned = Paned::new(Orientation::Horizontal);
    paned.set_position(0.4);
    let paned_rect = r(256.0, 772.0, 300.0, 80.0);
    let (a, b) = paned.panes(paned_rect, &theme);
    paint.scene.fill_rect(a, theme.colors.shade);
    paint.scene.fill_rect(b, theme.colors.card_bg);
    paned.paint(paint, paned_rect);

    Assistant::new(["Welcome", "Details", "Done"]).paint(
        paint,
        r(576.0, 772.0, 352.0, 80.0),
        "Cancel",
        "Next",
    );

    let mut bar = Scrollbar::new(Orientation::Horizontal);
    let mut adj = Adjustment::new(0.0, 200.0, 60.0);
    adj.set_fraction(0.35);
    bar.set_adjustment(adj);
    bar.paint(paint, r(16.0, 862.0, 300.0, 24.0));

    let popover = Popover::new(PopoverPlacement::Above);
    let pop_rect = r(560.0, 856.0, 200.0, 56.0);
    popover.paint(paint, pop_rect);
    paint.label_left(
        pop_rect.x() + theme.spacing.md,
        pop_rect.center().y,
        "Popover content",
        theme.typography.body.size,
        weight::NORMAL,
        theme.colors.text,
    );
}

/// Obraz gradientowy do demonstracji `Picture` (tworzony raz, współdzielony).
fn gradient() -> Arc<ImageData> {
    static IMAGE: OnceLock<Arc<ImageData>> = OnceLock::new();
    IMAGE
        .get_or_init(|| {
            let (w, h) = (160u32, 100u32);
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h {
                for x in 0..w {
                    let r = (x * 255 / w.max(1)) as u8;
                    let g = (y * 255 / h.max(1)) as u8;
                    rgba.extend_from_slice(&[r, g, 200, 255]);
                }
            }
            Arc::new(ImageData::new(w, h, rgba))
        })
        .clone()
}

/// Maksymalna wysokość strony galerii (do ewentualnego przewijania w przyszłości).
#[allow(dead_code)]
pub fn content_height() -> f64 {
    860.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_core::Size;
    use libluna_render::{Scene, SoftwareRenderer, TextEngine};

    /// Podgląd galerii do PNG — pozwala obejrzeć stronę bez kompozytora.
    #[test]
    #[ignore]
    fn render_gallery_preview() {
        let theme = libluna_core::Theme::dark();
        let text = TextEngine::system();
        let size = Size::new(960.0, 940.0);
        let mut scene = Scene::new();
        {
            let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
            paint(&mut ctx, Rect::from_size(size));
        }
        let mut renderer = SoftwareRenderer::new(960, 940);
        renderer.draw(&scene, 1.0, theme.colors.window_bg);
        let png = renderer.encode_png().expect("PNG");
        std::fs::write("/tmp/luna-gallery-page.png", png).expect("zapis PNG");
    }

    /// Podgląd całego okna (sidebar + header + galeria) do PNG.
    #[test]
    #[ignore]
    fn render_demo_widgets_window() {
        use crate::ui::{build_scene, Demo, WIDGETS_NAV};
        use libluna_core::{Accent, Theme, ThemePreference};

        let theme = Theme::dark();
        let text = TextEngine::system();
        let size = Size::new(1200.0, 1028.0);
        let mut demo = Demo::default();
        demo.nav_selected = Some(WIDGETS_NAV);
        let mut scene = Scene::new();
        build_scene(
            &mut scene,
            &theme,
            &text,
            size,
            &demo,
            ThemePreference::Dark,
            Accent::default(),
        );
        let mut renderer = SoftwareRenderer::new(1200, 1028);
        renderer.draw(&scene, 1.0, theme.colors.window_bg);
        let png = renderer.encode_png().expect("PNG");
        std::fs::write("/tmp/luna-demo-widgets.png", png).expect("zapis PNG");
    }
}
