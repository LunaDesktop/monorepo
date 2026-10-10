//! Galeria nowych widgetów libLuny (cz. 5–12) — render headless.
//!
//! Uruchom: `cargo run -p luna-demo --bin gallery`
//! Efekt:   `docs/images/gallery.png`

use std::error::Error;

use libluna_core::style::weight;
use libluna_core::{Color, Rect, Size, Theme};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use libluna_widgets::{
    Adjustment, Assistant, Calendar, ColorButton, ColorChooser, Column, ColumnView, EmojiChooser,
    Expander, FlowBox, Image, ImageIcon, LinkButton, MenuButton, NavigationRail, Orientation,
    PaintCtx, Paned, PasswordEntry, Popover, PopoverPlacement, Scrollbar, SpinButton, Statusbar,
    ToggleButton,
};

const W: u32 = 960;
const H: u32 = 760;

/// Symbole rysowane przez font interfejsu (emoji kolorowe wymagają osobnego fontu).
const SYMBOLS: [&str; 12] = ["★", "♥", "♪", "☀", "✓", "✗", "✚", "●", "◆", "■", "▲", "☰"];

fn main() -> Result<(), Box<dyn Error>> {
    let theme = Theme::light();
    let text = TextEngine::system();
    let size = Size::new(W as f64, H as f64);

    let mut scene = Scene::new();
    let mut paint = PaintCtx {
        scene: &mut scene,
        theme: &theme,
        text: &text,
    };
    paint
        .scene
        .fill_rect(Rect::from_size(size), theme.colors.window_bg);

    // — Wiersz 1: rail, tabela, kalendarz —
    caption(
        &mut paint,
        16.0,
        14.0,
        "NavigationRail / ColumnView / Calendar",
    );

    let rail = NavigationRail::new(["Overview", "Network", "System", "About"]);
    rail.paint(&mut paint, Rect::from_xywh(16.0, 40.0, 180.0, 200.0));

    let mut table = ColumnView::new(vec![
        Column::new("Name").weight(2.0),
        Column::new("Size"),
        Column::new("Date"),
    ]);
    table.set_rows(5);
    table.set_selected(Some(1));
    let table_rect = Rect::from_xywh(216.0, 40.0, 420.0, 200.0);
    table.paint(&mut paint, table_rect);
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
        &mut paint,
        Rect::from_xywh(656.0, 40.0, 288.0, 200.0),
        "January 2024",
        &weekdays,
    );

    // — Wiersz 2: przyciski, pola, chooser’y —
    caption(&mut paint, 16.0, 256.0, "Buttons / Entries / Choosers");

    let toggle = ToggleButton::new("Bold").active(true);
    let toggle_rect = Rect::from_xywh(16.0, 282.0, 90.0, toggle.height(&theme));
    toggle.paint(&mut paint, toggle_rect);

    LinkButton::new("Documentation").paint(&mut paint, Rect::from_xywh(116.0, 282.0, 130.0, 34.0));

    ColorButton::new(Color::from_rgba8(0x91, 0x41, 0xac, 0xff))
        .paint(&mut paint, Rect::from_xywh(256.0, 286.0, 28.0, 26.0));

    MenuButton::burger().paint(&mut paint, Rect::from_xywh(296.0, 282.0, 40.0, 34.0));
    MenuButton::new("Options").paint(&mut paint, Rect::from_xywh(344.0, 282.0, 110.0, 34.0));

    SpinButton::new(65.0, 0.0, 100.0).paint(&mut paint, Rect::from_xywh(464.0, 282.0, 110.0, 34.0));

    let mut password = PasswordEntry::new("Password");
    password.insert_str("hunter2");
    password.paint(&mut paint, Rect::from_xywh(588.0, 282.0, 160.0, 34.0));

    for (i, icon) in [
        ImageIcon::Folder,
        ImageIcon::File,
        ImageIcon::Search,
        ImageIcon::Burger,
    ]
    .into_iter()
    .enumerate()
    {
        Image::new(icon).size(28.0).paint(
            &mut paint,
            Rect::from_xywh(760.0 + i as f64 * 36.0, 282.0, 28.0, 34.0),
        );
    }

    let mut chooser = ColorChooser::new(ColorChooser::default_palette());
    chooser.set_columns(4);
    chooser.paint(
        &mut paint,
        Rect::from_xywh(16.0, 332.0, 220.0, chooser.height(220.0, &theme)),
    );

    let symbols: Vec<String> = SYMBOLS.iter().map(|s| s.to_string()).collect();
    let mut emoji = EmojiChooser::new(symbols);
    emoji.set_columns(6);
    emoji.set_selected(1);
    emoji.paint(
        &mut paint,
        Rect::from_xywh(256.0, 332.0, 400.0, emoji.height(400.0, &theme)),
    );

    let mut flow = FlowBox::new(56.0);
    flow.set_count(8);
    flow.set_selected(Some(2));
    flow.paint(&mut paint, Rect::from_xywh(676.0, 332.0, 268.0, 120.0));

    // — Wiersz 3: expander, paned, assistant, scrollbar, popover —
    caption(
        &mut paint,
        16.0,
        486.0,
        "Expander / Paned / Assistant / Scrollbar / Popover",
    );

    let expander = Expander::new("Advanced options").expanded(true);
    let exp_rect = Rect::from_xywh(16.0, 512.0, 220.0, 120.0);
    expander.paint(&mut paint, exp_rect);
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
    let paned_rect = Rect::from_xywh(256.0, 512.0, 300.0, 120.0);
    let (a, b) = paned.panes(paned_rect, &theme);
    paint.scene.fill_rect(a, theme.colors.shade);
    paint.scene.fill_rect(b, theme.colors.card_bg);
    paned.paint(&mut paint, paned_rect);

    Assistant::new(["Welcome", "Details", "Done"]).paint(
        &mut paint,
        Rect::from_xywh(576.0, 512.0, 368.0, 120.0),
        "Cancel",
        "Next",
    );

    let mut bar = Scrollbar::new(Orientation::Horizontal);
    let mut adj = Adjustment::new(0.0, 200.0, 60.0);
    adj.set_fraction(0.35);
    bar.set_adjustment(adj);
    bar.paint(&mut paint, Rect::from_xywh(16.0, 648.0, 300.0, 24.0));

    let popover = Popover::new(PopoverPlacement::Above);
    let pop_rect = Rect::from_xywh(560.0, 640.0, 200.0, 56.0);
    popover.paint(&mut paint, pop_rect);
    paint.label_left(
        pop_rect.x() + theme.spacing.md,
        pop_rect.center().y,
        "Popover content",
        theme.typography.body.size,
        weight::NORMAL,
        theme.colors.text,
    );

    // — Pasek stanu na dole —
    let mut status = Statusbar::new();
    status.set_message("Ready");
    status.set_right("8 items");
    let sh = status.height(&theme);
    status.paint(
        &mut paint,
        Rect::from_xywh(0.0, size.height - sh, size.width, sh),
    );

    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, theme.colors.window_bg);
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/gallery.png", png)?;
    println!("saved docs/images/gallery.png ({W}x{H})");
    Ok(())
}

fn caption(paint: &mut PaintCtx, x: f64, y: f64, text: &str) {
    paint.label_left(
        x,
        y + 8.0,
        text,
        paint.theme.typography.caption.size,
        weight::BOLD,
        paint.theme.colors.dim_text,
    );
}
