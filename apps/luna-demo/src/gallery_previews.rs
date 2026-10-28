//! Miniaturowe, żywe podglądy widgetów do galerii deweloperskiej.
//!
//! Każda funkcja rysuje prawdziwy widget `libluna-widgets` w prostokącie kafelka
//! (albo — w przypadku pełnoekranowych nakładek, jak dialogi — ich czytelną
//! miniaturę, bo `paint` takich widgetów operuje na całym oknie).

use std::sync::{Arc, OnceLock};

use libluna_core::style::weight;
use libluna_core::{Color, Insets, Point, Rect, Size};
use libluna_render::ImageData;
use libluna_widgets::{
    paint_icon, ActionBar, ActionRow, AspectFrame, Assistant, Avatar, Banner, BannerKind, Bin,
    BinAlign, BreakpointBin, Button, ButtonContent, ButtonVariant, Calendar, Card, CenterBox,
    CheckButton, Clamp, ColorButton, ColorChooser, Column, ColumnView, ControlsSide, DropDown,
    EditableLabel, EmojiChooser, Entry, EntryIcon, Expander, Fixed, FlowBox, Frame, Grid,
    HeaderBar, IconName, Image, ImageIcon, InfoBar, InfoBarKind, Item, ItemView, Label, LabelAlign,
    LevelBar, LinkButton, MediaControls, Menu, MenuButton, MenuEntry, NavEntry, NavList,
    NavigationRail, NavigationView, Overlay, PaintCtx, Paned, PasswordEntry, PathBar, Picture,
    Popover, PopoverMenuBar, PopoverPlacement, PreferencesGroup, PreferencesPage, ProgressBar,
    RadioGroup, Revealer, Row, ScrollView, Scrollbar, SearchBar, SearchEntry, Separator,
    ShortcutLabel, Slider, SpinButton, Spinner, SplitButton, Stack, StatusIcon, StatusPage,
    Statusbar, Stepper, Switch, TabBar, TextView, ToggleButton, ToggleGroup, Tooltip, ViewMode,
    ViewSwitcher, WindowTitle,
};
use libluna_widgets::{
    ContentFit, NavigationSplitView, Orientation, OverlaySplitView, Sidebar, TabView, ToolbarView,
    Viewport, WindowHandle, WrapBox,
};

fn inner(rect: Rect) -> Rect {
    Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 10.0,
        (rect.width() - 24.0).max(0.0),
        (rect.height() - 20.0).max(0.0),
    )
}

fn line(paint: &mut PaintCtx, x: f64, y: f64, text: &str, color: Color) {
    let size = paint.theme.typography.caption.size;
    paint.label_left(x, y, text, size, weight::NORMAL, color);
}

fn centered_h(rect: Rect, h: f64) -> Rect {
    Rect::from_xywh(rect.x(), rect.center().y - h / 2.0, rect.width(), h)
}

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

// ── Podstawy ────────────────────────────────────────────────────────────────

pub fn button(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let h = theme.metrics.button.height;
    let gap = 6.0;
    let w = ((rect.width() - 24.0) - gap * 3.0) / 4.0;
    let y = rect.center().y - h / 2.0;
    let buttons = [
        Button::new("Default"),
        Button::new("Suggested").variant(ButtonVariant::Suggested),
        Button::new("Danger").variant(ButtonVariant::Destructive),
        Button::new("Off").enabled(false),
    ];
    for (i, b) in buttons.iter().enumerate() {
        b.paint(
            paint,
            Rect::from_xywh(rect.x() + 12.0 + i as f64 * (w + gap), y, w, h),
        );
    }
}

pub fn buttons_extra(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let h = 30.0;
    let y = rect.center().y - h / 2.0;
    let toggle = ToggleButton::new("Bold").active(true);
    toggle.paint(paint, Rect::from_xywh(rect.x() + 12.0, y, 64.0, h));
    LinkButton::new("Documentation").paint(paint, Rect::from_xywh(rect.x() + 88.0, y, 120.0, h));
    let color = Color::from_rgba8(0x91, 0x41, 0xac, 0xff);
    ColorButton::new(color).paint(
        paint,
        Rect::from_xywh(rect.x() + 220.0, y + 2.0, 28.0, 26.0),
    );
    line(
        paint,
        rect.x() + 260.0,
        rect.center().y,
        "#9141ac",
        theme.colors.dim_text,
    );
}

pub fn split_button(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let h = theme.metrics.button.height;
    let y = rect.y() + 16.0;
    SplitButton::new("Export").paint(paint, Rect::from_xywh(rect.x() + 12.0, y, 110.0, h));
    MenuButton::new("Options").paint(paint, Rect::from_xywh(rect.x() + 132.0, y, 100.0, h));
}

pub fn menu_button(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let h = theme.metrics.button.height;
    let y = rect.center().y - h / 2.0;
    MenuButton::burger().paint(paint, Rect::from_xywh(rect.x() + 12.0, y, 38.0, h));
    MenuButton::new("Options").paint(paint, Rect::from_xywh(rect.x() + 60.0, y, 110.0, h));
}

pub fn label(paint: &mut PaintCtx, rect: Rect) {
    Label::new("Heading").bold(true).paint(
        paint,
        Rect::from_xywh(rect.x() + 12.0, rect.y() + 8.0, rect.width() - 24.0, 22.0),
    );
    Label::new("Body text").paint(
        paint,
        Rect::from_xywh(rect.x() + 12.0, rect.y() + 36.0, rect.width() - 24.0, 20.0),
    );
    Label::new("End-aligned hint")
        .dim(true)
        .align(LabelAlign::End)
        .paint(
            paint,
            Rect::from_xywh(rect.x() + 12.0, rect.y() + 62.0, rect.width() - 24.0, 20.0),
        );
}

pub fn separator(paint: &mut PaintCtx, rect: Rect) {
    Separator::horizontal().paint(
        paint,
        Rect::from_xywh(
            rect.x() + 12.0,
            rect.center().y - 10.0,
            rect.width() - 24.0,
            20.0,
        ),
    );
    Separator::vertical().paint(
        paint,
        Rect::from_xywh(
            rect.center().x - 10.0,
            rect.y() + 10.0,
            20.0,
            rect.height() - 20.0,
        ),
    );
}

pub fn image(paint: &mut PaintCtx, rect: Rect) {
    let icons = [
        ImageIcon::Folder,
        ImageIcon::File,
        ImageIcon::Search,
        ImageIcon::Burger,
    ];
    for (i, icon) in icons.into_iter().enumerate() {
        Image::new(icon).size(28.0).paint(
            paint,
            Rect::from_xywh(
                rect.x() + 16.0 + i as f64 * 40.0,
                rect.center().y - 16.0,
                28.0,
                32.0,
            ),
        );
    }
}

pub fn avatar(paint: &mut PaintCtx, rect: Rect) {
    Avatar::new("PL").paint(
        paint,
        Rect::from_xywh(rect.x() + 16.0, rect.center().y - 20.0, 40.0, 40.0),
    );
    Avatar::new("LU").paint(
        paint,
        Rect::from_xywh(rect.x() + 68.0, rect.center().y - 15.0, 30.0, 30.0),
    );
    Avatar::new("A").paint(
        paint,
        Rect::from_xywh(rect.x() + 110.0, rect.center().y - 12.0, 24.0, 24.0),
    );
}

pub fn spinner(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let s = Spinner::new().size(&theme);
    for (i, progress) in [0.1, 0.35, 0.7].into_iter().enumerate() {
        let mut spinner = Spinner::new();
        spinner.set_progress(progress);
        spinner.paint(
            paint,
            Rect::from_xywh(
                rect.x() + 20.0 + i as f64 * 48.0,
                rect.center().y - s / 2.0,
                s,
                s,
            ),
        );
    }
}

pub fn progress_level(paint: &mut PaintCtx, rect: Rect) {
    let w = (rect.width() - 40.0).min(240.0);
    ProgressBar::new().fraction(0.65).text("65%").paint(
        paint,
        Rect::from_xywh(rect.x() + 16.0, rect.y() + 22.0, w, 20.0),
    );
    LevelBar::new().value(0.72).segments(5).paint(
        paint,
        Rect::from_xywh(rect.x() + 16.0, rect.y() + 58.0, w, 18.0),
    );
}

pub fn shortcut(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let h = ShortcutLabel::new(["Ctrl", "N"]).height(&theme);
    ShortcutLabel::new(["Ctrl", "N"]).paint(
        paint,
        Rect::from_xywh(rect.x() + 16.0, rect.center().y - h / 2.0, 90.0, h),
    );
    ShortcutLabel::new(["Ctrl", "Shift", "S"]).paint(
        paint,
        Rect::from_xywh(rect.x() + 120.0, rect.center().y - h / 2.0, 150.0, h),
    );
}

pub fn tooltip(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let anchor = Rect::from_xywh(rect.center().x - 50.0, rect.center().y + 14.0, 100.0, 28.0);
    Button::new("Refresh").paint(paint, anchor);
    let tip = Tooltip::new("Reload the list");
    let area = tip.rect(anchor, &theme, paint.text);
    tip.paint(paint, area);
}

pub fn icons(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let names = [
        IconName::Menu,
        IconName::Files,
        IconName::Search,
        IconName::Settings,
        IconName::Music,
        IconName::Moon,
    ];
    for (i, name) in names.into_iter().enumerate() {
        paint_icon(
            paint,
            name,
            Rect::from_xywh(
                rect.x() + 16.0 + i as f64 * 40.0,
                rect.center().y - 12.0,
                24.0,
                24.0,
            ),
            theme.colors.text,
        );
    }
}

// ── Formularze ──────────────────────────────────────────────────────────────

pub fn entry(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let h = theme.metrics.entry.height;
    let mut first = Entry::new("Name")
        .leading(EntryIcon::Search)
        .clearable(true);
    first.set_text("Luna");
    first.paint(
        paint,
        Rect::from_xywh(rect.x() + 12.0, rect.y() + 12.0, rect.width() - 24.0, h),
    );
    Entry::new("Search").paint(
        paint,
        Rect::from_xywh(
            rect.x() + 12.0,
            rect.y() + 12.0 + h + 8.0,
            rect.width() - 24.0,
            h,
        ),
    );
}

pub fn editable_label(paint: &mut PaintCtx, rect: Rect) {
    let label = EditableLabel::new("Project name");
    label.paint(
        paint,
        Rect::from_xywh(rect.x() + 12.0, rect.y() + 14.0, rect.width() - 24.0, 28.0),
    );
    let mut editing = EditableLabel::new("Renamed project");
    editing.start_editing();
    editing.paint(
        paint,
        Rect::from_xywh(rect.x() + 12.0, rect.y() + 52.0, rect.width() - 24.0, 28.0),
    );
}

pub fn password(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let h = theme.metrics.entry.height;
    let mut masked = PasswordEntry::new("Password");
    masked.insert_str("hunter2");
    masked.paint(
        paint,
        Rect::from_xywh(rect.x() + 12.0, rect.y() + 12.0, rect.width() - 24.0, h),
    );
    let mut peek = PasswordEntry::new("Password");
    peek.insert_str("hunter2");
    peek.set_peek(true);
    peek.paint(
        paint,
        Rect::from_xywh(
            rect.x() + 12.0,
            rect.y() + 12.0 + h + 8.0,
            rect.width() - 24.0,
            h,
        ),
    );
}

pub fn search_entry(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let h = theme.metrics.entry.height;
    let mut search = SearchEntry::new("Search files");
    search.insert_str("report");
    search.set_focused(true);
    search.paint(
        paint,
        Rect::from_xywh(
            rect.x() + 12.0,
            rect.center().y - h / 2.0,
            rect.width() - 24.0,
            h,
        ),
    );
}

pub fn search_bar(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let bar = SearchBar::with_mode(true);
    let h = bar.height(&theme);
    let area = centered_h(inner(rect), h);
    bar.paint(paint, area);
    let mut entry = SearchEntry::new("Search");
    entry.insert_str("luna");
    entry.set_focused(true);
    entry.paint(paint, bar.entry_rect(area, &theme));
}

pub fn text_view(paint: &mut PaintCtx, rect: Rect) {
    let mut view = TextView::new("Write a note…");
    view.insert_str("Notatka\nDruga linia");
    view.paint(
        paint,
        Rect::from_xywh(
            rect.x() + 12.0,
            rect.y() + 8.0,
            rect.width() - 24.0,
            rect.height() - 16.0,
        ),
    );
}

pub fn spin_button(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let h = SpinButton::new(8.0, 1.0, 32.0).height(&theme);
    SpinButton::new(8.0, 1.0, 32.0).paint(
        paint,
        Rect::from_xywh(rect.x() + 16.0, rect.center().y - h / 2.0, 120.0, h),
    );
    SpinButton::new(65.0, 0.0, 100.0).paint(
        paint,
        Rect::from_xywh(rect.x() + 150.0, rect.center().y - h / 2.0, 120.0, h),
    );
}

pub fn stepper(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let step = Stepper::new(32, 8, 256).step(8);
    let h = step.height(&theme);
    step.paint(
        paint,
        Rect::from_xywh(rect.x() + 16.0, rect.center().y - h / 2.0, 120.0, h),
    );
    let mut step = Stepper::new(3, 0, 10);
    step.inc();
    step.paint(
        paint,
        Rect::from_xywh(rect.x() + 150.0, rect.center().y - h / 2.0, 100.0, h),
    );
}

pub fn slider(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let track_h = theme.metrics.slider.track_height;
    Slider::new(0.25).paint(
        paint,
        Rect::from_xywh(
            rect.x() + 16.0,
            rect.y() + 34.0,
            rect.width() - 32.0,
            track_h,
        ),
    );
    Slider::new(0.7).paint(
        paint,
        Rect::from_xywh(
            rect.x() + 16.0,
            rect.y() + 72.0,
            rect.width() - 32.0,
            track_h,
        ),
    );
}

pub fn switch(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let (w, h) = Switch::new(true).size(&theme);
    for (i, on) in [false, true, true].into_iter().enumerate() {
        Switch::new(on).paint(
            paint,
            Rect::from_xywh(
                rect.x() + 16.0 + i as f64 * 70.0,
                rect.center().y - h / 2.0,
                w,
                h,
            ),
        );
    }
}

pub fn check_button(paint: &mut PaintCtx, rect: Rect) {
    let mut first = CheckButton::new("Show hidden files");
    first.set_checked(true);
    first.paint(
        paint,
        Rect::from_xywh(rect.x() + 16.0, rect.y() + 14.0, rect.width() - 32.0, 24.0),
    );
    CheckButton::new("Auto-save").paint(
        paint,
        Rect::from_xywh(rect.x() + 16.0, rect.y() + 44.0, rect.width() - 32.0, 24.0),
    );
    CheckButton::new("Disabled").paint(
        paint,
        Rect::from_xywh(rect.x() + 16.0, rect.y() + 74.0, rect.width() - 32.0, 24.0),
    );
}

pub fn radio_group(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let mut group = RadioGroup::new(["Name", "Size", "Date"]);
    group.set_selected(1);
    let h = group.height(&theme);
    group.paint(
        paint,
        Rect::from_xywh(rect.x() + 16.0, rect.center().y - h / 2.0, 180.0, h),
    );
}

pub fn dropdown(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let h = DropDown::new(["Light"]).height(&theme);
    DropDown::new(["Light", "Dark", "Auto"]).paint(
        paint,
        Rect::from_xywh(rect.x() + 16.0, rect.center().y - h / 2.0, 150.0, h),
    );
    DropDown::new(["English", "Polski"]).paint(
        paint,
        Rect::from_xywh(rect.x() + 180.0, rect.center().y - h / 2.0, 150.0, h),
    );
}

pub fn toggle_group(paint: &mut PaintCtx, rect: Rect) {
    let group = ToggleGroup::new(["B", "I", "U"])
        .with_active(0, true)
        .with_active(2, true);
    group.paint(
        paint,
        Rect::from_xywh(rect.x() + 16.0, rect.center().y - 16.0, 120.0, 32.0),
    );
    ToggleGroup::new(["Left", "Center", "Right"])
        .with_active(1, true)
        .paint(
            paint,
            Rect::from_xywh(rect.x() + 150.0, rect.center().y - 16.0, 180.0, 32.0),
        );
}

pub fn calendar(paint: &mut PaintCtx, rect: Rect) {
    let cal = Calendar::new(2026, 10);
    let weekdays: Vec<String> = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let w = (rect.width() - 24.0).min(230.0);
    cal.paint(
        paint,
        Rect::from_xywh(rect.x() + 12.0, rect.y() + 4.0, w, rect.height() - 8.0),
        "October 2026",
        &weekdays,
    );
}

pub fn color_chooser(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let mut chooser = ColorChooser::new(ColorChooser::default_palette());
    chooser.set_columns(6);
    let w = (rect.width() - 24.0).min(240.0);
    chooser.paint(
        paint,
        Rect::from_xywh(
            rect.x() + 12.0,
            rect.y() + 8.0,
            w,
            chooser.height(w, &theme),
        ),
    );
}

pub fn emoji_chooser(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let mut emoji = EmojiChooser::new(EmojiChooser::default_palette());
    emoji.set_columns(9);
    emoji.set_selected(3);
    let w = (rect.width() - 24.0).min(320.0);
    emoji.paint(
        paint,
        Rect::from_xywh(rect.x() + 12.0, rect.y() + 6.0, w, emoji.height(w, &theme)),
    );
}

// ── Nawigacja ───────────────────────────────────────────────────────────────

pub fn view_switcher(paint: &mut PaintCtx, rect: Rect) {
    let mut view = ViewSwitcher::new(["List", "Grid"]);
    view.set_selected(1);
    view.paint(
        paint,
        Rect::from_xywh(rect.x() + 16.0, rect.y() + 16.0, 160.0, 32.0),
    );
    let mut theme = ViewSwitcher::new(["Light", "Dark", "Auto"]);
    theme.set_selected(1);
    theme.paint(
        paint,
        Rect::from_xywh(rect.x() + 16.0, rect.y() + 56.0, 230.0, 32.0),
    );
}

pub fn nav_list(paint: &mut PaintCtx, rect: Rect) {
    let mut nav = NavList::new(vec![
        NavEntry::Section("System".into()),
        NavEntry::Item("Network".into()),
        NavEntry::Item("Display".into()),
        NavEntry::Item("Sound".into()),
    ]);
    nav.set_selected(Some(1));
    nav.paint(
        paint,
        Rect::from_xywh(rect.x() + 12.0, rect.y() + 4.0, 200.0, rect.height() - 8.0),
    );
}

pub fn navigation_rail(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let mut rail = NavigationRail::new(["Home", "Files", "Media", "Settings"]);
    rail.set_selected(1);
    let w = rail.width(&theme);
    rail.paint(
        paint,
        Rect::from_xywh(rect.x() + 16.0, rect.y() + 4.0, w, rect.height() - 8.0),
    );
}

pub fn tab_bar(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let mut tabs = TabBar::new(["General", "Advanced", "About"]);
    tabs.set_selected(1);
    let h = tabs.height(&theme);
    tabs.paint(
        paint,
        Rect::from_xywh(
            rect.x() + 12.0,
            rect.center().y - h / 2.0,
            rect.width() - 24.0,
            h,
        ),
    );
}

pub fn tab_view(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let mut view = TabView::new();
    view.add_titled("General");
    view.add_titled("Advanced");
    view.add_titled("About");
    view.select(1);
    let bar = view.tab_bar();
    let h = bar.height(&theme);
    bar.paint(
        paint,
        Rect::from_xywh(rect.x() + 12.0, rect.y() + 4.0, rect.width() - 24.0, h),
    );
    let page = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + h + 10.0,
        rect.width() - 24.0,
        56.0,
    );
    paint
        .scene
        .fill_rounded_rect(page, theme.radius.small, theme.colors.shade);
    paint.label_center(
        page.center().x,
        page.center().y,
        "Advanced — page content",
        theme.typography.body.size,
        weight::NORMAL,
        theme.colors.text,
    );
}

pub fn path_bar(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let bar = PathBar::new(["Home", "Documents", "Projects"]);
    let h = bar.height(&theme);
    bar.paint(
        paint,
        Rect::from_xywh(
            rect.x() + 12.0,
            rect.center().y - h / 2.0,
            rect.width() - 24.0,
            h,
        ),
    );
}

pub fn pager(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let mut pager = libluna_widgets::Pager::new(3);
    pager.set_active(1);
    let pages = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 6.0,
        rect.width() - 24.0,
        rect.height() - 26.0,
    );
    paint.scene.push_clip(pages);
    for i in 0..3 {
        let page = pager.page_rect(pages, i);
        paint
            .scene
            .fill_rounded_rect(page, theme.radius.medium, theme.colors.shade);
        paint.label_center(
            page.center().x,
            page.center().y,
            &format!("Page {}", i + 1),
            theme.typography.body.size,
            weight::MEDIUM,
            theme.colors.text,
        );
    }
    paint.scene.pop_clip();
    pager.paint(paint, pages);
}

pub fn navigation_view(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let mut nav = NavigationView::new();
    nav.push();
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 6.0,
        rect.width() - 24.0,
        rect.height() - 12.0,
    );
    nav.paint(paint, area);
    let title = nav.content_rect(area, &theme);
    paint.label_center(
        title.center().x,
        title.center().y,
        "Page title",
        theme.typography.body.size,
        weight::MEDIUM,
        theme.colors.text,
    );
}

pub fn navigation_split(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let view = NavigationSplitView::new().sidebar_width(120.0);
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 6.0,
        rect.width() - 24.0,
        rect.height() - 12.0,
    );
    if let Some(side) = view.sidebar_rect(area) {
        Sidebar::new().paint(paint, side);
        let mut nav = NavList::new(vec![
            NavEntry::Item("Files".into()),
            NavEntry::Item("Recent".into()),
        ]);
        nav.set_selected(Some(0));
        nav.paint(
            paint,
            Rect::from_xywh(
                side.x() + 6.0,
                side.y() + 6.0,
                side.width() - 12.0,
                side.height() - 12.0,
            ),
        );
    }
    let content = view.content_rect(area);
    paint.scene.fill_rect(content, theme.colors.view_bg);
    paint.label_center(
        content.center().x,
        content.center().y,
        "Detail",
        theme.typography.body.size,
        weight::NORMAL,
        theme.colors.text,
    );
}

pub fn overlay_split(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let mut view = OverlaySplitView::new();
    view.set_open(true);
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 6.0,
        rect.width() - 24.0,
        rect.height() - 12.0,
    );
    paint.scene.fill_rect(area, theme.colors.view_bg);
    view.paint_scrim(paint, area);
    Sidebar::new().paint(paint, view.sidebar_rect(area));
}

pub fn sidebar(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 6.0,
        (rect.width() - 24.0).min(220.0),
        rect.height() - 12.0,
    );
    Sidebar::new().paint(paint, area);
    let mut nav = NavList::new(vec![
        NavEntry::Item("Files".into()),
        NavEntry::Item("Recent".into()),
    ]);
    nav.set_selected(Some(1));
    let inner = Sidebar::new().content_rect(area, &theme);
    nav.paint(paint, inner);
}

pub fn popover_menu_bar(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let bar = PopoverMenuBar::new(["File", "Edit", "View"]);
    let h = theme.metrics.button.height;
    bar.paint(
        paint,
        Rect::from_xywh(
            rect.x() + 12.0,
            rect.center().y - h / 2.0,
            rect.width() - 24.0,
            h,
        ),
    );
}

pub fn menu(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let menu = Menu::new(vec![
        MenuEntry::action("New Window").shortcut("Ctrl+N"),
        MenuEntry::separator(),
        MenuEntry::action("Settings"),
    ]);
    let w = (rect.width() - 24.0).min(230.0);
    let h = menu.height(&theme);
    menu.paint(
        paint,
        Rect::from_xywh(rect.x() + 12.0, rect.center().y - h / 2.0, w, h),
    );
}

// ── Układ ───────────────────────────────────────────────────────────────────

pub fn card(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 6.0,
        rect.width() - 24.0,
        rect.height() - 12.0,
    );
    let content = Card::new("Appearance").paint(paint, area);
    paint.label_left(
        content.x() + theme.spacing.md,
        content.center().y,
        "3 settings",
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
}

pub fn preferences_group(paint: &mut PaintCtx, rect: Rect) {
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 4.0,
        rect.width() - 24.0,
        rect.height() - 8.0,
    );
    let content = PreferencesGroup::new("Network").paint(paint, area);
    let row_h = 32.0;
    for (i, (title, subtitle)) in [("Wi-Fi", "Connected to Luna"), ("Bluetooth", "Off")]
        .into_iter()
        .enumerate()
    {
        let row = PreferencesGroup::row_rect(content, i, row_h);
        ActionRow::new(title)
            .subtitle(subtitle)
            .paint(paint, row, 40.0);
    }
}

pub fn preferences_page(paint: &mut PaintCtx, rect: Rect) {
    let page = PreferencesPage::new()
        .add(PreferencesGroup::new("General"), 2)
        .add(PreferencesGroup::new("Advanced"), 1);
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 2.0,
        rect.width() - 24.0,
        rect.height() - 4.0,
    );
    page.paint(paint, area, 28.0);
}

pub fn action_row(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let row = ActionRow::new("Wi-Fi").subtitle("Connected to Luna");
    let suffix = row.paint(
        paint,
        Rect::from_xywh(rect.x() + 12.0, rect.y() + 14.0, rect.width() - 24.0, 32.0),
        60.0,
    );
    Switch::new(true).paint(paint, suffix);
    ActionRow::new("Auto-connect")
        .subtitle("Join known networks")
        .paint(
            paint,
            Rect::from_xywh(rect.x() + 12.0, rect.y() + 54.0, rect.width() - 24.0, 32.0),
            60.0,
        );
    let _ = theme;
}

pub fn row(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    Row::switch("Notifications", true)
        .subtitle("Allow alerts")
        .paint(
            paint,
            Rect::from_xywh(rect.x() + 12.0, rect.y() + 12.0, rect.width() - 24.0, 36.0),
        );
    Row::combo("Theme", "Dark").paint(
        paint,
        Rect::from_xywh(rect.x() + 12.0, rect.y() + 58.0, rect.width() - 24.0, 32.0),
    );
    let _ = theme;
}

pub fn expander(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let exp = Expander::new("Advanced options").expanded(true);
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 6.0,
        rect.width() - 24.0,
        rect.height() - 12.0,
    );
    exp.paint(paint, area);
    let content = exp.content_rect(area, &theme);
    paint.label_left(
        content.x() + theme.spacing.md,
        content.y() + 14.0,
        "Extra settings…",
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
}

pub fn flow_box(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let mut flow = FlowBox::new(44.0);
    flow.set_count(10);
    flow.set_selected(Some(2));
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 8.0,
        rect.width() - 24.0,
        rect.height() - 16.0,
    );
    flow.paint(paint, area);
    for (i, cell) in flow.cells(area).iter().enumerate() {
        paint
            .scene
            .fill_rounded_rect(*cell, theme.radius.small, theme.colors.shade);
        paint.label_center(
            cell.center().x,
            cell.center().y,
            &(i + 1).to_string(),
            theme.typography.caption.size,
            weight::MEDIUM,
            theme.colors.text,
        );
    }
}

pub fn paned(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let mut paned = Paned::new(Orientation::Horizontal);
    paned.set_position(0.4);
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 8.0,
        rect.width() - 24.0,
        rect.height() - 16.0,
    );
    let (a, b) = paned.panes(area, &theme);
    paint.scene.fill_rect(a, theme.colors.shade);
    paint.scene.fill_rect(b, theme.colors.card_bg);
    paned.paint(paint, area);
}

pub fn stack(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let mut stack = Stack::with_count(2);
    stack.set_visible_index(1);
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 8.0,
        rect.width() - 24.0,
        rect.height() - 16.0,
    );
    let (_, incoming) = stack.frames(area);
    paint
        .scene
        .fill_rounded_rect(incoming.rect, theme.radius.small, theme.colors.shade);
    paint.label_center(
        incoming.rect.center().x,
        incoming.rect.center().y,
        "Page 2",
        theme.typography.body.size,
        weight::MEDIUM,
        theme.colors.text,
    );
}

pub fn layout_basics(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let grid = Grid::new(2, 3);
    let area = Rect::from_xywh(rect.x() + 12.0, rect.y() + 8.0, 150.0, rect.height() - 16.0);
    for cell in grid.cells(area, &theme) {
        paint
            .scene
            .fill_rounded_rect(cell, theme.radius.small, theme.colors.shade);
    }
    let center = Rect::from_xywh(
        rect.x() + 178.0,
        rect.y() + 8.0,
        rect.width() - 190.0,
        rect.height() - 16.0,
    );
    let [start, middle, end] = CenterBox::parts(center, 32.0, 70.0, 40.0);
    for r in [start, middle, end] {
        paint
            .scene
            .fill_rounded_rect(r, theme.radius.small, theme.colors.card_bg);
        paint
            .scene
            .stroke_rounded_rect(r, theme.radius.small, 1.0, theme.colors.border);
    }
    let bin = Bin::new().padding(Insets::splat(4.0));
    let inner = bin.content_rect(middle, Size::new(40.0, 16.0));
    paint.scene.fill_rounded_rect(
        inner,
        theme.radius.small,
        theme.colors.accent.scale_alpha(0.35),
    );
    let rule = Fixed::child(center, 0.0, center.height() - 8.0, center.width(), 1.0);
    paint.scene.fill_rect(rule, theme.colors.border);
}

pub fn layout_display(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 8.0,
        rect.width() - 24.0,
        rect.height() - 16.0,
    );
    let frame = Frame::new().label("Frame");
    let _inner = frame.paint(paint, area);
    let clamped = Clamp::new(120.0).content_rect(area);
    let framed = AspectFrame::new(16.0 / 9.0).content_rect(clamped);
    paint
        .scene
        .fill_rounded_rect(framed, theme.radius.small, theme.colors.shade);
    let mut revealer = Revealer::new();
    revealer.set_fraction(0.55);
    let visible = revealer.reveal_rect(framed);
    paint.scene.fill_rounded_rect(
        visible,
        theme.radius.small,
        theme.colors.accent.scale_alpha(0.35),
    );
    let badge = Overlay::new().child_rect(
        framed,
        Size::new(16.0, 16.0),
        BinAlign::End,
        BinAlign::Start,
        4.0,
    );
    paint
        .scene
        .fill_rounded_rect(badge, 4.0, theme.colors.accent);
}

pub fn layout_wrap(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let sizes = [
        Size::new(44.0, 20.0),
        Size::new(60.0, 20.0),
        Size::new(52.0, 20.0),
        Size::new(70.0, 20.0),
    ];
    let wrap = WrapBox::new().spacing(6.0).line_spacing(6.0);
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 10.0,
        rect.width() - 24.0,
        rect.height() - 20.0,
    );
    for (i, r) in wrap.layout(area, &sizes).iter().enumerate() {
        paint
            .scene
            .fill_rounded_rect(*r, theme.radius.small, theme.colors.shade);
        paint.label_center(
            r.center().x,
            r.center().y,
            &(i + 1).to_string(),
            theme.typography.caption.size,
            weight::MEDIUM,
            theme.colors.text,
        );
    }
    let bp = BreakpointBin::new(vec![600.0, 900.0]);
    let active = bp.active(rect.width());
    paint.label_right(
        rect.right() - 12.0,
        rect.bottom() - 10.0,
        &format!("breakpoint {active}"),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    let _ = ButtonContent::with_icon().layout(area, &theme, paint.text, "Open");
}

pub fn layout_window(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 6.0,
        rect.width() - 24.0,
        rect.height() - 12.0,
    );
    let tv = ToolbarView::new().top(24.0).bottom(18.0);
    let parts = tv.rects(area);
    if let Some(top) = parts.top {
        paint.scene.fill_rect(top, theme.colors.headerbar_bg);
    }
    if let Some(bottom) = parts.bottom {
        paint.scene.fill_rect(bottom, theme.colors.window_bg);
    }
    paint.scene.fill_rect(parts.content, theme.colors.view_bg);
    let bar_h = ActionBar::height(&theme);
    let bar = Rect::from_xywh(area.x(), area.bottom() - bar_h, area.width(), bar_h);
    ActionBar.paint(paint, bar);
    let [start, end] = ActionBar::parts(bar, &theme, 64.0, 54.0);
    Button::new("Cancel").paint(paint, start);
    Button::new("Save")
        .variant(ButtonVariant::Suggested)
        .paint(paint, end);
    let _ = Viewport::content_rect(area, Size::new(area.width(), 400.0), Point::new(0.0, 40.0));
    let _ = WindowHandle::is_drag_region(area, &[], Point::new(area.x(), area.y()));
}

// ── Kolekcje ────────────────────────────────────────────────────────────────

pub fn item_view(paint: &mut PaintCtx, rect: Rect) {
    let mut view = ItemView::new(vec![
        Item::folder("Projects"),
        Item::new("Report").detail("PDF"),
        Item::new("Notes").detail("TXT"),
        Item::new("Budget").detail("XLSX"),
    ]);
    view.set_mode(ViewMode::List);
    view.set_selected(Some(1));
    view.paint(
        paint,
        Rect::from_xywh(
            rect.x() + 12.0,
            rect.y() + 6.0,
            rect.width() - 24.0,
            rect.height() - 12.0,
        ),
    );
}

pub fn column_view(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let mut table = ColumnView::new(vec![Column::new("Name").weight(2.0), Column::new("Size")]);
    table.set_rows(4);
    table.set_selected(Some(1));
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 4.0,
        rect.width() - 24.0,
        rect.height() - 8.0,
    );
    table.paint(paint, area);
    for (i, name) in ["Projects", "Report.pdf", "Notes.txt", "Budget.xlsx"]
        .iter()
        .enumerate()
    {
        let cell = table.cell_rect(area, &theme, i, 0);
        paint.label_left(
            cell.x() + theme.spacing.md,
            cell.center().y,
            name,
            theme.typography.body.size,
            weight::NORMAL,
            theme.colors.text,
        );
    }
}

pub fn scroll_view(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 6.0,
        rect.width() - 24.0,
        rect.height() - 12.0,
    );
    let mut view = ScrollView::new();
    view.set_content_height(400.0);
    view.set_offset(48.0, area);
    paint.scene.push_clip(area);
    for i in 0..8 {
        let y = area.y() + i as f64 * 24.0 - view.offset();
        let row = Rect::from_xywh(area.x(), y, area.width(), 22.0);
        paint
            .scene
            .fill_rounded_rect(row, theme.radius.small, theme.colors.shade);
        paint.label_left(
            row.x() + theme.spacing.md,
            row.center().y,
            &format!("Row {}", i + 1),
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.text,
        );
    }
    paint.scene.pop_clip();
    view.paint_scrollbar(paint, area);
}

pub fn scrollbar(paint: &mut PaintCtx, rect: Rect) {
    let mut adj = libluna_widgets::Adjustment::new(0.0, 200.0, 60.0);
    adj.set_fraction(0.35);
    let mut bar = Scrollbar::new(Orientation::Vertical);
    bar.set_adjustment(adj);
    bar.paint(
        paint,
        Rect::from_xywh(
            rect.center().x - 60.0,
            rect.y() + 6.0,
            420.0,
            rect.height() - 12.0,
        ),
    );
    let mut adj = libluna_widgets::Adjustment::new(0.0, 200.0, 60.0);
    adj.set_fraction(0.6);
    let mut bar = Scrollbar::new(Orientation::Horizontal);
    bar.set_adjustment(adj);
    bar.paint(
        paint,
        Rect::from_xywh(
            rect.x() + 12.0,
            rect.center().y - 30.0,
            rect.width() - 24.0,
            20.0,
        ),
    );
}

// ── Media ───────────────────────────────────────────────────────────────────

pub fn media_controls(paint: &mut PaintCtx, rect: Rect) {
    let mut media = MediaControls::new("Aurora", "Luna Ensemble");
    media.set_times("1:42", "4:07");
    media.set_position(0.41);
    media.paint(
        paint,
        Rect::from_xywh(
            rect.x() + 12.0,
            rect.center().y - 26.0,
            rect.width() - 24.0,
            52.0,
        ),
    );
}

pub fn picture(paint: &mut PaintCtx, rect: Rect) {
    let w = (rect.width() - 34.0) / 2.0;
    let h = rect.height() - 20.0;
    for (i, fit) in [ContentFit::Contain, ContentFit::Cover]
        .into_iter()
        .enumerate()
    {
        let area = Rect::from_xywh(
            rect.x() + 12.0 + i as f64 * (w + 10.0),
            rect.y() + 10.0,
            w,
            h,
        );
        let mut pic = Picture::with_image(gradient());
        pic.set_fit(fit);
        pic.paint(paint, area);
        paint
            .scene
            .stroke_rounded_rect(area, 0.0, 1.0, paint.theme.colors.border);
    }
}

// ── Informacje ──────────────────────────────────────────────────────────────

pub fn info_bar(paint: &mut PaintCtx, rect: Rect) {
    InfoBar::new("Nowa wersja jest dostępna")
        .kind(InfoBarKind::Info)
        .action("OK")
        .paint(
            paint,
            Rect::from_xywh(rect.x() + 12.0, rect.y() + 10.0, rect.width() - 24.0, 40.0),
        );
    InfoBar::new("Mało miejsca na dysku")
        .kind(InfoBarKind::Warning)
        .action("Zarządzaj")
        .paint(
            paint,
            Rect::from_xywh(rect.x() + 12.0, rect.y() + 58.0, rect.width() - 24.0, 40.0),
        );
}

pub fn banner(paint: &mut PaintCtx, rect: Rect) {
    Banner::new("Nowa wersja libLuny")
        .kind(BannerKind::Info)
        .description("Wszystkie widgety są już dostępne.")
        .action("Zainstaluj")
        .paint(
            paint,
            Rect::from_xywh(rect.x() + 12.0, rect.y() + 14.0, rect.width() - 24.0, 56.0),
        );
}

pub fn toast(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let w = (rect.width() - 60.0).min(280.0);
    let h = theme.metrics.toast.height;
    let toast = Rect::from_xywh(
        rect.center().x - w / 2.0,
        rect.center().y - h / 2.0 + 8.0,
        w,
        h,
    );
    paint
        .scene
        .fill_rounded_rect(toast, theme.radius.medium, theme.colors.text);
    paint.label_left(
        toast.x() + theme.metrics.toast.pad_x,
        toast.center().y,
        "Accent changed",
        theme.typography.body.size,
        weight::NORMAL,
        theme.colors.view_bg,
    );
    paint.label_right(
        toast.right() - theme.metrics.toast.pad_x,
        toast.center().y,
        "Undo",
        theme.typography.body.size,
        weight::BOLD,
        theme.colors.accent,
    );
}

pub fn status_page(paint: &mut PaintCtx, rect: Rect) {
    let page = StatusPage::new("No results")
        .icon(StatusIcon::Search)
        .action("Clear");
    page.paint(
        paint,
        Rect::from_xywh(
            rect.x() + 12.0,
            rect.y() + 4.0,
            rect.width() - 24.0,
            rect.height() - 8.0,
        ),
    );
}

pub fn statusbar(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let mut status = Statusbar::new();
    status.set_message("Ready");
    status.set_right("8 items");
    let h = status.height(&theme);
    status.paint(
        paint,
        Rect::from_xywh(
            rect.x() + 12.0,
            rect.bottom() - h - 10.0,
            rect.width() - 24.0,
            h,
        ),
    );
}

pub fn popover(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let popover = Popover::new(PopoverPlacement::Above);
    let panel = Rect::from_xywh(rect.center().x - 90.0, rect.center().y - 22.0, 180.0, 44.0);
    popover.paint(paint, panel);
    paint.label_center(
        panel.center().x,
        panel.center().y,
        "Popover content",
        theme.typography.body.size,
        weight::NORMAL,
        theme.colors.text,
    );
}

// ── Okna i dialogi ──────────────────────────────────────────────────────────

pub fn header_bar(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let bar = HeaderBar::new()
        .title("Files")
        .controls(ControlsSide::Right);
    let h = bar.height(&theme);
    bar.paint(
        paint,
        Rect::from_xywh(rect.x() + 12.0, rect.y() + 16.0, rect.width() - 24.0, h),
        None,
    );
}

pub fn window_title(paint: &mut PaintCtx, rect: Rect) {
    let title = WindowTitle::new("Documents").subtitle("12 items");
    title.paint(
        paint,
        Rect::from_xywh(
            rect.x() + 12.0,
            rect.center().y - 20.0,
            rect.width() - 24.0,
            40.0,
        ),
    );
}

fn mini_dialog(paint: &mut PaintCtx, rect: Rect, w: f64, h: f64, title: &str) -> Rect {
    let theme = *paint.theme;
    paint
        .scene
        .fill_rect(rect, Color::rgba(0.0, 0.0, 0.0, 0.18));
    let panel = Rect::from_xywh(rect.center().x - w / 2.0, rect.center().y - h / 2.0, w, h);
    paint
        .scene
        .fill_rounded_rect(panel, theme.radius.large, theme.colors.card_bg);
    paint
        .scene
        .stroke_rounded_rect(panel, theme.radius.large, 1.0, theme.colors.border);
    paint.label_center(
        panel.center().x,
        panel.y() + 18.0,
        title,
        theme.typography.body.size,
        weight::BOLD,
        theme.colors.text,
    );
    panel
}

pub fn dialog(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let panel = mini_dialog(paint, rect, 240.0, 86.0, "Preferences");
    paint.label_left(
        panel.x() + theme.spacing.md,
        panel.y() + 44.0,
        "Dialog content",
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    let close = Rect::from_xywh(panel.right() - 22.0, panel.y() + 10.0, 12.0, 12.0);
    paint
        .scene
        .stroke_rounded_rect(close, 2.0, 1.0, theme.colors.dim_text);
}

pub fn alert_dialog(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let panel = mini_dialog(paint, rect, 260.0, 92.0, "Delete file?");
    paint.label_left(
        panel.x() + theme.spacing.md,
        panel.y() + 42.0,
        "This cannot be undone.",
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    let h = 24.0;
    let y = panel.bottom() - h - 10.0;
    Button::new("Cancel").paint(paint, Rect::from_xywh(panel.right() - 156.0, y, 70.0, h));
    Button::new("Delete")
        .variant(ButtonVariant::Destructive)
        .paint(paint, Rect::from_xywh(panel.right() - 80.0, y, 68.0, h));
}

pub fn about_dialog(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let panel = mini_dialog(paint, rect, 250.0, 96.0, "Luna Files");
    paint.label_center(
        panel.center().x,
        panel.y() + 42.0,
        "0.0.1",
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    paint.label_center(
        panel.center().x,
        panel.y() + 62.0,
        "libLuna contributors",
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
}

pub fn shortcuts_dialog(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let panel = mini_dialog(paint, rect, 260.0, 96.0, "Keyboard Shortcuts");
    for (i, (label, keys)) in [("New window", ["Ctrl", "N"]), ("Search", ["Ctrl", "F"])]
        .into_iter()
        .enumerate()
    {
        let y = panel.y() + 42.0 + i as f64 * 24.0;
        paint.label_left(
            panel.x() + theme.spacing.md,
            y,
            label,
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.text,
        );
        ShortcutLabel::new(keys).paint(
            paint,
            Rect::from_xywh(panel.right() - 108.0, y - 9.0, 96.0, 18.0),
        );
    }
}

pub fn preferences_dialog(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    let panel = mini_dialog(paint, rect, 280.0, 96.0, "Preferences");
    let side = Rect::from_xywh(panel.x(), panel.y() + 30.0, 86.0, panel.height() - 30.0);
    paint.scene.fill_rect(side, theme.colors.sidebar_bg);
    paint.label_left(
        side.x() + theme.spacing.sm,
        side.y() + 14.0,
        "General",
        theme.typography.caption.size,
        weight::MEDIUM,
        theme.colors.text,
    );
    paint.label_left(
        side.x() + theme.spacing.sm,
        side.y() + 36.0,
        "Advanced",
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    paint.label_left(
        panel.x() + 100.0,
        panel.y() + 50.0,
        "Page content",
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
}

pub fn bottom_sheet(paint: &mut PaintCtx, rect: Rect) {
    let theme = *paint.theme;
    paint
        .scene
        .fill_rect(rect, Color::rgba(0.0, 0.0, 0.0, 0.18));
    let sheet = Rect::from_xywh(
        rect.x() + 12.0,
        rect.bottom() - 76.0,
        rect.width() - 24.0,
        70.0,
    );
    paint
        .scene
        .fill_rounded_rect(sheet, theme.radius.large, theme.colors.card_bg);
    let grabber = Rect::from_xywh(sheet.center().x - 16.0, sheet.y() + 6.0, 32.0, 4.0);
    paint
        .scene
        .fill_rounded_rect(grabber, 2.0, theme.colors.dim_text);
    paint.label_left(
        sheet.x() + theme.spacing.md,
        sheet.y() + 26.0,
        "About",
        theme.typography.body.size,
        weight::BOLD,
        theme.colors.text,
    );
    paint.label_left(
        sheet.x() + theme.spacing.md,
        sheet.y() + 48.0,
        "Sheet content",
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
}

pub fn assistant(paint: &mut PaintCtx, rect: Rect) {
    let wizard = Assistant::new(["Welcome", "Details", "Done"]);
    wizard.paint(
        paint,
        Rect::from_xywh(
            rect.x() + 12.0,
            rect.y() + 4.0,
            rect.width() - 24.0,
            rect.height() - 8.0,
        ),
        "Cancel",
        "Next",
    );
}
