//! Rysowanie i obsługa wejścia centrum ustawień.
//!
//! Układ jest liczony w jednym miejscu ([`layout`]), więc trafienia klawiszy/myszy
//! i piksele na ekranie nie mogą się rozjechać. Kontrolki pochodzą z
//! `libluna-widgets` (`HeaderBar`, `NavList`, `PreferencesPage`, `Switch`, `Slider`…).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use libluna_core::style::weight;
use libluna_core::{ButtonState, Color, Event, IconTheme, Point, PointerButton, Rect, Size, Theme};
use libluna_i18n::tr;
use libluna_render::wallpaper::decode_png;
use libluna_render::{ImageData, Scene, TextEngine};
use libluna_widgets::{
    clipboard_command, paint_icon, ActionRow, Button, ButtonVariant, CheckButton, ControlsSide,
    HeaderBar, IconName, PaintCtx, PreferencesGroup, PreferencesPage, Slider, Switch,
    TextClipboardCommand, TitleAlign, ViewSwitcher, WindowControl,
};

use crate::app::{SettingsApp, TextPasteTarget};
use crate::pages::{self, AccentTarget, Action, Control, Group, SliderKind};

/// Wysokość wiersza ustawień (spójna dla rysowania i trafień).
pub const ROW_H: f64 = 56.0;
/// Przewinięcie za jeden „ząbek” kółka.
const SCROLL_STEP: f64 = 64.0;
/// Szerokość sidebara.
const SIDEBAR_W: f64 = 236.0;

/// Miniatury tapet (dekodowane raz, przycięte do 16:9).
static THUMBS: OnceLock<Mutex<HashMap<PathBuf, Option<Arc<ImageData>>>>> = OnceLock::new();

fn wallpaper_thumbnail(path: &Path) -> Option<Arc<ImageData>> {
    let cache = THUMBS.get_or_init(Default::default);
    if let Ok(cache) = cache.lock() {
        if let Some(hit) = cache.get(path) {
            return hit.clone();
        }
    }
    let image = std::fs::read(path)
        .ok()
        .and_then(|bytes| decode_png(&bytes))
        .map(|image| Arc::new(cover_thumbnail(&image, 192, 108)));
    if let Ok(mut cache) = cache.lock() {
        cache.insert(path.to_path_buf(), image.clone());
    }
    image
}

/// Skaluje i przycina obraz „cover” do `width × height` (najbliższy sąsiad).
fn cover_thumbnail(image: &ImageData, width: u32, height: u32) -> ImageData {
    let mut out = vec![0u8; (width * height * 4) as usize];
    if image.width == 0 || image.height == 0 {
        return ImageData::new(width, height, out);
    }
    let scale = (width as f64 / image.width as f64).max(height as f64 / image.height as f64);
    let src_w = width as f64 / scale;
    let src_h = height as f64 / scale;
    let off_x = (image.width as f64 - src_w) / 2.0;
    let off_y = (image.height as f64 - src_h) / 2.0;
    for y in 0..height {
        for x in 0..width {
            let sx = (off_x + (x as f64 + 0.5) / scale).clamp(0.0, image.width as f64 - 1.0) as u32;
            let sy =
                (off_y + (y as f64 + 0.5) / scale).clamp(0.0, image.height as f64 - 1.0) as u32;
            let src = ((sy * image.width + sx) * 4) as usize;
            let dst = ((y * width + x) * 4) as usize;
            out[dst..dst + 4].copy_from_slice(&image.rgba[src..src + 4]);
        }
    }
    ImageData::new(width, height, out)
}

fn paint_wallpaper_control(
    ctx: &mut PaintCtx,
    control: Rect,
    path: &Path,
    selected: bool,
    theme: &Theme,
) {
    ctx.scene
        .fill_rounded_rect(control, theme.radius.small, theme.colors.shade);
    if let Some(image) = wallpaper_thumbnail(path) {
        ctx.scene.image(control, image);
    }
    ctx.scene.stroke_rounded_rect(
        control,
        theme.radius.small,
        if selected { 2.0 } else { 1.0 },
        if selected {
            theme.colors.accent
        } else {
            theme.colors.border
        },
    );
    if selected {
        let badge = Rect::from_xywh(control.right() - 20.0, control.y() + 4.0, 16.0, 16.0);
        ctx.scene
            .fill_rounded_rect(badge, badge.width() / 2.0, theme.colors.accent);
        paint_icon(
            ctx,
            IconName::Check,
            Rect::from_xywh(badge.x() + 3.0, badge.y() + 3.0, 10.0, 10.0),
            theme.colors.accent_fg,
        );
    }
}

/// Podgląd motywu: miniaturowe okno w barwach akcentu i kilka ikon.
fn paint_theme_preview(
    ctx: &mut PaintCtx,
    rect: Rect,
    accent: libluna_core::Accent,
    icons: &[IconName],
    theme: &Theme,
    icon_theme: IconTheme,
) {
    let radius = theme.radius.medium;
    ctx.scene
        .fill_rounded_rect(rect, radius, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(rect, radius, 1.0, theme.colors.border);

    let bar = Rect::from_xywh(rect.x() + 6.0, rect.y() + 6.0, 6.0, rect.height() - 12.0);
    ctx.scene.fill_rounded_rect(bar, 3.0, accent.color());

    let win = Rect::from_xywh(rect.x() + 20.0, rect.y() + 8.0, 76.0, rect.height() - 16.0);
    ctx.scene
        .fill_rounded_rect(win, theme.radius.small, theme.colors.shade);
    ctx.scene.fill_rect(
        Rect::from_xywh(win.x() + 8.0, win.y() + 8.0, 30.0, 3.0),
        theme.colors.text.scale_alpha(0.55),
    );
    for index in 0..2 {
        ctx.scene.fill_rect(
            Rect::from_xywh(
                win.x() + 8.0,
                win.y() + 17.0 + index as f64 * 7.0,
                44.0 - index as f64 * 10.0,
                3.0,
            ),
            theme.colors.dim_text.scale_alpha(0.5),
        );
    }
    ctx.scene.fill_rounded_rect(
        Rect::from_xywh(win.x() + 8.0, win.bottom() - 14.0, 24.0, 8.0),
        theme.radius.small,
        accent.color(),
    );

    let shown = icons.len().min(4) as f64;
    let size = 18.0;
    let gap = 8.0;
    let total = shown * size + (shown - 1.0).max(0.0) * gap;
    let left = rect.right() - 8.0 - total;
    let mono = icon_theme.is_monochrome() || libluna_core::icons::active_icon_set().is_some();
    for (index, icon) in icons.iter().take(4).enumerate() {
        let r = Rect::from_xywh(
            left + index as f64 * (size + gap),
            rect.center().y - size / 2.0,
            size,
            size,
        );
        if mono {
            paint_icon(ctx, *icon, r, theme.colors.text);
        } else {
            ctx.scene
                .fill_rounded_rect(r, theme.radius.small, accent.color().scale_alpha(0.16));
            paint_icon(
                ctx,
                *icon,
                Rect::from_xywh(r.x() + 4.0, r.y() + 4.0, 10.0, 10.0),
                accent.color(),
            );
        }
    }
}

/// Prostokąty okna.
pub struct Layout {
    pub header: Rect,
    pub sidebar: Rect,
    pub nav: Rect,
    pub content: Rect,
    pub title: Rect,
    pub viewport: Rect,
}

pub fn layout(size: Size, theme: &Theme) -> Layout {
    let header = Rect::from_xywh(0.0, 0.0, size.width, theme.metrics.headerbar.height);
    let sidebar_w = SIDEBAR_W.min((size.width * 0.32).max(200.0));
    let sidebar = Rect::from_xywh(
        0.0,
        header.bottom(),
        sidebar_w,
        (size.height - header.height()).max(0.0),
    );
    let nav = Rect::from_xywh(
        sidebar.x() + 8.0,
        sidebar.y() + 8.0,
        (sidebar.width() - 16.0).max(0.0),
        (sidebar.height() - 40.0).max(0.0),
    );
    let content = Rect::from_xywh(
        sidebar.right(),
        header.bottom(),
        (size.width - sidebar.width()).max(0.0),
        (size.height - header.height()).max(0.0),
    );
    let pad = 32.0_f64.min((content.width() * 0.08).max(16.0));
    let title = Rect::from_xywh(
        content.x() + pad,
        content.y() + 20.0,
        (content.width() - 2.0 * pad).max(0.0),
        52.0,
    );
    let viewport = Rect::from_xywh(
        title.x(),
        title.bottom() + 8.0,
        title.width(),
        (content.bottom() - 24.0 - (title.bottom() + 8.0)).max(0.0),
    );
    Layout {
        header,
        sidebar,
        nav,
        content,
        title,
        viewport,
    }
}

/// Szerokość przycisku „Info” w nagłówku.
const INFO_BUTTON_W: f64 = 64.0;

/// Prostokąt przycisku „Info” w nagłówku (na lewo od kontrolek okna).
fn info_button_rect(lay: &Layout, theme: &Theme) -> Rect {
    let content = HeaderBar::new()
        .controls(ControlsSide::Right)
        .controls_layout(lay.header, theme)
        .content;
    let height = theme
        .metrics
        .button
        .height
        .min((lay.header.height() - 8.0).max(20.0));
    Rect::from_xywh(
        (content.right() - INFO_BUTTON_W - 4.0).max(content.x()),
        lay.header.center().y - height / 2.0,
        INFO_BUTTON_W,
        height,
    )
}

/// Rysuje całą klatkę.
pub fn paint(app: &SettingsApp, scene: &mut Scene, size: Size, theme: &Theme, text: &TextEngine) {
    let lay = layout(size, theme);
    let mut ctx = PaintCtx::new(scene, theme, text);

    ctx.scene
        .fill_rect(Rect::from_size(size), theme.colors.window_bg);
    // Sidebar.
    ctx.scene.fill_rect(lay.sidebar, theme.colors.headerbar_bg);
    ctx.scene.fill_rect(
        Rect::from_xywh(
            lay.sidebar.right() - 1.0,
            lay.sidebar.y(),
            1.0,
            lay.sidebar.height(),
        ),
        theme.colors.border,
    );
    ctx.label_left(
        lay.sidebar.x() + 16.0,
        lay.sidebar.bottom() - 14.0,
        &format!("Luna {}", env!("CARGO_PKG_VERSION")),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );

    // Nagłówek okna (kontrolki po prawej).
    HeaderBar::new()
        .title(app.title.clone())
        .title_align(TitleAlign::Start)
        .controls(ControlsSide::Right)
        .paint(&mut ctx, lay.header, app.hover_control);

    // Przejrzysty dostęp do technicznych informacji bieżącej strony.
    let info_rect = info_button_rect(&lay, theme);
    let info_variant = if app.info_sheet.is_open() {
        ButtonVariant::Suggested
    } else {
        ButtonVariant::Default
    };
    Button::new(tr("Info"))
        .variant(info_variant)
        .paint(&mut ctx, info_rect);

    // Nawigacja.
    app.nav.paint(&mut ctx, lay.nav);

    // Nagłówek strony.
    let icon = Rect::from_xywh(lay.title.x(), lay.title.y() + 9.0, 22.0, 22.0);
    let page_icon_color = if app.icons.theme.is_monochrome() {
        theme.colors.text
    } else {
        theme.colors.accent
    };
    paint_icon(&mut ctx, app.page.icon(), icon, page_icon_color);
    let page_title = tr(app.page.title_key());
    let page_subtitle = tr(app.page.subtitle_key());
    ctx.label_left(
        lay.title.x() + 32.0,
        lay.title.y() + 20.0,
        &page_title,
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    ctx.label_left(
        lay.title.x() + 32.0,
        lay.title.y() + 42.0,
        &page_subtitle,
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );

    // Zawartość strony.
    let groups = pages::content_groups(app);
    let page = preferences_page(&groups);
    let row_h = ROW_H;
    let mut scroll = app.scroll;
    scroll.set_content_height(page.content_height(row_h, theme));
    let offset = scroll.offset().min(scroll.max_offset(lay.viewport));
    scroll.set_offset(offset, lay.viewport);
    let area = Rect::from_xywh(
        lay.viewport.x(),
        lay.viewport.y() - offset,
        lay.viewport.width(),
        scroll.content_height().max(lay.viewport.height()),
    );

    ctx.scene.push_clip(lay.viewport);
    let contents = page.paint(&mut ctx, area, row_h);
    for (group, content) in groups.iter().zip(contents.iter()) {
        for (index, row) in group.rows.iter().enumerate() {
            let rect = PreferencesGroup::row_rect(*content, index, row_h);
            paint_row(
                &mut ctx,
                row,
                rect,
                index + 1 == group.rows.len(),
                app.icons.theme,
            );
        }
    }
    ctx.scene.pop_clip();
    scroll.paint_scrollbar(&mut ctx, lay.viewport);

    if app.info_sheet.is_open() || app.info_sheet.progress() > 0.001 {
        app.info_sheet.paint(&mut ctx, size);
        paint_info_sheet(app, &mut ctx, size);
    }
    if app.picker.is_some() {
        paint_picker(app, &mut ctx, size);
    }
    if app.editor.is_some() {
        paint_editor(app, &mut ctx, size);
    }
    if app.wallpaper_picker.is_some() {
        paint_wallpaper_picker(app, &mut ctx, size);
    }
    if app.wifi_prompt.is_some() {
        paint_wifi_prompt(app, &mut ctx, size);
    }
    if app.admin.is_some() {
        paint_admin(app, &mut ctx, size);
    }
    app.status.paint(&mut ctx, size);
}

fn preferences_page(groups: &[Group]) -> PreferencesPage {
    let mut page = PreferencesPage::new();
    for group in groups {
        page = page.add(PreferencesGroup::new(group.title.clone()), group.rows.len());
    }
    page
}

fn paint_row(ctx: &mut PaintCtx, row: &pages::Row, rect: Rect, last: bool, icon_theme: IconTheme) {
    let theme = *ctx.theme;
    if !last {
        ctx.scene.fill_rect(
            Rect::from_xywh(
                rect.x() + theme.metrics.group.pad_x,
                rect.bottom() - 1.0,
                (rect.width() - 2.0 * theme.metrics.group.pad_x).max(0.0),
                1.0,
            ),
            theme.colors.border.scale_alpha(0.55),
        );
    }

    let width = control_width(&row.control, &theme, ctx.text);
    let text_w =
        (rect.width() - 2.0 * theme.metrics.row.pad_x - width - theme.metrics.row.gap).max(0.0);
    let title = fit_text(
        &row.title,
        text_w,
        theme.typography.body.size,
        weight::NORMAL,
        ctx.text,
    );
    let subtitle = row.subtitle.as_ref().map(|subtitle| {
        fit_text(
            subtitle,
            text_w,
            theme.typography.caption.size,
            weight::NORMAL,
            ctx.text,
        )
    });
    let mut action_row = ActionRow::new(title);
    if let Some(subtitle) = subtitle {
        action_row = action_row.subtitle(subtitle);
    }
    let suffix = action_row.paint(ctx, rect, width);
    let control = control_rect(&row.control, rect, &theme, ctx.text);
    match &row.control {
        Control::None => {}
        Control::Value(value) => ctx.label_right(
            suffix.right(),
            rect.center().y,
            value,
            theme.typography.body.size,
            weight::NORMAL,
            theme.colors.text,
        ),
        Control::Switch(on) => Switch::new(*on).paint(ctx, control),
        Control::Slider { value, .. } => {
            let percent = (value * 100.0).round() as i64;
            ctx.label_right(
                suffix.right(),
                rect.y() + 16.0,
                &format!("{percent}%"),
                theme.typography.caption.size,
                weight::MEDIUM,
                theme.colors.dim_text,
            );
            Slider::new(*value).paint(ctx, control);
        }
        Control::Segmented {
            options, selected, ..
        } => {
            let mut switcher = ViewSwitcher::new(options.clone());
            switcher.set_selected(*selected);
            switcher.paint(ctx, control);
        }
        Control::Accents { selected, .. } => {
            for (index, swatch) in accent_rects(control, &theme).iter().enumerate() {
                ctx.scene.fill_rounded_rect(
                    *swatch,
                    swatch.width() / 2.0,
                    libluna_core::Accent::ALL[index].color(),
                );
                if index == *selected {
                    let ring = Rect::from_xywh(
                        swatch.x() - 2.0,
                        swatch.y() - 2.0,
                        swatch.width() + 4.0,
                        swatch.height() + 4.0,
                    );
                    ctx.scene
                        .stroke_rounded_rect(ring, ring.width() / 2.0, 2.0, theme.colors.text);
                }
            }
        }
        Control::Check(checked) => {
            let mut check = CheckButton::new(String::new());
            check.set_checked(*checked);
            check.paint(ctx, control);
        }
        Control::Select { value } => {
            let button = Button::new(value.clone()).dropdown(true);
            button.paint(ctx, control);
        }
        Control::IconStrip(icons) => {
            let mono = icon_theme.is_monochrome();
            let external = libluna_core::icons::active_icon_set().is_some();
            let size = 20.0;
            let gap = 8.0;
            for (index, icon) in icons.iter().enumerate() {
                let r = Rect::from_xywh(
                    control.x() + index as f64 * (size + gap),
                    control.center().y - size / 2.0,
                    size,
                    size,
                );
                if external {
                    // Zestaw zewnętrzny ma własne kolory — bez tła akcentu.
                    paint_icon(ctx, *icon, r, theme.colors.text);
                } else if mono {
                    paint_icon(ctx, *icon, r, theme.colors.text);
                } else {
                    ctx.scene.fill_rounded_rect(
                        r,
                        theme.radius.small,
                        theme.colors.accent.scale_alpha(0.16),
                    );
                    let inner = Rect::from_xywh(r.x() + 4.0, r.y() + 4.0, 12.0, 12.0);
                    paint_icon(ctx, *icon, inner, theme.colors.accent);
                }
            }
        }
        Control::ThemePreview { accent, icons } => {
            paint_theme_preview(ctx, control, *accent, icons, &theme, icon_theme);
        }
        Control::Wallpaper { path, selected } => {
            paint_wallpaper_control(ctx, control, path, *selected, &theme);
        }
        Control::Button { label, suggested } => {
            let button = Button::new(label.clone()).variant(if *suggested {
                ButtonVariant::Suggested
            } else {
                ButtonVariant::Default
            });
            button.paint(ctx, control);
        }
    }
}

fn fit_text(text: &str, max_w: f64, size: f64, weight: u16, engine: &TextEngine) -> String {
    if max_w <= 0.0 || engine.measure(text, size, weight).width <= max_w {
        return text.to_owned();
    }
    let dots = "…";
    let dots_w = engine.measure(dots, size, weight).width;
    let mut out = String::new();
    for ch in text.chars() {
        let mut probe = out.clone();
        probe.push(ch);
        if engine.measure(&probe, size, weight).width + dots_w > max_w {
            break;
        }
        out = probe;
    }
    out.push('…');
    out
}

fn control_width(control: &Control, theme: &Theme, text: &TextEngine) -> f64 {
    match control {
        Control::None => 0.0,
        Control::Value(value) => {
            text.measure(value, theme.typography.body.size, weight::NORMAL)
                .width
        }
        Control::Switch(on) => Switch::new(*on).size(theme).0,
        Control::Slider { .. } => 220.0,
        Control::Segmented { options, .. } => {
            let caption = theme.typography.caption.size;
            let max = options
                .iter()
                .map(|option| text.measure(option, caption, weight::MEDIUM).width)
                .fold(0.0, f64::max);
            ((max + 24.0) * options.len() as f64).clamp(180.0, 340.0)
        }
        Control::Accents { .. } => {
            let n = libluna_core::Accent::ALL.len() as f64;
            n * 20.0 + (n - 1.0) * 8.0
        }
        Control::Check(_) => theme.metrics.check.size,
        Control::Select { value } => Button::new(value.clone())
            .dropdown(true)
            .width(text, theme)
            .clamp(160.0, 320.0),
        Control::IconStrip(icons) => {
            let n = icons.len() as f64;
            n * 20.0 + (n - 1.0).max(0.0) * 8.0
        }
        Control::ThemePreview { .. } => 248.0,
        Control::Wallpaper { .. } => 96.0,
        Control::Button { label, .. } => Button::new(label.clone()).width(text, theme),
    }
}

fn control_rect(control: &Control, rect: Rect, theme: &Theme, text: &TextEngine) -> Rect {
    let width = control_width(control, theme, text);
    let suffix = Rect::from_xywh(
        rect.right() - theme.metrics.row.pad_x - width,
        rect.y(),
        width,
        rect.height(),
    );
    match control {
        Control::Switch(on) => {
            let (w, h) = Switch::new(*on).size(theme);
            Rect::from_xywh(suffix.right() - w, suffix.center().y - h / 2.0, w, h)
        }
        Control::Slider { .. } => Rect::from_xywh(
            suffix.x(),
            suffix.center().y + 8.0,
            suffix.width(),
            theme.metrics.slider.track_height,
        ),
        Control::Segmented { .. } => {
            Rect::from_xywh(suffix.x(), suffix.center().y - 14.0, suffix.width(), 28.0)
        }
        Control::Accents { .. } => {
            let h = 20.0;
            Rect::from_xywh(suffix.x(), suffix.center().y - h / 2.0, suffix.width(), h)
        }
        Control::Check(_) => {
            let size = theme.metrics.check.size;
            Rect::from_xywh(
                suffix.right() - size,
                suffix.center().y - size / 2.0,
                size,
                size,
            )
        }
        Control::Button { label, .. } => {
            let button = Button::new(label.clone());
            let h = button.height(theme);
            Rect::from_xywh(
                suffix.right() - width,
                suffix.center().y - h / 2.0,
                width,
                h,
            )
        }
        Control::Select { value } => {
            let button = Button::new(value.clone()).dropdown(true);
            let h = button.height(theme);
            Rect::from_xywh(
                suffix.right() - width,
                suffix.center().y - h / 2.0,
                width,
                h,
            )
        }
        Control::IconStrip(_) => Rect::from_xywh(
            suffix.right() - width,
            suffix.center().y - 12.0,
            width,
            24.0,
        ),
        Control::ThemePreview { .. } => Rect::from_xywh(
            suffix.right() - width,
            suffix.center().y - 26.0,
            width,
            52.0,
        ),
        Control::Wallpaper { .. } => Rect::from_xywh(
            suffix.right() - width,
            suffix.center().y - 27.0,
            width,
            54.0,
        ),
        Control::None | Control::Value(_) => suffix,
    }
}

fn accent_rects(strip: Rect, _theme: &Theme) -> Vec<Rect> {
    let size = 20.0;
    let gap = 8.0;
    let n = libluna_core::Accent::ALL.len();
    let total = n as f64 * size + (n as f64 - 1.0) * gap;
    let left = strip.right() - total;
    (0..n)
        .map(|i| {
            Rect::from_xywh(
                left + i as f64 * (size + gap),
                strip.center().y - size / 2.0,
                size,
                size,
            )
        })
        .collect()
}

/// Trafienie wiersza strony.
struct RowHit<'a> {
    row: &'a pages::Row,
    rect: Rect,
    control: Rect,
}

/// Układa wiersze aktywnej strony (z uwzględnieniem przewinięcia).
fn row_hits<'a>(
    groups: &'a [Group],
    viewport: Rect,
    theme: &Theme,
    text: &TextEngine,
    offset: f64,
) -> Vec<RowHit<'a>> {
    let page = preferences_page(groups);
    let area = Rect::from_xywh(
        viewport.x(),
        viewport.y() - offset,
        viewport.width(),
        page.content_height(ROW_H, theme).max(viewport.height()),
    );
    let mut out = Vec::new();
    for (group, group_rect) in groups.iter().zip(page.group_rects(area, ROW_H, theme)) {
        let content = Rect::from_xywh(
            group_rect.x(),
            group_rect.y() + theme.metrics.group.header,
            group_rect.width(),
            (group_rect.height() - theme.metrics.group.header).max(0.0),
        );
        for (index, row) in group.rows.iter().enumerate() {
            let rect = PreferencesGroup::row_rect(content, index, ROW_H);
            let control = control_rect(&row.control, rect, theme, text);
            out.push(RowHit { row, rect, control });
        }
    }
    out
}

/// Obsługuje wejście.
pub fn handle_events(
    app: &mut SettingsApp,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    apply_paste(app, events);
    // Modalny formularz/picker/edytor przechwytuje wejście — pod spodem nic nie reaguje.
    if app.admin.is_some() {
        handle_admin_events(app, events, size, theme);
        return;
    }
    if app.wifi_prompt.is_some() {
        handle_wifi_events(app, events, size, theme);
        return;
    }
    if app.wallpaper_picker.is_some() {
        handle_wallpaper_events(app, events, size, theme);
        return;
    }
    if app.picker.is_some() {
        handle_picker_events(app, events, size, theme);
        return;
    }
    if app.editor.is_some() {
        handle_editor_events(app, events, size, theme);
        return;
    }
    if app.info_sheet.is_open() {
        handle_info_events(app, events, size, theme, text);
        return;
    }

    let lay = layout(size, theme);
    let header_bar = HeaderBar::new().controls(ControlsSide::Right);
    let groups = pages::content_groups(app);
    let page = preferences_page(&groups);
    app.scroll
        .set_content_height(page.content_height(ROW_H, theme));
    app.scroll.set_offset(app.scroll.offset(), lay.viewport);

    for event in events {
        match event {
            Event::PointerMotion { position, .. } => {
                app.hover_control = header_bar.control_at(lay.header, theme, *position);
                if let Some((kind, track)) = app.drag {
                    let value = Slider::new(0.0).value_at(track, *position);
                    let percent = (value * 100.0).round() as u8;
                    app.on_action(match kind {
                        SliderKind::Volume => Action::SetVolume(percent),
                        SliderKind::Brightness => Action::SetBrightness(percent),
                    });
                }
            }
            Event::PointerLeave => {
                app.hover_control = None;
                app.drag = None;
            }
            Event::PointerButton(button) if button.state == ButtonState::Pressed => {
                if button.button != PointerButton::Left {
                    continue;
                }
                let p = button.position;
                if let Some(control) = header_bar.control_at(lay.header, theme, p) {
                    match control {
                        WindowControl::Minimize => app.pending.minimize = true,
                        WindowControl::Maximize => app.pending.maximize = true,
                        WindowControl::Close => app.pending.close = true,
                    }
                    continue;
                }
                let info_rect = info_button_rect(&lay, theme);
                if Button::new(tr("Info")).click_at(info_rect, p) {
                    if app.info_sheet.is_open() {
                        app.close_info_sheet();
                    } else {
                        app.open_info_sheet();
                    }
                    continue;
                }
                if lay.header.contains(p) {
                    app.pending.begin_move = true;
                    continue;
                }
                if lay.nav.contains(p) {
                    if app.nav.click_at(lay.nav, p, theme) {
                        if let Some(page) = app
                            .nav
                            .selected()
                            .and_then(|index| app.nav_pages.get(index).copied().flatten())
                        {
                            app.go_to(page);
                        }
                    }
                    continue;
                }
                if !lay.viewport.contains(p) {
                    continue;
                }

                let hits = row_hits(&groups, lay.viewport, theme, text, app.scroll.offset());
                for hit in hits {
                    handle_click(app, hit, p, theme);
                }
            }
            Event::PointerButton(button) if button.state == ButtonState::Released => {
                if button.button == PointerButton::Left {
                    app.drag = None;
                }
            }
            Event::Scroll(scroll) if lay.viewport.contains(scroll.position) => {
                app.scroll.on_scroll(scroll.dy * SCROLL_STEP, lay.viewport);
            }
            Event::Scroll(_) => {}
            _ => {}
        }
    }
}

/// Rozdziela wklejony tekst do pola, które zgłosiło Ctrl+V.
fn apply_paste(app: &mut SettingsApp, events: &[Event]) {
    for event in events {
        let Event::Paste(text) = event else {
            continue;
        };
        let Some(target) = app.paste_target.take() else {
            break;
        };
        match target {
            TextPasteTarget::PickerQuery => picker_insert(app, text),
            TextPasteTarget::EditorEntry => editor_insert(app, text),
            TextPasteTarget::AdminPassword => admin_insert(app, text),
            TextPasteTarget::WifiPassword => wifi_insert(app, text),
        }
    }
}

fn apply_text_clipboard(app: &mut SettingsApp, cmd: TextClipboardCommand, target: TextPasteTarget) {
    match cmd {
        TextClipboardCommand::Copy(text) | TextClipboardCommand::Cut(text) => {
            if let Some(text) = text {
                app.clipboard_text = Some(text);
            }
        }
        TextClipboardCommand::Paste => {
            app.paste_target = Some(target);
            app.paste_requested = true;
        }
    }
}

fn handle_click(app: &mut SettingsApp, hit: RowHit, p: Point, theme: &Theme) {
    match &hit.row.control {
        Control::None | Control::Value(_) => {
            if hit.row.action != Action::None && hit.rect.contains(p) {
                app.on_action(hit.row.action.clone());
            }
        }
        Control::Select { .. } => {
            if hit.control.contains(p) || hit.rect.contains(p) {
                app.on_action(hit.row.action.clone());
            }
        }
        Control::IconStrip(_) => {}
        Control::ThemePreview { .. } => {}
        Control::Switch(_) => {
            // Klik w cały wiersz jest wygodniejszy niż trafianie w mały przełącznik.
            if hit.rect.contains(p) || hit.control.contains(p) {
                app.on_action(hit.row.action.clone());
            }
        }
        Control::Slider { kind, value } => {
            let slider = Slider::new(*value);
            if slider.hit(hit.control, p, theme) {
                let percent = (slider.value_at(hit.control, p) * 100.0).round() as u8;
                app.drag = Some((*kind, hit.control));
                app.on_action(match kind {
                    SliderKind::Volume => Action::SetVolume(percent),
                    SliderKind::Brightness => Action::SetBrightness(percent),
                });
            }
        }
        Control::Segmented {
            options, actions, ..
        } => {
            let switcher = ViewSwitcher::new(options.clone());
            if let Some(index) = switcher.index_at(hit.control, p) {
                if let Some(action) = actions.get(index) {
                    app.on_action(action.clone());
                }
            }
        }
        Control::Accents { target, .. } => {
            for (index, swatch) in accent_rects(hit.control, theme).iter().enumerate() {
                if swatch.contains(p) {
                    let accent = libluna_core::Accent::ALL[index];
                    app.on_action(match target {
                        AccentTarget::Desktop => Action::SetAccent(accent),
                        AccentTarget::Greeter => Action::SetGreeterAccent(accent),
                        AccentTarget::Lock => Action::SetLockAccent(accent),
                    });
                    break;
                }
            }
        }
        Control::Check(_) => {
            // Cały wiersz jest wygodnym celem — tak jak przy przełączniku.
            if hit.rect.contains(p) || hit.control.contains(p) {
                app.on_action(hit.row.action.clone());
            }
        }
        Control::Wallpaper { .. } => {
            if hit.rect.contains(p) || hit.control.contains(p) {
                app.on_action(hit.row.action.clone());
            }
        }
        Control::Button { .. } => {
            let button = match &hit.row.control {
                Control::Button { label, .. } => Button::new(label.clone()),
                _ => return,
            };
            if button.click_at(hit.control, p) {
                app.on_action(hit.row.action.clone());
            }
        }
    }
}

// ── Wyszukiwany wybór (picker) i mały edytor tekstu ──

const PICKER_ROW_H: f64 = 32.0;

struct PickerLayout {
    card: Rect,
    title: Rect,
    search: Rect,
    list: Rect,
    hint: Rect,
}

fn picker_layout(size: Size, _theme: &Theme) -> PickerLayout {
    let w = (size.width - 120.0).clamp(320.0, 560.0);
    let h = (size.height - 140.0).clamp(260.0, 480.0);
    let card = Rect::from_xywh((size.width - w) / 2.0, (size.height - h) / 2.0, w, h);
    let title = Rect::from_xywh(card.x() + 16.0, card.y() + 12.0, card.width() - 32.0, 30.0);
    let search = Rect::from_xywh(
        card.x() + 16.0,
        title.bottom() + 8.0,
        card.width() - 32.0,
        36.0,
    );
    let hint = Rect::from_xywh(
        card.x() + 16.0,
        card.bottom() - 30.0,
        card.width() - 32.0,
        20.0,
    );
    let list = Rect::from_xywh(
        card.x() + 8.0,
        search.bottom() + 8.0,
        card.width() - 16.0,
        (hint.y() - search.bottom() - 16.0).max(0.0),
    );
    PickerLayout {
        card,
        title,
        search,
        list,
        hint,
    }
}

fn picker_row_at(app: &SettingsApp, lay: &PickerLayout, p: Point) -> Option<usize> {
    if !lay.list.contains(p) {
        return None;
    }
    let picker = app.picker.as_ref()?;
    let index = ((p.y - lay.list.y() + picker.scroll) / PICKER_ROW_H).floor() as usize;
    (index < app.picker_filtered().len()).then_some(index)
}

fn picker_max_scroll(app: &SettingsApp, lay: &PickerLayout) -> f64 {
    let count = app.picker_filtered().len() as f64;
    (count * PICKER_ROW_H - lay.list.height()).max(0.0)
}

fn picker_insert(app: &mut SettingsApp, text: &str) {
    let plain: String = text.chars().filter(|c| !c.is_control()).collect();
    if plain.is_empty() {
        return;
    }
    if let Some(picker) = &mut app.picker {
        picker.query.insert_str(&plain);
        picker.selected = 0;
        picker.scroll = 0.0;
    }
}

fn picker_move(app: &mut SettingsApp, lay: &PickerLayout, delta: i64) {
    let count = app.picker_filtered().len();
    if count == 0 {
        return;
    }
    let Some(picker) = &mut app.picker else {
        return;
    };
    let next = (picker.selected as i64 + delta).clamp(0, count as i64 - 1) as usize;
    picker.selected = next;
    let top = next as f64 * PICKER_ROW_H;
    if top < picker.scroll {
        picker.scroll = top;
    } else if top + PICKER_ROW_H > picker.scroll + lay.list.height() {
        picker.scroll = (top + PICKER_ROW_H - lay.list.height()).max(0.0);
    }
}

fn handle_picker_events(app: &mut SettingsApp, events: &[Event], size: Size, theme: &Theme) {
    let lay = picker_layout(size, theme);
    for event in events {
        match event {
            Event::PointerMotion { position, .. } => {
                app.picker_hover = picker_row_at(app, &lay, *position);
            }
            Event::PointerLeave => app.picker_hover = None,
            Event::PointerButton(button)
                if button.state == ButtonState::Pressed && button.button == PointerButton::Left =>
            {
                let p = button.position;
                if !lay.card.contains(p) {
                    app.picker = None;
                    continue;
                }
                if lay.search.contains(p) {
                    if let Some(picker) = &mut app.picker {
                        picker.query.set_focused(true);
                    }
                } else if let Some(index) = picker_row_at(app, &lay, p) {
                    if let Some(picker) = &mut app.picker {
                        picker.selected = index;
                    }
                    app.apply_picker();
                }
            }
            Event::Scroll(scroll) if lay.list.contains(scroll.position) => {
                let max = picker_max_scroll(app, &lay);
                if let Some(picker) = &mut app.picker {
                    picker.scroll = (picker.scroll + scroll.dy * PICKER_ROW_H).clamp(0.0, max);
                }
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                let clipboard = app
                    .picker
                    .as_mut()
                    .and_then(|picker| clipboard_command(&mut picker.query, key));
                if let Some(cmd) = clipboard {
                    apply_text_clipboard(app, cmd, TextPasteTarget::PickerQuery);
                    continue;
                }
                match key.keysym {
                    0xff1b => app.picker = None,
                    0xff0d | 0xff8d => app.apply_picker(),
                    0xff52 => picker_move(app, &lay, -1),
                    0xff54 => picker_move(app, &lay, 1),
                    0xff50 => picker_move(app, &lay, -(app.picker_filtered().len() as i64)),
                    0xff57 => picker_move(app, &lay, app.picker_filtered().len() as i64),
                    0xff08 => {
                        if let Some(picker) = &mut app.picker {
                            picker.query.backspace();
                            picker.selected = 0;
                            picker.scroll = 0.0;
                        }
                    }
                    0x61 if key.modifiers.ctrl => {
                        if let Some(picker) = &mut app.picker {
                            picker.query.select_all();
                        }
                    }
                    _ => {
                        if !key.modifiers.ctrl {
                            if let Some(text) = &key.text {
                                picker_insert(app, text);
                            }
                        }
                    }
                }
            }
            Event::Text(text) => picker_insert(app, text),
            _ => {}
        }
    }
}

fn paint_picker(app: &SettingsApp, ctx: &mut PaintCtx, size: Size) {
    let Some(picker) = &app.picker else {
        return;
    };
    let theme = *ctx.theme;
    let lay = picker_layout(size, &theme);
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(lay.card, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.card, theme.radius.large, 1.0, theme.colors.border);

    let title = match picker.kind {
        pages::PickerKind::Language => tr("Interface language"),
        pages::PickerKind::Locale => tr("Region and formats"),
        pages::PickerKind::Timezone => tr("Time zone"),
        pages::PickerKind::IconSet => tr("External icon set"),
    };
    ctx.label_left(
        lay.title.x(),
        lay.title.center().y,
        &title,
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );

    let mut query = picker.query.clone();
    query.set_focused(true);
    query.paint(ctx, lay.search);

    let filtered = app.picker_filtered();
    if filtered.is_empty() {
        ctx.label_center(
            lay.list.center().x,
            lay.list.center().y,
            &tr("No results"),
            theme.typography.body.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    } else {
        ctx.scene.push_clip(lay.list);
        let current = match picker.kind {
            pages::PickerKind::Language => app.language.clone(),
            pages::PickerKind::Locale => app.region.locale.clone().unwrap_or_default(),
            pages::PickerKind::Timezone => app.region.timezone.clone().unwrap_or_default(),
            pages::PickerKind::IconSet => app.icons.external.clone().unwrap_or_default(),
        };
        for (index, item) in filtered.iter().enumerate() {
            let row = Rect::from_xywh(
                lay.list.x() + 4.0,
                lay.list.y() + index as f64 * PICKER_ROW_H - picker.scroll,
                lay.list.width() - 8.0,
                PICKER_ROW_H,
            );
            if row.bottom() < lay.list.y() || row.y() > lay.list.bottom() {
                continue;
            }
            if index == picker.selected {
                ctx.scene.fill_rounded_rect(
                    row,
                    theme.radius.small,
                    theme.colors.accent.scale_alpha(0.18),
                );
                ctx.scene
                    .stroke_rounded_rect(row, theme.radius.small, 1.5, theme.colors.accent);
            } else if app.picker_hover == Some(index) {
                ctx.scene.fill_rounded_rect(
                    row,
                    theme.radius.small,
                    theme.colors.shade.scale_alpha(1.8),
                );
            }
            ctx.label_left(
                row.x() + 10.0,
                row.center().y,
                &item.label,
                theme.typography.body.size,
                weight::NORMAL,
                theme.colors.text,
            );
            if item.value == current {
                let check = Rect::from_xywh(row.right() - 22.0, row.center().y - 7.0, 14.0, 14.0);
                paint_icon(ctx, IconName::Check, check, theme.colors.accent);
            }
        }
        ctx.scene.pop_clip();
    }

    ctx.label_center(
        lay.hint.center().x,
        lay.hint.center().y,
        &tr("↑↓ select · Enter confirm · Esc close"),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
}

struct EditorLayout {
    card: Rect,
    title: Rect,
    entry: Rect,
    save: Rect,
    cancel: Rect,
}

fn editor_layout(size: Size, theme: &Theme) -> EditorLayout {
    let w = (size.width - 160.0).clamp(320.0, 520.0);
    let h = 190.0_f64.min(size.height - 80.0);
    let card = Rect::from_xywh((size.width - w) / 2.0, (size.height - h) / 2.0, w, h);
    let title = Rect::from_xywh(card.x() + 20.0, card.y() + 14.0, card.width() - 40.0, 30.0);
    let entry = Rect::from_xywh(
        card.x() + 20.0,
        title.bottom() + 10.0,
        card.width() - 40.0,
        36.0,
    );
    let button_h = theme.metrics.button.height;
    let save = Rect::from_xywh(
        card.right() - 20.0 - 96.0,
        card.bottom() - 16.0 - button_h,
        96.0,
        button_h,
    );
    let cancel = Rect::from_xywh(save.x() - 8.0 - 96.0, save.y(), 96.0, button_h);
    EditorLayout {
        card,
        title,
        entry,
        save,
        cancel,
    }
}

fn editor_insert(app: &mut SettingsApp, text: &str) {
    let plain: String = text.chars().filter(|c| !c.is_control()).collect();
    if plain.is_empty() {
        return;
    }
    if let Some(editor) = &mut app.editor {
        editor.entry.insert_str(&plain);
    }
}

fn handle_editor_events(app: &mut SettingsApp, events: &[Event], size: Size, theme: &Theme) {
    let lay = editor_layout(size, theme);
    for event in events {
        match event {
            Event::PointerButton(button)
                if button.state == ButtonState::Pressed && button.button == PointerButton::Left =>
            {
                let p = button.position;
                if lay.save.contains(p) {
                    app.save_workspace_editor();
                } else if lay.cancel.contains(p) || !lay.card.contains(p) {
                    app.editor = None;
                } else if lay.entry.contains(p) {
                    if let Some(editor) = &mut app.editor {
                        editor.entry.set_focused(true);
                    }
                }
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                let clipboard = app
                    .editor
                    .as_mut()
                    .and_then(|editor| clipboard_command(&mut editor.entry, key));
                if let Some(cmd) = clipboard {
                    apply_text_clipboard(app, cmd, TextPasteTarget::EditorEntry);
                    continue;
                }
                match key.keysym {
                    0xff1b => app.editor = None,
                    0xff0d | 0xff8d => app.save_workspace_editor(),
                    0xff08 => {
                        if let Some(editor) = &mut app.editor {
                            editor.entry.backspace();
                        }
                    }
                    0x61 if key.modifiers.ctrl => {
                        if let Some(editor) = &mut app.editor {
                            editor.entry.select_all();
                        }
                    }
                    _ => {
                        if !key.modifiers.ctrl {
                            if let Some(text) = &key.text {
                                editor_insert(app, text);
                            }
                        }
                    }
                }
            }
            Event::Text(text) => editor_insert(app, text),
            _ => {}
        }
    }
}

struct AdminLayout {
    card: Rect,
    title: Rect,
    subtitle: Rect,
    entry: Rect,
    error: Rect,
    save: Rect,
    cancel: Rect,
}

fn admin_layout(size: Size, theme: &Theme) -> AdminLayout {
    let w = (size.width - 160.0).clamp(360.0, 560.0);
    let h = 244.0_f64.min(size.height - 80.0);
    let card = Rect::from_xywh((size.width - w) / 2.0, (size.height - h) / 2.0, w, h);
    let title = Rect::from_xywh(card.x() + 20.0, card.y() + 14.0, card.width() - 40.0, 28.0);
    let subtitle = Rect::from_xywh(
        card.x() + 20.0,
        title.bottom() + 2.0,
        card.width() - 40.0,
        20.0,
    );
    let entry = Rect::from_xywh(
        card.x() + 20.0,
        subtitle.bottom() + 12.0,
        card.width() - 40.0,
        36.0,
    );
    let error = Rect::from_xywh(
        card.x() + 20.0,
        entry.bottom() + 6.0,
        card.width() - 40.0,
        18.0,
    );
    let button_h = theme.metrics.button.height;
    let save = Rect::from_xywh(
        card.right() - 20.0 - 96.0,
        card.bottom() - 16.0 - button_h,
        96.0,
        button_h,
    );
    let cancel = Rect::from_xywh(save.x() - 8.0 - 96.0, save.y(), 96.0, button_h);
    AdminLayout {
        card,
        title,
        subtitle,
        entry,
        error,
        save,
        cancel,
    }
}

fn admin_insert(app: &mut SettingsApp, text: &str) {
    let plain: String = text.chars().filter(|c| !c.is_control()).collect();
    if plain.is_empty() {
        return;
    }
    if let Some(prompt) = &mut app.admin {
        if !prompt.is_pending() {
            prompt.password.insert_str(&plain);
        }
    }
}

fn handle_admin_events(app: &mut SettingsApp, events: &[Event], size: Size, theme: &Theme) {
    let lay = admin_layout(size, theme);
    for event in events {
        match event {
            Event::PointerButton(button)
                if button.state == ButtonState::Pressed && button.button == PointerButton::Left =>
            {
                let p = button.position;
                if lay.save.contains(p) {
                    app.submit_admin_password();
                } else if lay.cancel.contains(p) || !lay.card.contains(p) {
                    app.admin = None;
                } else if lay.entry.contains(p) {
                    if let Some(prompt) = &mut app.admin {
                        prompt.password.set_focused(true);
                    }
                }
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                let clipboard = app
                    .admin
                    .as_mut()
                    .filter(|prompt| !prompt.is_pending())
                    .and_then(|prompt| clipboard_command(&mut prompt.password, key));
                if let Some(cmd) = clipboard {
                    apply_text_clipboard(app, cmd, TextPasteTarget::AdminPassword);
                    continue;
                }
                match key.keysym {
                    0xff1b => app.admin = None,
                    0xff0d | 0xff8d => app.submit_admin_password(),
                    0xff08 => {
                        if let Some(prompt) = &mut app.admin {
                            if !prompt.is_pending() {
                                prompt.password.backspace();
                            }
                        }
                    }
                    0x61 if key.modifiers.ctrl => {
                        if let Some(prompt) = &mut app.admin {
                            if !prompt.is_pending() {
                                prompt.password.select_all();
                            }
                        }
                    }
                    _ => {
                        if !key.modifiers.ctrl {
                            if let Some(text) = &key.text {
                                admin_insert(app, text);
                            }
                        }
                    }
                }
            }
            Event::Text(text) => admin_insert(app, text),
            _ => {}
        }
    }
}

fn paint_admin(app: &SettingsApp, ctx: &mut PaintCtx, size: Size) {
    let Some(prompt) = &app.admin else {
        return;
    };
    let theme = *ctx.theme;
    let lay = admin_layout(size, &theme);
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(lay.card, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.card, theme.radius.large, 1.0, theme.colors.border);
    ctx.label_left(
        lay.title.x(),
        lay.title.center().y,
        &tr("Administrator password"),
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    ctx.label_left(
        lay.subtitle.x(),
        lay.subtitle.center().y,
        &tr("Changing the login screen requires administrator approval"),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    let mut password = prompt.password.clone();
    password.set_focused(true);
    password.paint(ctx, lay.entry);
    if let Some(error) = &prompt.error {
        ctx.label_left(
            lay.error.x(),
            lay.error.center().y,
            error,
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.warning,
        );
    } else if prompt.is_pending() {
        ctx.label_left(
            lay.error.x(),
            lay.error.center().y,
            &tr("Applying changes…"),
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    }
    Button::new(tr("Apply changes"))
        .variant(ButtonVariant::Suggested)
        .paint(ctx, lay.save);
    Button::new(tr("Cancel")).paint(ctx, lay.cancel);
}

fn paint_editor(app: &SettingsApp, ctx: &mut PaintCtx, size: Size) {
    let Some(editor) = &app.editor else {
        return;
    };
    let theme = *ctx.theme;
    let lay = editor_layout(size, &theme);
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(lay.card, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.card, theme.radius.large, 1.0, theme.colors.border);
    ctx.label_left(
        lay.title.x(),
        lay.title.center().y,
        &editor.title,
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    let mut entry = editor.entry.clone();
    entry.set_focused(true);
    entry.paint(ctx, lay.entry);
    Button::new(tr("Save"))
        .variant(ButtonVariant::Suggested)
        .paint(ctx, lay.save);
    Button::new(tr("Cancel")).paint(ctx, lay.cancel);
}

// ── Panel Info (BottomSheet) ──

/// Wewnętrzny obszar treści panelu Info.
fn info_content_rect(app: &SettingsApp, size: Size, theme: &Theme) -> Rect {
    let content = app.info_sheet.content_rect(size, theme);
    Rect::from_xywh(
        content.x() + 20.0,
        content.y() + 8.0,
        (content.width() - 40.0).max(0.0),
        (content.height() - 16.0).max(0.0),
    )
}

fn paint_info_sheet(app: &SettingsApp, ctx: &mut PaintCtx, size: Size) {
    let theme = *ctx.theme;
    let content = info_content_rect(app, size, &theme);
    let groups = pages::info_groups(app);
    if groups.is_empty() {
        ctx.label_center(
            content.center().x,
            content.center().y,
            &tr("No additional information for this page"),
            theme.typography.body.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
        return;
    }

    let page = preferences_page(&groups);
    let mut scroll = app.info_scroll;
    scroll.set_content_height(page.content_height(ROW_H, &theme));
    let offset = scroll.offset().min(scroll.max_offset(content));
    let area = Rect::from_xywh(
        content.x(),
        content.y() - offset,
        content.width(),
        scroll.content_height().max(content.height()),
    );

    ctx.scene.push_clip(content);
    let contents = page.paint(ctx, area, ROW_H);
    for (group, group_rect) in groups.iter().zip(contents.iter()) {
        for (index, row) in group.rows.iter().enumerate() {
            let rect = PreferencesGroup::row_rect(*group_rect, index, ROW_H);
            paint_row(
                ctx,
                row,
                rect,
                index + 1 == group.rows.len(),
                app.icons.theme,
            );
        }
    }
    ctx.scene.pop_clip();
    scroll.paint_scrollbar(ctx, content);
}

fn handle_info_events(
    app: &mut SettingsApp,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let groups = pages::info_groups(app);
    let page = preferences_page(&groups);
    let content = info_content_rect(app, size, theme);
    app.info_scroll
        .set_content_height(page.content_height(ROW_H, theme));
    app.info_scroll
        .set_offset(app.info_scroll.offset(), content);

    for event in events {
        match event {
            Event::PointerMotion { position, .. } => {
                app.info_sheet.update_drag(position.y);
            }
            Event::PointerButton(button)
                if button.state == ButtonState::Pressed && button.button == PointerButton::Left =>
            {
                let p = button.position;
                if app.info_sheet.close_at(size, theme, p)
                    || app.info_sheet.scrim_at(size, theme, p)
                {
                    app.close_info_sheet();
                } else if app.info_sheet.drag_zone_at(size, theme, p) {
                    app.info_sheet.begin_drag(p.y);
                } else if content.contains(p) {
                    let hits = row_hits(&groups, content, theme, text, app.info_scroll.offset());
                    for hit in hits {
                        handle_click(app, hit, p, theme);
                    }
                }
            }
            Event::PointerButton(button)
                if button.state == ButtonState::Released
                    && button.button == PointerButton::Left =>
            {
                if app.info_sheet.end_drag(theme) {
                    app.close_info_sheet();
                }
            }
            Event::Scroll(scroll) if content.contains(scroll.position) => {
                app.info_scroll.on_scroll(scroll.dy * SCROLL_STEP, content);
            }
            Event::Key(key) if key.state == ButtonState::Pressed && key.keysym == 0xff1b => {
                app.close_info_sheet();
            }
            _ => {}
        }
    }
}

// ── Wybór tapety z dysku ──

const WALLPAPER_ROW_H: f64 = 34.0;

struct WallpaperLayout {
    card: Rect,
    title: Rect,
    path: Rect,
    list: Rect,
    hint: Rect,
    choose: Rect,
    cancel: Rect,
}

fn wallpaper_layout(size: Size, theme: &Theme) -> WallpaperLayout {
    let w = (size.width - 120.0).clamp(360.0, 640.0);
    let h = (size.height - 120.0).clamp(320.0, 540.0);
    let card = Rect::from_xywh((size.width - w) / 2.0, (size.height - h) / 2.0, w, h);
    let title = Rect::from_xywh(card.x() + 20.0, card.y() + 14.0, card.width() - 40.0, 28.0);
    let path = Rect::from_xywh(
        card.x() + 20.0,
        title.bottom() + 2.0,
        card.width() - 40.0,
        20.0,
    );
    let button_h = theme.metrics.button.height;
    let choose = Rect::from_xywh(
        card.right() - 20.0 - 96.0,
        card.bottom() - 16.0 - button_h,
        96.0,
        button_h,
    );
    let cancel = Rect::from_xywh(choose.x() - 8.0 - 96.0, choose.y(), 96.0, button_h);
    let hint = Rect::from_xywh(
        card.x() + 20.0,
        choose.y() - 26.0,
        card.width() - 40.0,
        18.0,
    );
    let list = Rect::from_xywh(
        card.x() + 12.0,
        path.bottom() + 8.0,
        card.width() - 24.0,
        (hint.y() - path.bottom() - 14.0).max(0.0),
    );
    WallpaperLayout {
        card,
        title,
        path,
        list,
        hint,
        choose,
        cancel,
    }
}

fn wallpaper_row_at(app: &SettingsApp, lay: &WallpaperLayout, p: Point) -> Option<usize> {
    let picker = app.wallpaper_picker.as_ref()?;
    if !lay.list.contains(p) {
        return None;
    }
    let index = ((p.y - lay.list.y() + picker.scroll) / WALLPAPER_ROW_H).floor() as usize;
    (index < picker.entries.len()).then_some(index)
}

fn wallpaper_max_scroll(app: &SettingsApp, lay: &WallpaperLayout) -> f64 {
    let count = app
        .wallpaper_picker
        .as_ref()
        .map(|picker| picker.entries.len())
        .unwrap_or(0) as f64;
    (count * WALLPAPER_ROW_H - lay.list.height()).max(0.0)
}

fn handle_wallpaper_events(app: &mut SettingsApp, events: &[Event], size: Size, theme: &Theme) {
    let lay = wallpaper_layout(size, theme);
    for event in events {
        match event {
            Event::PointerMotion { position, .. } => {
                let hover = wallpaper_row_at(app, &lay, *position);
                if let Some(picker) = &mut app.wallpaper_picker {
                    picker.hover = hover;
                }
            }
            Event::PointerLeave => {
                if let Some(picker) = &mut app.wallpaper_picker {
                    picker.hover = None;
                }
            }
            Event::PointerButton(button)
                if button.state == ButtonState::Pressed && button.button == PointerButton::Left =>
            {
                let p = button.position;
                if lay.cancel.contains(p) || !lay.card.contains(p) {
                    app.wallpaper_picker = None;
                } else if lay.choose.contains(p) {
                    app.choose_wallpaper();
                } else if let Some(index) = wallpaper_row_at(app, &lay, p) {
                    app.wallpaper_pick(index);
                }
            }
            Event::Scroll(scroll) if lay.list.contains(scroll.position) => {
                let max = wallpaper_max_scroll(app, &lay);
                if let Some(picker) = &mut app.wallpaper_picker {
                    picker.scroll = (picker.scroll + scroll.dy * WALLPAPER_ROW_H).clamp(0.0, max);
                }
            }
            Event::Key(key) if key.state == ButtonState::Pressed => match key.keysym {
                0xff1b => app.wallpaper_picker = None,
                0xff0d | 0xff8d => app.choose_wallpaper(),
                0xff52 => wallpaper_move(app, -1, lay.list.height()),
                0xff54 => wallpaper_move(app, 1, lay.list.height()),
                0xff50 => wallpaper_move(app, -8, lay.list.height()),
                0xff57 => wallpaper_move(app, 8, lay.list.height()),
                _ => {}
            },
            _ => {}
        }
    }
}

fn wallpaper_move(app: &mut SettingsApp, delta: i64, view_h: f64) {
    let Some(picker) = &mut app.wallpaper_picker else {
        return;
    };
    if picker.entries.is_empty() {
        return;
    }
    let next = (picker.selected as i64 + delta).clamp(0, picker.entries.len() as i64 - 1) as usize;
    picker.selected = next;
    let top = next as f64 * WALLPAPER_ROW_H;
    if top < picker.scroll {
        picker.scroll = top;
    } else if top + WALLPAPER_ROW_H > picker.scroll + view_h {
        picker.scroll = (top + WALLPAPER_ROW_H - view_h).max(0.0);
    }
}

fn paint_wallpaper_picker(app: &SettingsApp, ctx: &mut PaintCtx, size: Size) {
    let Some(picker) = &app.wallpaper_picker else {
        return;
    };
    let theme = *ctx.theme;
    let lay = wallpaper_layout(size, &theme);
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(lay.card, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.card, theme.radius.large, 1.0, theme.colors.border);

    ctx.label_left(
        lay.title.x(),
        lay.title.center().y,
        &tr("Choose a wallpaper"),
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    let path = fit_text(
        &picker.dir.display().to_string(),
        lay.path.width(),
        theme.typography.caption.size,
        weight::NORMAL,
        ctx.text,
    );
    ctx.label_left(
        lay.path.x(),
        lay.path.center().y,
        &path,
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );

    if picker.entries.is_empty() {
        ctx.label_center(
            lay.list.center().x,
            lay.list.center().y,
            &tr("No PNG images in this folder"),
            theme.typography.body.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    } else {
        ctx.scene.push_clip(lay.list);
        for (index, entry) in picker.entries.iter().enumerate() {
            let row = Rect::from_xywh(
                lay.list.x() + 4.0,
                lay.list.y() + index as f64 * WALLPAPER_ROW_H - picker.scroll,
                lay.list.width() - 8.0,
                WALLPAPER_ROW_H,
            );
            if row.bottom() < lay.list.y() || row.y() > lay.list.bottom() {
                continue;
            }
            if index == picker.selected {
                ctx.scene.fill_rounded_rect(
                    row,
                    theme.radius.small,
                    theme.colors.accent.scale_alpha(0.18),
                );
                ctx.scene
                    .stroke_rounded_rect(row, theme.radius.small, 1.5, theme.colors.accent);
            } else if picker.hover == Some(index) {
                ctx.scene.fill_rounded_rect(
                    row,
                    theme.radius.small,
                    theme.colors.shade.scale_alpha(1.8),
                );
            }

            let is_parent = index == 0 && picker.dir.parent().is_some();
            let is_dir = entry.is_dir();
            let label = if is_parent {
                "..".to_owned()
            } else if is_dir {
                format!(
                    "{}/",
                    entry
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("?")
                )
            } else {
                entry
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("wallpaper.png")
                    .to_owned()
            };
            let icon = if is_dir || is_parent {
                IconName::Files
            } else {
                IconName::Image
            };
            let icon_rect = Rect::from_xywh(row.x() + 8.0, row.center().y - 8.0, 16.0, 16.0);
            paint_icon(ctx, icon, icon_rect, theme.colors.dim_text);
            ctx.label_left(
                row.x() + 32.0,
                row.center().y,
                &fit_text(
                    &label,
                    (row.width() - 40.0).max(0.0),
                    theme.typography.body.size,
                    weight::NORMAL,
                    ctx.text,
                ),
                theme.typography.body.size,
                weight::NORMAL,
                theme.colors.text,
            );
        }
        ctx.scene.pop_clip();
    }

    ctx.label_left(
        lay.hint.x(),
        lay.hint.center().y,
        &tr("Only PNG images are supported by the login screen"),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    let selected_is_dir = picker
        .selected_path()
        .map(|path| path.is_dir())
        .unwrap_or(false);
    Button::new(if selected_is_dir {
        tr("Open")
    } else {
        tr("Choose")
    })
    .variant(ButtonVariant::Suggested)
    .paint(ctx, lay.choose);
    Button::new(tr("Cancel")).paint(ctx, lay.cancel);
}

// ── Formularz hasła Wi-Fi ──

fn wifi_insert(app: &mut SettingsApp, text: &str) {
    let plain: String = text.chars().filter(|c| !c.is_control()).collect();
    if plain.is_empty() {
        return;
    }
    if let Some(prompt) = &mut app.wifi_prompt {
        if !prompt.is_pending() {
            prompt.password.insert_str(&plain);
        }
    }
}

fn handle_wifi_events(app: &mut SettingsApp, events: &[Event], size: Size, theme: &Theme) {
    let lay = admin_layout(size, theme);
    for event in events {
        match event {
            Event::PointerButton(button)
                if button.state == ButtonState::Pressed && button.button == PointerButton::Left =>
            {
                let p = button.position;
                if lay.save.contains(p) {
                    app.submit_wifi_password();
                } else if lay.cancel.contains(p) || !lay.card.contains(p) {
                    app.wifi_prompt = None;
                } else if lay.entry.contains(p) {
                    if let Some(prompt) = &mut app.wifi_prompt {
                        prompt.password.set_focused(true);
                    }
                }
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                let clipboard = app
                    .wifi_prompt
                    .as_mut()
                    .filter(|prompt| !prompt.is_pending())
                    .and_then(|prompt| clipboard_command(&mut prompt.password, key));
                if let Some(cmd) = clipboard {
                    apply_text_clipboard(app, cmd, TextPasteTarget::WifiPassword);
                    continue;
                }
                match key.keysym {
                    0xff1b => app.wifi_prompt = None,
                    0xff0d | 0xff8d => app.submit_wifi_password(),
                    0xff08 => {
                        if let Some(prompt) = &mut app.wifi_prompt {
                            if !prompt.is_pending() {
                                prompt.password.backspace();
                            }
                        }
                    }
                    0x61 if key.modifiers.ctrl => {
                        if let Some(prompt) = &mut app.wifi_prompt {
                            if !prompt.is_pending() {
                                prompt.password.select_all();
                            }
                        }
                    }
                    _ => {
                        if !key.modifiers.ctrl {
                            if let Some(text) = &key.text {
                                wifi_insert(app, text);
                            }
                        }
                    }
                }
            }
            Event::Text(text) => wifi_insert(app, text),
            _ => {}
        }
    }
}

fn paint_wifi_prompt(app: &SettingsApp, ctx: &mut PaintCtx, size: Size) {
    let Some(prompt) = &app.wifi_prompt else {
        return;
    };
    let theme = *ctx.theme;
    let lay = admin_layout(size, &theme);
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(lay.card, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.card, theme.radius.large, 1.0, theme.colors.border);
    ctx.label_left(
        lay.title.x(),
        lay.title.center().y,
        &tr("Wi-Fi password"),
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    ctx.label_left(
        lay.subtitle.x(),
        lay.subtitle.center().y,
        &format!("{}: {}", tr("Network"), prompt.ssid),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    let mut password = prompt.password.clone();
    password.set_focused(true);
    password.paint(ctx, lay.entry);
    if let Some(error) = &prompt.error {
        ctx.label_left(
            lay.error.x(),
            lay.error.center().y,
            error,
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.warning,
        );
    } else if prompt.is_pending() {
        ctx.label_left(
            lay.error.x(),
            lay.error.center().y,
            &tr("Connecting…"),
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    }
    Button::new(tr("Connect"))
        .variant(ButtonVariant::Suggested)
        .paint(ctx, lay.save);
    Button::new(tr("Cancel")).paint(ctx, lay.cancel);
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_core::{KeyEvent, Modifiers, PointerEvent, Theme, ThemeSettings};

    fn key(keysym: u32, text: Option<&str>) -> Event {
        Event::Key(KeyEvent {
            keysym,
            keycode: 0,
            text: text.map(str::to_owned),
            state: ButtonState::Pressed,
            modifiers: Modifiers::NONE,
            repeat: false,
        })
    }

    #[test]
    fn info_button_opens_sheet_and_escape_closes() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.page = pages::Page::Workspaces;
        let lay = layout(size, &theme);
        let rect = info_button_rect(&lay, &theme);
        let click = Event::PointerButton(PointerEvent {
            position: rect.center(),
            button: PointerButton::Left,
            state: ButtonState::Pressed,
            modifiers: Modifiers::NONE,
        });
        handle_events(&mut app, &[click], size, &theme, &text);
        assert!(app.info_sheet.is_open(), "przycisk Info ma otworzyć panel");
        handle_events(&mut app, &[key(0xff1b, None)], size, &theme, &text);
        assert!(!app.info_sheet.is_open(), "Esc ma zamknąć panel Info");
    }

    #[test]
    fn wallpaper_picker_and_info_sheet_paint() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.open_wallpaper_picker();
        app.info_sheet.set_open(true);
        let mut scene = Scene::new();
        paint(&app, &mut scene, size, &theme, &text);
        assert!(!scene.is_empty());
    }

    #[test]
    fn picker_typing_filters_and_escape_closes() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.open_picker(pages::PickerKind::Language);
        handle_events(
            &mut app,
            &[key(0x64, Some("d")), key(0x65, Some("e"))],
            size,
            &theme,
            &text,
        );
        let filtered = app.picker_filtered();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].value, "de");
        handle_events(&mut app, &[key(0xff1b, None)], size, &theme, &text);
        assert!(app.picker.is_none());
    }

    #[test]
    fn clipboard_works_in_picker_query() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.open_picker(pages::PickerKind::Language);
        {
            let picker = app.picker.as_mut().unwrap();
            picker.query.set_text("de");
            picker.query.select_all();
        }

        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        let copy = Event::Key(KeyEvent {
            keysym: 0x63,
            keycode: 0,
            text: None,
            state: ButtonState::Pressed,
            modifiers: ctrl,
            repeat: false,
        });
        handle_events(&mut app, &[copy], size, &theme, &text);
        assert_eq!(app.clipboard_text.as_deref(), Some("de"));

        let paste = Event::Key(KeyEvent {
            keysym: 0x76,
            keycode: 0,
            text: None,
            state: ButtonState::Pressed,
            modifiers: ctrl,
            repeat: false,
        });
        handle_events(&mut app, &[paste], size, &theme, &text);
        assert!(app.paste_requested);
        assert_eq!(app.paste_target, Some(TextPasteTarget::PickerQuery));

        handle_events(
            &mut app,
            &[Event::Paste("pl".to_owned())],
            size,
            &theme,
            &text,
        );
        assert_eq!(app.picker.as_ref().unwrap().query.text(), "pl");
    }

    #[test]
    fn workspace_editor_accepts_typing_and_escape_cancels() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.open_workspace_editor(0);
        handle_events(&mut app, &[key(0x4b, Some("K"))], size, &theme, &text);
        assert!(app.editor.as_ref().unwrap().entry.text().contains('K'));
        handle_events(&mut app, &[key(0xff1b, None)], size, &theme, &text);
        assert!(app.editor.is_none());
    }

    #[test]
    fn admin_prompt_paints_and_escape_closes() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.on_action(Action::ApplyGreeter);
        assert!(app.admin.is_some());
        let mut scene = Scene::new();
        paint(&app, &mut scene, size, &theme, &text);
        assert!(!scene.is_empty());
        handle_events(&mut app, &[key(0xff1b, None)], size, &theme, &text);
        assert!(app.admin.is_none());
    }

    #[test]
    fn layout_keeps_content_inside_window() {
        let theme = Theme::dark();
        let lay = layout(Size::new(900.0, 600.0), &theme);
        assert!(lay.header.contains(Point::new(100.0, 10.0)));
        assert!(lay.nav.right() <= lay.sidebar.right());
        assert!(lay.viewport.right() <= 900.0);
        assert!(lay.viewport.bottom() <= 600.0);
        assert!(lay.viewport.height() > 0.0);
    }

    #[test]
    fn every_page_paints_and_rows_hit() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let mut app = SettingsApp::new(&ThemeSettings::load());
        let size = Size::new(1080.0, 760.0);
        let lay = layout(size, &theme);
        for page in pages::Page::ALL {
            app.page = page;
            let groups = pages::groups(&app);
            let hits = row_hits(&groups, lay.viewport, &theme, &text, 0.0);
            assert!(!hits.is_empty(), "strona {page:?} bez wierszy");
            let mut scene = Scene::new();
            paint(&app, &mut scene, size, &theme, &text);
            assert!(!scene.is_empty());
        }
    }

    #[test]
    fn control_rects_stay_inside_rows() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.page = pages::Page::Appearance;
        let groups = pages::groups(&app);
        let lay = layout(Size::new(1080.0, 760.0), &theme);
        for hit in row_hits(&groups, lay.viewport, &theme, &text, 0.0) {
            assert!(
                hit.control.x() >= hit.rect.x() && hit.control.right() <= hit.rect.right(),
                "kontrolka poza wierszem"
            );
        }
    }
}
