//! Rysowanie i obsługa wejścia Luna Installer.
//!
//! Jeden przebieg [`layout`] liczy wszystkie prostokąty (także po przewinięciu),
//! a rysowanie i trafienia korzystają z tych samych danych.

use libluna_core::style::weight;
use libluna_core::{
    ButtonState, Color, Event, Insets, KeyEvent, Point, PointerButton, Rect, Size, Theme,
};
use libluna_i18n::{tr, tr_args};
use libluna_render::{Scene, TextEngine};
use libluna_widgets::{
    paint_icon, Assistant, Banner, BannerKind, Button, ButtonVariant, ControlsSide, Entry,
    HeaderBar, IconName, PaintCtx, Popover, PopoverPlacement, ProgressBar, Spinner, WindowControl,
};

use crate::app::{App, Hit, PickerKind, Step};
use crate::checks::Level;
use crate::disk::format_size;
use crate::keyboard::XkbVariant;
use crate::plan::{Field, PartitionMode, RootMode};

const PAD: f64 = 28.0;
const SCROLL_STEP: f64 = 64.0;
const SCROLL_UNIT: f64 = 20.0;
const ROW_H: f64 = 30.0;
const ENTRY_H: f64 = 62.0;

/// Geometria bieżącego kroku.
#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub header: Rect,
    pub area: Rect,
    pub viewport: Rect,
    pub content_height: f64,
    pub offset: f64,
    pub entries: Vec<(Field, Rect)>,
    pub rows: Vec<(Hit, Rect)>,
    pub labels: Vec<(String, Rect)>,
    pub notes: Vec<(String, Rect)>,
    pub summary_rows: Vec<(String, String, Rect)>,
    pub check_rows: Vec<(crate::checks::Check, Rect)>,
    pub next: Option<Rect>,
    pub cancel: Option<Rect>,
    pub footer: Option<Rect>,
    pub log_toggle: Option<Rect>,
    pub save_log: Option<Rect>,
    pub retry_network: Option<Rect>,
    pub refresh_checks: Option<Rect>,
    pub picker: Option<PickerLayout>,
}

/// Układ otwartego popupu wyboru.
#[derive(Debug, Clone, Default)]
pub struct PickerLayout {
    pub panel: Rect,
    pub title: Rect,
    pub close: Rect,
    pub search: Option<Rect>,
    pub list: Rect,
    pub rows: Vec<(usize, Rect)>,
    pub content_height: f64,
    pub offset: f64,
}

impl Layout {
    fn shift(&mut self, offset: f64) {
        if offset == 0.0 {
            return;
        }
        let shift =
            |rect: Rect| Rect::from_xywh(rect.x(), rect.y() - offset, rect.width(), rect.height());
        for (_, rect) in &mut self.entries {
            *rect = shift(*rect);
        }
        for (_, rect) in &mut self.rows {
            *rect = shift(*rect);
        }
        for (_, rect) in &mut self.labels {
            *rect = shift(*rect);
        }
        for (_, rect) in &mut self.notes {
            *rect = shift(*rect);
        }
        for (_, _, rect) in &mut self.summary_rows {
            *rect = shift(*rect);
        }
        for (_, rect) in &mut self.check_rows {
            *rect = shift(*rect);
        }
    }

    pub fn row(&self, hit: Hit) -> Option<Rect> {
        self.rows
            .iter()
            .find(|(candidate, _)| *candidate == hit)
            .map(|(_, rect)| *rect)
    }
}

struct Stack {
    x: f64,
    y: f64,
    width: f64,
}

impl Stack {
    fn new(viewport: Rect) -> Self {
        Self {
            x: viewport.x(),
            y: viewport.y(),
            width: viewport.width(),
        }
    }

    fn rect(&mut self, height: f64) -> Rect {
        let rect = Rect::from_xywh(self.x, self.y, self.width, height);
        self.y += height;
        rect
    }

    fn space(&mut self, height: f64) {
        self.y += height;
    }

    fn label(&mut self, layout: &mut Layout, text: impl Into<String>) {
        self.space(10.0);
        let rect = self.rect(20.0);
        layout.labels.push((text.into(), rect));
        self.space(6.0);
    }

    fn note(&mut self, layout: &mut Layout, text: impl Into<String>) {
        let rect = self.rect(18.0);
        layout.notes.push((text.into(), rect));
        self.space(4.0);
    }

    fn entry(&mut self, layout: &mut Layout, field: Field) {
        let rect = self.rect(ENTRY_H);
        layout.entries.push((field, rect));
        self.space(4.0);
    }

    fn row(&mut self, layout: &mut Layout, hit: Hit) -> Rect {
        let rect = self.rect(ROW_H);
        layout.rows.push((hit, rect));
        self.space(2.0);
        rect
    }

    fn select_row(&mut self, layout: &mut Layout, hit: Hit) -> Rect {
        let rect = self.rect(38.0);
        layout.rows.push((hit, rect));
        self.space(4.0);
        rect
    }

    fn row_width(&mut self, layout: &mut Layout, hit: Hit, width: f64) -> Rect {
        let rect = Rect::from_xywh(self.x, self.y, width.min(self.width), ROW_H);
        self.y += ROW_H + 2.0;
        layout.rows.push((hit, rect));
        rect
    }
}

fn assistant_labels(step: Step) -> (String, String) {
    let cancel = if step == Step::Welcome {
        tr("Cancel")
    } else {
        tr("Back")
    };
    let next = if step == Step::Summary {
        tr("Install")
    } else {
        tr("Continue")
    };
    (cancel, next)
}

fn step_description(step: Step) -> String {
    match step {
        Step::Welcome => String::new(),
        Step::Language => tr("Choose the language of the installed system and your keyboard layout."),
        Step::Region => tr("Set the timezone. It is detected automatically and can be changed."),
        Step::Network => tr("An internet connection is optional, but required for system updates during installation."),
        Step::Requirements => tr("The installer checks the live image, firmware, memory, power and required tools."),
        Step::Disk => tr("Choose a disk, then how Luna should use it. Nothing is written until you confirm the summary."),
        Step::Account => tr("Create the account you will use to log in. It can administer the computer."),
        Step::Summary => tr("Review every choice before the installer starts."),
        Step::Progress => tr("Do not turn off the computer. This can take several minutes."),
        Step::Done => String::new(),
    }
}

fn wizard_assistant(app: &App) -> Assistant {
    let mut assistant = Assistant::new(Step::WIZARD.iter().map(|step| step.title()));
    assistant.set_current(app.step.index());
    assistant
}

/// Oblicza układ.
pub fn layout(app: &App, size: Size, theme: &Theme, text: &TextEngine) -> Layout {
    let header = Rect::from_xywh(
        0.0,
        0.0,
        size.width,
        theme.metrics.headerbar.height.min(size.height),
    );
    let area = Rect::from_xywh(
        0.0,
        header.bottom(),
        size.width,
        (size.height - header.bottom()).max(0.0),
    );
    let mut layout = Layout {
        header,
        area,
        viewport: area,
        ..Layout::default()
    };

    if app.step.is_wizard() {
        let assistant = wizard_assistant(app);
        let content = assistant
            .content_rect(area, theme)
            .deflate(Insets::symmetric(theme.spacing.sm, PAD));
        layout.viewport = Rect::from_xywh(
            content.x(),
            content.y() + 52.0,
            content.width(),
            (content.height() - 52.0).max(0.0),
        );
        let (cancel_label, next_label) = assistant_labels(app.step);
        let [cancel, next] = assistant.buttons(area, text, theme, &cancel_label, &next_label);
        layout.cancel = Some(cancel);
        layout.next = Some(next);
        layout.footer = Some(assistant.footer_rect(area, theme));
    } else {
        layout.viewport = area.deflate(Insets::symmetric(theme.spacing.lg, PAD));
    }

    let mut stack = Stack::new(layout.viewport);
    match app.step {
        Step::Welcome => layout_welcome(app, &mut stack, &mut layout),
        Step::Language => layout_language(app, &mut stack, &mut layout),
        Step::Region => layout_region(app, &mut stack, &mut layout),
        Step::Network => layout_network(app, &mut stack, &mut layout),
        Step::Requirements => layout_requirements(app, &mut stack, &mut layout),
        Step::Disk => layout_disk(app, &mut stack, &mut layout),
        Step::Account => layout_account(app, &mut stack, &mut layout),
        Step::Summary => layout_summary(app, &mut stack, &mut layout),
        Step::Progress => layout_progress(app, &mut stack, &mut layout, theme),
        Step::Done => layout_done(app, &mut stack, &mut layout),
    }

    layout.content_height = (stack.y - layout.viewport.y()).max(0.0);
    let max_offset = (layout.content_height - layout.viewport.height()).max(0.0);
    layout.offset = app.scroll.offset().min(max_offset);
    layout.shift(layout.offset);
    layout.picker = layout_picker(app, layout.area);
    layout
}

fn layout_picker(app: &App, area: Rect) -> Option<PickerLayout> {
    let picker = app.picker.as_ref()?;
    let width = (area.width() - 80.0).clamp(300.0, 560.0);
    let height = (area.height() - 80.0).clamp(260.0, 560.0);
    let panel = Rect::from_xywh(
        area.center().x - width / 2.0,
        area.center().y - height / 2.0,
        width,
        height,
    );
    let title = Rect::from_xywh(
        panel.x() + 16.0,
        panel.y() + 10.0,
        panel.width() - 60.0,
        28.0,
    );
    let close = Rect::from_xywh(panel.right() - 40.0, panel.y() + 10.0, 28.0, 28.0);
    let search = picker.kind.searchable().then(|| {
        Rect::from_xywh(
            panel.x() + 14.0,
            panel.y() + 48.0,
            panel.width() - 28.0,
            38.0,
        )
    });
    let list_top = search
        .map(|rect| rect.bottom() + 8.0)
        .unwrap_or(panel.y() + 48.0);
    let list = Rect::from_xywh(
        panel.x() + 8.0,
        list_top,
        panel.width() - 16.0,
        (panel.bottom() - 8.0 - list_top).max(0.0),
    );
    let items = app.picker_items();
    let content_height = items.len() as f64 * 36.0;
    let max_offset = (content_height - list.height()).max(0.0);
    let offset = picker.scroll.offset().min(max_offset);
    let rows = items
        .iter()
        .enumerate()
        .map(|(index, _)| {
            (
                index,
                Rect::from_xywh(
                    list.x(),
                    list.y() + index as f64 * 36.0 - offset,
                    list.width(),
                    34.0,
                ),
            )
        })
        .collect();
    Some(PickerLayout {
        panel,
        title,
        close,
        search,
        list,
        rows,
        content_height,
        offset,
    })
}

fn layout_welcome(_app: &App, stack: &mut Stack, layout: &mut Layout) {
    stack.space(76.0);
    stack.note(
        layout,
        tr("This guided installer copies the running Luna system to a disk."),
    );
    stack.space(8.0);
    stack.note(
        layout,
        tr("You can go back at every step. The disk is erased only after you confirm the summary."),
    );
    stack.space(8.0);
    stack.note(
        layout,
        tr("On a live image your current session is not changed."),
    );
}

fn layout_language(_app: &App, stack: &mut Stack, layout: &mut Layout) {
    stack.label(layout, tr("Language of the installed system"));
    stack.select_row(layout, Hit::OpenPicker(PickerKind::Language));
    stack.space(8.0);
    stack.label(layout, tr("Keyboard layout"));
    stack.select_row(layout, Hit::OpenPicker(PickerKind::Layout));
    stack.label(layout, tr("Layout variant"));
    stack.select_row(layout, Hit::OpenPicker(PickerKind::Variant));
    stack.label(layout, tr("Keyboard model"));
    stack.select_row(layout, Hit::OpenPicker(PickerKind::Model));
}

fn layout_region(_app: &App, stack: &mut Stack, layout: &mut Layout) {
    stack.label(layout, tr("Timezone"));
    stack.select_row(layout, Hit::OpenPicker(PickerKind::Timezone));
}

fn layout_network(app: &App, stack: &mut Stack, layout: &mut Layout) {
    stack.space(62.0);
    let rect = stack.row_width(layout, Hit::RetryNetwork, 160.0);
    layout.retry_network = Some(rect);
    if app.network.wifi.is_empty() {
        stack.note(layout, tr("No Wi-Fi networks found."));
    } else {
        for (index, _) in app.network.wifi.iter().enumerate() {
            stack.row(layout, Hit::Wifi(index));
        }
    }
    if app
        .selected_wifi
        .and_then(|index| app.network.wifi.get(index))
        .is_some_and(|wifi| !wifi.open())
    {
        stack.entry(layout, Field::WifiPassword);
        stack.row(layout, Hit::ConnectWifi);
    }
    if let Some(message) = &app.wifi_message {
        stack.note(layout, message.clone());
    }
}

fn layout_requirements(app: &App, stack: &mut Stack, layout: &mut Layout) {
    layout.refresh_checks = Some(stack.row_width(layout, Hit::RefreshChecks, 140.0));
    stack.space(4.0);
    for check in &app.checks {
        let rect = stack.rect(44.0);
        layout.check_rows.push((check.clone(), rect));
        stack.space(4.0);
    }
}

fn layout_disk(app: &App, stack: &mut Stack, layout: &mut Layout) {
    stack.label(layout, tr("Disk"));
    stack.select_row(layout, Hit::OpenPicker(PickerKind::Disk));
    stack.space(8.0);
    stack.label(layout, tr("How should Luna use this disk?"));
    stack.select_row(layout, Hit::OpenPicker(PickerKind::Mode));
    stack.note(layout, app.options.mode.description());
    if app.options.mode == PartitionMode::FreeSpace {
        if let Some(disk) = app.selected_disk() {
            stack.note(
                layout,
                tr_args(
                    "Unallocated space: {free}",
                    &[("free", &format_size(disk.free_bytes))],
                ),
            );
        }
    }

    if app.options.mode == PartitionMode::Manual {
        stack.space(6.0);
        stack.label(layout, tr("Root partition"));
        stack.select_row(layout, Hit::OpenPicker(PickerKind::RootPartition));
        if app.options.uefi {
            stack.label(layout, tr("EFI system partition"));
            stack.select_row(layout, Hit::OpenPicker(PickerKind::EfiPartition));
        }
        stack.label(layout, tr("Swap partition (optional)"));
        stack.select_row(layout, Hit::OpenPicker(PickerKind::SwapPartition));
    }

    stack.space(6.0);
    stack.label(layout, tr("Filesystem"));
    stack.select_row(layout, Hit::OpenPicker(PickerKind::Filesystem));
    stack.label(layout, tr("Swap file"));
    stack.select_row(layout, Hit::OpenPicker(PickerKind::SwapSize));
    stack.label(layout, tr("Boot loader"));
    stack.select_row(layout, Hit::OpenPicker(PickerKind::Bootloader));
    stack.space(8.0);
    stack.row(layout, Hit::EncryptionToggle);
    if app.options.encrypt {
        stack.entry(layout, Field::LuksPassword);
        stack.entry(layout, Field::LuksConfirm);
    }
    stack.row(layout, Hit::UpdateToggle);
    stack.note(
        layout,
        if app.network.internet {
            tr("Updates are downloaded during installation (the network is connected).")
        } else {
            tr("Updates require an internet connection.")
        },
    );
}

fn layout_account(app: &App, stack: &mut Stack, layout: &mut Layout) {
    stack.space(14.0);
    for field in App::ACCOUNT_FIELDS {
        stack.entry(layout, field);
    }
    stack.space(4.0);
    stack.row(layout, Hit::AutologinToggle);
    stack.label(layout, tr("Root account"));
    stack.select_row(layout, Hit::OpenPicker(PickerKind::RootMode));
    if app.root_mode == RootMode::Custom {
        stack.space(24.0);
        stack.entry(layout, Field::RootPassword);
        stack.entry(layout, Field::RootConfirm);
    }
    stack.note(
        layout,
        tr_args(
            "Language: {locale} · Timezone: {timezone}",
            &[("locale", &app.locale), ("timezone", &app.timezone)],
        ),
    );
}

fn layout_summary(app: &App, stack: &mut Stack, layout: &mut Layout) {
    let rows = app.plan().map(|plan| plan.summary()).unwrap_or_default();
    for (label, value) in rows {
        let rect = stack.rect(34.0);
        layout.summary_rows.push((label, value, rect));
        stack.space(2.0);
    }
    stack.space(8.0);
    stack.row(layout, Hit::ConfirmErase);
    stack.space(6.0);
    stack.note(layout, app.summary_note());
    if app.options.encrypt {
        stack.note(
            layout,
            tr("There is no recovery key: losing the passphrase means losing the data."),
        );
    }
    if app.options.update_system {
        stack.note(
            layout,
            tr("The system will be updated during installation."),
        );
    }
}

fn layout_progress(app: &App, stack: &mut Stack, layout: &mut Layout, theme: &Theme) {
    let top = layout.viewport.y();
    layout.save_log = Some(Rect::from_xywh(
        layout.viewport.right() - 120.0,
        top,
        120.0,
        30.0,
    ));
    layout.log_toggle = Some(Rect::from_xywh(
        layout.viewport.right() - 250.0,
        top,
        120.0,
        30.0,
    ));
    stack.space(96.0);
    let _ = (app, theme);
}

fn layout_done(app: &App, stack: &mut Stack, layout: &mut Layout) {
    let y = layout.area.bottom() - 52.0;
    let width = 150.0;
    let gap = 12.0;
    let close = Rect::from_xywh(layout.area.right() - width, y, width, 38.0);
    layout.rows.push((Hit::Close, close));
    let save = Rect::from_xywh(layout.area.right() - 2.0 * width - gap, y, width, 38.0);
    layout.save_log = Some(save);
    if matches!(app.finished, Some(Ok(()))) {
        let restart = Rect::from_xywh(
            layout.area.right() - 3.0 * width - 2.0 * gap,
            y,
            width,
            38.0,
        );
        let power = Rect::from_xywh(
            layout.area.right() - 4.0 * width - 3.0 * gap,
            y,
            width,
            38.0,
        );
        layout.rows.push((Hit::Restart, restart));
        layout.rows.push((Hit::PowerOff, power));
    } else {
        let retry = Rect::from_xywh(
            layout.area.right() - 3.0 * width - 2.0 * gap,
            y,
            width,
            38.0,
        );
        layout.rows.push((Hit::Retry, retry));
    }
    let _ = stack;
}

/// Rysuje okno.
pub fn paint(app: &App, scene: &mut Scene, size: Size, theme: &Theme, text: &TextEngine) {
    let lay = layout(app, size, theme, text);
    let mut ctx = PaintCtx::new(scene, theme, text);
    ctx.scene
        .fill_rect(Rect::from_size(size), theme.colors.window_bg);

    let header = HeaderBar::new()
        .title(tr("Install Luna"))
        .controls(ControlsSide::Right);
    let hover_control = app
        .pointer
        .and_then(|point| header.control_at(lay.header, theme, point));
    header.paint(&mut ctx, lay.header, hover_control);

    if app.step.is_wizard() {
        let assistant = wizard_assistant(app);
        let (cancel_label, next_label) = assistant_labels(app.step);
        assistant.paint(&mut ctx, lay.area, &cancel_label, &next_label);
    }

    paint_step_title(app, &mut ctx, &lay);
    ctx.scene.push_clip(lay.viewport);
    paint_labels(&mut ctx, &lay);
    match app.step {
        Step::Welcome => paint_welcome(app, &mut ctx, &lay),
        Step::Language => paint_language(app, &mut ctx, &lay),
        Step::Region => paint_region(app, &mut ctx, &lay),
        Step::Network => paint_network(app, &mut ctx, &lay),
        Step::Requirements => paint_requirements(app, &mut ctx, &lay),
        Step::Disk => paint_disk(app, &mut ctx, &lay),
        Step::Account => paint_account(app, &mut ctx, &lay),
        Step::Summary => paint_summary(app, &mut ctx, &lay),
        Step::Progress => paint_progress(app, &mut ctx, &lay),
        Step::Done => paint_done(app, &mut ctx, &lay),
    }
    ctx.scene.pop_clip();

    paint_fixed_controls(app, &mut ctx, &lay);
    let mut scroll = app.scroll;
    scroll.set_content_height(lay.content_height);
    scroll.set_offset(lay.offset, lay.viewport);
    scroll.paint_scrollbar(&mut ctx, lay.viewport);

    if let Some(message) = &app.banner {
        paint_banner(&mut ctx, message, lay.area);
    }
    if lay.picker.is_some() {
        paint_picker(app, &mut ctx, &lay);
    }
}

fn paint_picker(app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    let Some(picker_layout) = &lay.picker else {
        return;
    };
    let Some(picker) = &app.picker else {
        return;
    };
    let theme = *ctx.theme;
    let colors = theme.colors;
    ctx.scene
        .fill_rect(lay.area, Color::rgba(0.0, 0.0, 0.0, 0.35));
    Popover::new(PopoverPlacement::Below).paint(ctx, picker_layout.panel);
    ctx.scene.fill_rect(picker_layout.title, colors.window_bg);
    ctx.label_left(
        picker_layout.title.x(),
        picker_layout.title.center().y,
        &picker.kind.title(),
        theme.typography.body.size,
        weight::BOLD,
        colors.text,
    );
    if app.hover == Some(Hit::PickerClose) {
        ctx.scene
            .fill_rounded_rect(picker_layout.close, theme.radius.small, colors.shade);
    }
    paint_icon(
        ctx,
        IconName::Close,
        picker_layout.close.deflate(Insets::splat(7.0)),
        colors.text,
    );
    if let Some(search) = picker_layout.search {
        picker.query.paint(ctx, search);
    }
    let items = app.picker_items();
    ctx.scene.push_clip(picker_layout.list);
    if items.is_empty() {
        ctx.label_center(
            picker_layout.list.center().x,
            picker_layout.list.y() + 24.0,
            &tr("No results"),
            theme.typography.body.size,
            weight::NORMAL,
            colors.dim_text,
        );
    }
    for (index, rect) in &picker_layout.rows {
        let Some(item) = items.get(*index) else {
            continue;
        };
        if rect.bottom() < picker_layout.list.y() || rect.y() > picker_layout.list.bottom() {
            continue;
        }
        let hovered = app.hover == Some(Hit::PickerRow(*index));
        let highlighted = picker.selected == *index;
        let bg = if item.selected {
            colors.accent.scale_alpha(0.14)
        } else if highlighted || hovered {
            colors.shade
        } else {
            colors.card_bg
        };
        ctx.scene.fill_rounded_rect(*rect, theme.radius.small, bg);
        if highlighted {
            ctx.scene
                .stroke_rounded_rect(*rect, theme.radius.small, 1.0, colors.accent);
        }
        let label = ellipsize(
            ctx,
            &item.label,
            (rect.width() - 44.0).max(20.0),
            theme.typography.body.size,
            weight::NORMAL,
        );
        ctx.label_left(
            rect.x() + 10.0,
            rect.center().y,
            &label,
            theme.typography.body.size,
            weight::NORMAL,
            if item.enabled {
                colors.text
            } else {
                colors.dim_text
            },
        );
        if item.selected {
            paint_icon(
                ctx,
                IconName::Check,
                Rect::from_xywh(rect.right() - 26.0, rect.center().y - 8.0, 16.0, 16.0),
                colors.accent,
            );
        }
    }
    ctx.scene.pop_clip();
    let mut scroll = picker.scroll;
    scroll.set_content_height(picker_layout.content_height);
    scroll.set_offset(picker_layout.offset, picker_layout.list);
    scroll.paint_scrollbar(ctx, picker_layout.list);
}

fn paint_step_title(app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    if !app.step.is_wizard() {
        return;
    }
    let description = step_description(app.step);
    if description.is_empty() {
        return;
    }
    ctx.label_left(
        lay.viewport.x(),
        lay.viewport.y() - 22.0,
        &description,
        ctx.theme.typography.caption.size,
        weight::NORMAL,
        ctx.theme.colors.dim_text,
    );
}

fn paint_labels(ctx: &mut PaintCtx, lay: &Layout) {
    let theme = *ctx.theme;
    for (text, rect) in &lay.labels {
        ctx.label_left(
            rect.x(),
            rect.center().y,
            text,
            theme.typography.body.size,
            weight::BOLD,
            theme.colors.text,
        );
    }
    for (text, rect) in &lay.notes {
        ctx.label_left(
            rect.x(),
            rect.center().y,
            text,
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    }
}

fn paint_welcome(_app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let area = lay.viewport;
    let icon = Rect::from_xywh(area.x(), area.y(), 56.0, 56.0);
    ctx.scene
        .fill_rounded_rect(icon, 28.0, colors.accent.scale_alpha(0.14));
    paint_icon(
        ctx,
        IconName::App,
        icon.deflate(Insets::splat(13.0)),
        colors.accent,
    );
    ctx.label_left(
        icon.right() + theme.spacing.md,
        icon.center().y - 8.0,
        &tr("Install Luna"),
        theme.typography.title.size,
        weight::BOLD,
        colors.text,
    );
    ctx.label_left(
        icon.right() + theme.spacing.md,
        icon.center().y + 20.0,
        &tr("A guided installation of the Luna desktop"),
        theme.typography.body.size,
        weight::NORMAL,
        colors.dim_text,
    );
}

fn paint_language(app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    if let Some(rect) = lay.row(Hit::OpenPicker(PickerKind::Language)) {
        let value = app
            .selected_language()
            .map(|language| language.label())
            .unwrap_or_else(|| "—".to_owned());
        paint_select_row(
            ctx,
            rect,
            &value,
            app.hover == Some(Hit::OpenPicker(PickerKind::Language)),
        );
    }
    if let Some(rect) = lay.row(Hit::OpenPicker(PickerKind::Layout)) {
        let value = app
            .xkb
            .layouts
            .iter()
            .find(|option| option.code == app.keyboard.layout)
            .map(|option| option.label())
            .unwrap_or_else(|| app.keyboard.layout.clone());
        paint_select_row(
            ctx,
            rect,
            &value,
            app.hover == Some(Hit::OpenPicker(PickerKind::Layout)),
        );
    }
    if let Some(rect) = lay.row(Hit::OpenPicker(PickerKind::Variant)) {
        let value = variants_for(app)
            .iter()
            .find(|variant| variant.code == app.keyboard.variant)
            .map(|variant| format!("{} — {}", variant.code, variant.name))
            .unwrap_or_else(|| tr("No variant"));
        paint_select_row(
            ctx,
            rect,
            &value,
            app.hover == Some(Hit::OpenPicker(PickerKind::Variant)),
        );
    }
    if let Some(rect) = lay.row(Hit::OpenPicker(PickerKind::Model)) {
        let value = app
            .xkb
            .models
            .iter()
            .find(|option| option.code == app.keyboard.model)
            .map(|option| option.label())
            .unwrap_or_else(|| app.keyboard.model.clone());
        paint_select_row(
            ctx,
            rect,
            &value,
            app.hover == Some(Hit::OpenPicker(PickerKind::Model)),
        );
    }
}

fn paint_region(app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    if let Some(rect) = lay.row(Hit::OpenPicker(PickerKind::Timezone)) {
        paint_select_row(
            ctx,
            rect,
            &app.timezone,
            app.hover == Some(Hit::OpenPicker(PickerKind::Timezone)),
        );
    }
}

fn paint_network(app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let status = Rect::from_xywh(
        lay.viewport.x(),
        lay.viewport.y(),
        lay.viewport.width(),
        54.0,
    );
    let color = if app.network.connected && app.network.internet {
        colors.success
    } else if app.network.connected {
        colors.warning
    } else {
        colors.error
    };
    ctx.scene
        .fill_rounded_rect(status, theme.radius.medium, colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(status, theme.radius.medium, 1.0, colors.border);
    ctx.label_left(
        status.x() + theme.spacing.md,
        status.center().y - 8.0,
        &app.network.status(),
        theme.typography.body.size,
        weight::MEDIUM,
        color,
    );
    if let Some(interface) = &app.network.interface {
        ctx.label_left(
            status.x() + theme.spacing.md,
            status.center().y + 12.0,
            &tr_args("Interface: {name}", &[("name", interface)]),
            theme.typography.caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
    }
    if let Some(rect) = lay.row(Hit::RetryNetwork) {
        let mut button = Button::new(tr("Rescan networks")).variant(ButtonVariant::Flat);
        button.set_hovered(app.hover == Some(Hit::RetryNetwork));
        button.paint(ctx, rect);
    }
    for (index, wifi) in app.network.wifi.iter().enumerate() {
        if let Some(rect) = lay.row(Hit::Wifi(index)) {
            paint_choice_row(
                ctx,
                rect,
                &wifi.label(),
                app.selected_wifi == Some(index),
                app.hover == Some(Hit::Wifi(index)),
                true,
            );
        }
    }
    paint_entry(app, ctx, lay, Field::WifiPassword);
    if let Some(rect) = lay.row(Hit::ConnectWifi) {
        let mut button = Button::new(tr("Connect")).variant(ButtonVariant::Suggested);
        button.set_hovered(app.hover == Some(Hit::ConnectWifi));
        button.paint(ctx, rect);
    }
}

fn paint_requirements(app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    if let Some(rect) = lay.row(Hit::RefreshChecks) {
        let mut button = Button::new(tr("Check again")).variant(ButtonVariant::Flat);
        button.set_hovered(app.hover == Some(Hit::RefreshChecks));
        button.paint(ctx, rect);
    }
    for (check, rect) in &lay.check_rows {
        let color = match check.level {
            Level::Ok => colors.success,
            Level::Info => colors.accent,
            Level::Warning => colors.warning,
            Level::Error => colors.error,
        };
        ctx.scene
            .fill_rounded_rect(*rect, theme.radius.small, colors.card_bg);
        ctx.label_left(
            rect.x() + theme.spacing.md,
            rect.y() + 12.0,
            &check.title,
            theme.typography.body.size,
            weight::MEDIUM,
            colors.text,
        );
        ctx.label_left(
            rect.x() + theme.spacing.md,
            rect.y() + 30.0,
            &ellipsize(
                ctx,
                &check.detail,
                (rect.width() - 2.0 * theme.spacing.md).max(40.0),
                theme.typography.caption.size,
                weight::NORMAL,
            ),
            theme.typography.caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
        ctx.label_right(
            rect.right() - theme.spacing.md,
            rect.y() + 12.0,
            &check.level.label(),
            theme.typography.caption.size,
            weight::BOLD,
            color,
        );
    }
}

fn paint_disk(app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    if let Some(rect) = lay.row(Hit::OpenPicker(PickerKind::Disk)) {
        let value = app
            .selected_disk()
            .map(|disk| {
                let mut label = format!("{}  ·  {}", disk.path, disk.detail());
                if disk.free_bytes > 0 {
                    label.push_str(&format!("  ·  {} free", format_size(disk.free_bytes)));
                }
                label
            })
            .unwrap_or_else(|| "—".to_owned());
        paint_select_row(
            ctx,
            rect,
            &value,
            app.hover == Some(Hit::OpenPicker(PickerKind::Disk)),
        );
    }
    if let Some(rect) = lay.row(Hit::OpenPicker(PickerKind::Mode)) {
        paint_select_row(
            ctx,
            rect,
            &app.options.mode.label(),
            app.hover == Some(Hit::OpenPicker(PickerKind::Mode)),
        );
    }
    if app.options.mode == PartitionMode::Manual {
        if let Some(rect) = lay.row(Hit::OpenPicker(PickerKind::RootPartition)) {
            paint_select_row(
                ctx,
                rect,
                app.options.root_partition.as_deref().unwrap_or("—"),
                app.hover == Some(Hit::OpenPicker(PickerKind::RootPartition)),
            );
        }
        if app.options.uefi {
            if let Some(rect) = lay.row(Hit::OpenPicker(PickerKind::EfiPartition)) {
                paint_select_row(
                    ctx,
                    rect,
                    app.options.efi_partition.as_deref().unwrap_or("—"),
                    app.hover == Some(Hit::OpenPicker(PickerKind::EfiPartition)),
                );
            }
        }
        if let Some(rect) = lay.row(Hit::OpenPicker(PickerKind::SwapPartition)) {
            paint_select_row(
                ctx,
                rect,
                app.options.swap_partition.as_deref().unwrap_or("—"),
                app.hover == Some(Hit::OpenPicker(PickerKind::SwapPartition)),
            );
        }
    }
    if let Some(rect) = lay.row(Hit::OpenPicker(PickerKind::Filesystem)) {
        paint_select_row(
            ctx,
            rect,
            app.options.filesystem.label(),
            app.hover == Some(Hit::OpenPicker(PickerKind::Filesystem)),
        );
    }
    if let Some(rect) = lay.row(Hit::OpenPicker(PickerKind::SwapSize)) {
        paint_select_row(
            ctx,
            rect,
            &app.options.swap.label(),
            app.hover == Some(Hit::OpenPicker(PickerKind::SwapSize)),
        );
    }
    if let Some(rect) = lay.row(Hit::OpenPicker(PickerKind::Bootloader)) {
        paint_select_row(
            ctx,
            rect,
            &app.options.bootloader.label(),
            app.hover == Some(Hit::OpenPicker(PickerKind::Bootloader)),
        );
    }
    if let Some(rect) = lay.row(Hit::EncryptionToggle) {
        paint_check_row(
            ctx,
            rect,
            &tr("Encrypt the system with LUKS2"),
            app.options.encrypt,
            app.hover == Some(Hit::EncryptionToggle),
        );
    }
    paint_entry(app, ctx, lay, Field::LuksPassword);
    paint_entry(app, ctx, lay, Field::LuksConfirm);
    if let Some(rect) = lay.row(Hit::UpdateToggle) {
        paint_check_row(
            ctx,
            rect,
            &tr("Update the system during installation"),
            app.options.update_system,
            app.hover == Some(Hit::UpdateToggle),
        );
    }
}

fn paint_select_row(ctx: &mut PaintCtx, rect: Rect, value: &str, hovered: bool) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    ctx.scene.fill_rounded_rect(
        rect,
        theme.radius.small,
        if hovered {
            colors.shade
        } else {
            colors.card_bg
        },
    );
    ctx.scene
        .stroke_rounded_rect(rect, theme.radius.small, 1.0, colors.border);
    let label = ellipsize(
        ctx,
        value,
        (rect.width() - 44.0).max(20.0),
        theme.typography.body.size,
        weight::MEDIUM,
    );
    ctx.label_left(
        rect.x() + 12.0,
        rect.center().y,
        &label,
        theme.typography.body.size,
        weight::MEDIUM,
        colors.text,
    );
    paint_icon(
        ctx,
        IconName::ChevronRight,
        Rect::from_xywh(rect.right() - 28.0, rect.center().y - 7.0, 14.0, 14.0),
        colors.dim_text,
    );
}

fn paint_account(app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    for field in App::ACCOUNT_FIELDS {
        paint_entry(app, ctx, lay, field);
    }
    if let Some(rect) = lay.row(Hit::AutologinToggle) {
        paint_check_row(
            ctx,
            rect,
            &tr("Log in automatically"),
            app.autologin,
            app.hover == Some(Hit::AutologinToggle),
        );
    }
    if let Some(rect) = lay.row(Hit::OpenPicker(PickerKind::RootMode)) {
        paint_select_row(
            ctx,
            rect,
            &app.root_mode.label(),
            app.hover == Some(Hit::OpenPicker(PickerKind::RootMode)),
        );
    }
    if app.root_mode == RootMode::Custom {
        paint_entry(app, ctx, lay, Field::RootPassword);
        paint_entry(app, ctx, lay, Field::RootConfirm);
    }
}

fn paint_summary(app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    for (label, value, rect) in &lay.summary_rows {
        let label_width = rect.width() * 0.38;
        let value_width = rect.width() * 0.58;
        let label = ellipsize(
            ctx,
            label,
            label_width,
            ctx.theme.typography.body.size,
            weight::NORMAL,
        );
        let value = ellipsize(
            ctx,
            value,
            value_width,
            ctx.theme.typography.body.size,
            weight::MEDIUM,
        );
        ctx.label_left(
            rect.x(),
            rect.center().y,
            &label,
            ctx.theme.typography.body.size,
            weight::NORMAL,
            ctx.theme.colors.dim_text,
        );
        ctx.label_right(
            rect.right() - 14.0,
            rect.center().y,
            &value,
            ctx.theme.typography.body.size,
            weight::MEDIUM,
            ctx.theme.colors.text,
        );
    }
    if let Some(rect) = lay.row(Hit::ConfirmErase) {
        paint_check_row(
            ctx,
            rect,
            &app.confirm_label(),
            app.confirm_erase,
            app.hover == Some(Hit::ConfirmErase),
        );
    }
}

fn paint_progress(app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let mut spinner = Spinner::new();
    spinner.set_progress(app.spinner);
    let side = spinner.size(&theme);
    spinner.paint(
        ctx,
        Rect::from_xywh(
            lay.viewport.center().x - side / 2.0,
            lay.viewport.y() + 6.0,
            side,
            side,
        ),
    );
    ctx.label_center(
        lay.viewport.center().x,
        lay.viewport.y() + 6.0 + side + 26.0,
        &tr("Installing Luna"),
        theme.typography.heading.size,
        weight::BOLD,
        colors.text,
    );
    let bar = ProgressBar::new()
        .fraction(app.progress)
        .text(format!("{:.0}%", app.progress * 100.0));
    bar.paint(
        ctx,
        Rect::from_xywh(
            lay.viewport.x(),
            lay.viewport.y() + 6.0 + side + 54.0,
            lay.viewport.width(),
            18.0,
        ),
    );
    let log_area = Rect::from_xywh(
        lay.viewport.x(),
        lay.viewport.y() + 104.0,
        lay.viewport.width(),
        (lay.viewport.height() - 110.0).max(120.0),
    );
    if !app.show_log {
        return;
    }
    ctx.scene
        .fill_rounded_rect(log_area, theme.radius.medium, colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(log_area, theme.radius.medium, 1.0, colors.border);
    ctx.scene.push_clip(log_area.deflate(Insets::splat(10.0)));
    let line_h = theme.typography.caption.line_height;
    let visible = ((log_area.height() - 20.0) / line_h).floor().max(0.0) as usize;
    let start = app.log.len().saturating_sub(visible);
    let mut y = log_area.y() + 10.0 + line_h / 2.0;
    for line in app.log[start..].iter() {
        ctx.label_left(
            log_area.x() + 10.0,
            y,
            line,
            theme.typography.caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
        y += line_h;
    }
    ctx.scene.pop_clip();
}

fn paint_done(app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let success = matches!(app.finished, Some(Ok(())));
    let (color, icon) = if success {
        (colors.success, IconName::Check)
    } else {
        (colors.error, IconName::Close)
    };
    let area = lay.area;
    let circle = Rect::from_xywh(area.center().x - 32.0, area.y() + 30.0, 64.0, 64.0);
    ctx.scene
        .fill_rounded_rect(circle, 32.0, color.scale_alpha(0.14));
    paint_icon(ctx, icon, circle.deflate(Insets::splat(18.0)), color);
    let title = if success {
        tr("Luna is installed")
    } else {
        tr("Installation failed")
    };
    ctx.label_center(
        area.center().x,
        circle.bottom() + 30.0,
        &title,
        theme.typography.heading.size,
        weight::BOLD,
        colors.text,
    );
    let description = match &app.finished {
        Some(Ok(())) => tr("Remove the installation medium, then restart the computer."),
        Some(Err(error)) => error.clone(),
        None => tr("The installer has stopped."),
    };
    ctx.label_center(
        area.center().x,
        circle.bottom() + 56.0,
        &description,
        theme.typography.body.size,
        weight::NORMAL,
        colors.dim_text,
    );
}

fn paint_fixed_controls(app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    if let Some(rect) = lay.save_log {
        let mut button = Button::new(tr("Save log")).variant(ButtonVariant::Flat);
        button.set_hovered(app.hover == Some(Hit::SaveLog));
        button.paint(ctx, rect);
    }
    if app.step == Step::Progress {
        if let Some(rect) = lay.log_toggle {
            let mut button = Button::new(if app.show_log {
                tr("Hide details")
            } else {
                tr("Show details")
            })
            .variant(ButtonVariant::Flat);
            button.set_hovered(app.hover == Some(Hit::ToggleLog));
            button.paint(ctx, rect);
        }
    }
    for (hit, label, variant) in [
        (Hit::Restart, tr("Restart now"), ButtonVariant::Suggested),
        (Hit::PowerOff, tr("Power off"), ButtonVariant::Flat),
        (Hit::Retry, tr("Try again"), ButtonVariant::Suggested),
        (Hit::Close, tr("Close"), ButtonVariant::Flat),
    ] {
        if let Some(rect) = lay.row(hit) {
            let mut button = Button::new(label).variant(variant);
            button.set_hovered(app.hover == Some(hit));
            button.paint(ctx, rect);
        }
    }
}

fn paint_entry(app: &App, ctx: &mut PaintCtx, lay: &Layout, field: Field) {
    let Some(rect) = lay
        .entries
        .iter()
        .find(|(candidate, _)| *candidate == field)
        .map(|(_, rect)| *rect)
    else {
        return;
    };
    let Some(entry) = app.form.entry(field) else {
        return;
    };
    let theme = *ctx.theme;
    if !matches!(field, Field::LayoutSearch | Field::TimezoneSearch) {
        ctx.label_left(
            rect.x(),
            rect.y() - 10.0,
            &field_label(field),
            theme.typography.caption.size,
            weight::MEDIUM,
            theme.colors.dim_text,
        );
    }
    entry.paint(
        ctx,
        Rect::from_xywh(rect.x(), rect.y(), rect.width(), theme.metrics.entry.height),
    );
    if let Some(issue) = app.issue_for(field) {
        ctx.label_left(
            rect.x(),
            rect.y() + theme.metrics.entry.height + 10.0,
            issue,
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.error,
        );
    }
}

fn field_label(field: Field) -> String {
    match field {
        Field::FullName => tr("Full name"),
        Field::Username => tr("Username"),
        Field::Hostname => tr("Computer name"),
        Field::Password => tr("Password"),
        Field::ConfirmPassword => tr("Confirm password"),
        Field::RootPassword => tr("Root password"),
        Field::RootConfirm => tr("Confirm root password"),
        Field::LuksPassword => tr("Encryption passphrase"),
        Field::LuksConfirm => tr("Confirm passphrase"),
        Field::LayoutSearch => tr("Search keyboard layouts"),
        Field::TimezoneSearch => tr("Search timezones"),
        Field::WifiPassword => tr("Wi-Fi password"),
        Field::Disk => tr("Disk"),
        Field::Confirmation => tr("Confirmation"),
    }
}

fn paint_choice_row(
    ctx: &mut PaintCtx,
    rect: Rect,
    label: &str,
    selected: bool,
    hovered: bool,
    enabled: bool,
) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let bg = if selected {
        colors.accent.scale_alpha(0.12)
    } else if hovered {
        colors.shade
    } else {
        colors.card_bg
    };
    ctx.scene.fill_rounded_rect(rect, theme.radius.small, bg);
    if selected {
        ctx.scene
            .stroke_rounded_rect(rect, theme.radius.small, 1.0, colors.accent);
    }
    let radio = Rect::from_xywh(rect.x() + 10.0, rect.center().y - 7.0, 14.0, 14.0);
    ctx.scene.fill_rounded_rect(
        radio,
        7.0,
        if selected {
            if enabled {
                colors.accent
            } else {
                colors.dim_text
            }
        } else {
            colors.border
        },
    );
    if selected {
        ctx.scene
            .fill_rounded_rect(radio.deflate(Insets::splat(4.0)), 3.0, colors.accent_fg);
    }
    let color = if enabled {
        colors.text
    } else {
        colors.dim_text
    };
    let label = ellipsize(
        ctx,
        label,
        (rect.width() - 44.0).max(20.0),
        theme.typography.body.size,
        if selected {
            weight::MEDIUM
        } else {
            weight::NORMAL
        },
    );
    ctx.label_left(
        rect.x() + 34.0,
        rect.center().y,
        &label,
        theme.typography.body.size,
        if selected {
            weight::MEDIUM
        } else {
            weight::NORMAL
        },
        color,
    );
}

fn ellipsize(ctx: &PaintCtx, text: &str, max_width: f64, size: f64, weight: u16) -> String {
    if ctx.measure(text, size, weight) <= max_width {
        return text.to_owned();
    }
    let dots = "…";
    let mut out = String::new();
    for ch in text.chars() {
        let candidate = format!("{out}{ch}");
        if ctx.measure(&format!("{candidate}{dots}"), size, weight) > max_width {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

fn paint_check_row(ctx: &mut PaintCtx, rect: Rect, label: &str, checked: bool, hovered: bool) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    if hovered {
        ctx.scene
            .fill_rounded_rect(rect, theme.radius.small, colors.shade);
    }
    let box_ = Rect::from_xywh(rect.x() + 10.0, rect.center().y - 8.0, 16.0, 16.0);
    if checked {
        ctx.scene
            .fill_rounded_rect(box_, theme.radius.small, colors.accent);
        paint_icon(
            ctx,
            IconName::Check,
            box_.deflate(Insets::splat(3.0)),
            colors.accent_fg,
        );
    } else {
        ctx.scene
            .stroke_rounded_rect(box_, theme.radius.small, 1.0, colors.border);
    }
    ctx.label_left(
        rect.x() + 36.0,
        rect.center().y,
        label,
        theme.typography.body.size,
        weight::NORMAL,
        colors.text,
    );
}

fn paint_banner(ctx: &mut PaintCtx, message: &str, area: Rect) {
    let width = (area.width().min(780.0) - 2.0 * PAD).max(160.0);
    let banner = Rect::from_xywh(area.center().x - width / 2.0, area.y() + 10.0, width, 58.0);
    Banner::new(tr("Notice"))
        .kind(BannerKind::Error)
        .description(message)
        .paint(ctx, banner);
}

/// Filtrowane listy używane zarówno w układzie, jak i w rysowaniu.
pub fn variants_for(app: &App) -> Vec<&XkbVariant> {
    app.xkb
        .variants
        .iter()
        .filter(|variant| variant.layout == app.keyboard.layout)
        .collect()
}

/// Obsługa zdarzeń.
pub fn handle_events(
    app: &mut App,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let lay = layout(app, size, theme, text);
    app.scroll.set_content_height(lay.content_height);
    app.scroll.set_offset(lay.offset, lay.viewport);
    if let (Some(picker_layout), Some(picker)) = (&lay.picker, &mut app.picker) {
        picker
            .scroll
            .set_content_height(picker_layout.content_height);
        picker
            .scroll
            .set_offset(picker_layout.offset, picker_layout.list);
    }

    for event in events {
        match event {
            Event::PointerMotion { position, .. } => {
                app.pointer = Some(*position);
                app.hover = hit_test(app, &lay, theme, *position);
            }
            Event::PointerLeave => {
                app.pointer = None;
                app.hover = None;
            }
            Event::Scroll(scroll) => {
                if let (Some(picker_layout), Some(picker)) = (&lay.picker, &mut app.picker) {
                    if picker_layout.list.contains(scroll.position) {
                        picker
                            .scroll
                            .on_scroll(scroll.dy / SCROLL_UNIT * SCROLL_STEP, picker_layout.list);
                    }
                } else if lay.viewport.contains(scroll.position) {
                    app.scroll
                        .on_scroll(scroll.dy / SCROLL_UNIT * SCROLL_STEP, lay.viewport);
                }
            }
            Event::PointerButton(button)
                if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
            {
                handle_click(app, &lay, theme, button.position);
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                handle_key(app, &lay, key);
            }
            Event::Text(value) => {
                if let Some(picker) = &mut app.picker {
                    if picker.kind.searchable() {
                        picker.query.insert_str(value);
                        picker.selected = 0;
                    }
                } else if let Some(entry) = app.form.entry_mut(app.focus) {
                    entry.insert_str(value);
                }
            }
            Event::Window(libluna_core::WindowEvent::CloseRequested) => {
                app.pending.close = true;
            }
            _ => {}
        }
    }
}

fn hit_test(app: &App, lay: &Layout, theme: &Theme, point: Point) -> Option<Hit> {
    if let Some(picker) = &lay.picker {
        if picker.close.contains(point) {
            return Some(Hit::PickerClose);
        }
        if picker.search.is_some_and(|rect| rect.contains(point)) {
            return Some(Hit::PickerSearch);
        }
        for (index, rect) in &picker.rows {
            if rect.contains(point) {
                return Some(Hit::PickerRow(*index));
            }
        }
        return if picker.panel.contains(point) {
            None
        } else {
            Some(Hit::PickerClose)
        };
    }
    if let Some(control) = crate::app::window_control_at(lay.header, theme, point) {
        return Some(Hit::WindowControl(control));
    }
    for (field, rect) in &lay.entries {
        if rect.contains(point) {
            return Some(Hit::Entry(*field));
        }
    }
    for (hit, rect) in &lay.rows {
        if rect.contains(point) {
            return Some(*hit);
        }
    }
    if lay.save_log.is_some_and(|rect| rect.contains(point)) {
        return Some(Hit::SaveLog);
    }
    if lay.log_toggle.is_some_and(|rect| rect.contains(point)) {
        return Some(Hit::ToggleLog);
    }
    if lay.next.is_some_and(|rect| rect.contains(point)) {
        return Some(Hit::Next);
    }
    if lay.cancel.is_some_and(|rect| rect.contains(point)) {
        return Some(Hit::Back);
    }
    let _ = app;
    None
}

fn handle_click(app: &mut App, lay: &Layout, theme: &Theme, point: Point) {
    if lay.picker.is_some() {
        match hit_test(app, lay, theme, point) {
            Some(Hit::PickerClose) => app.close_picker(),
            Some(Hit::PickerSearch) => {
                if let Some(picker) = &mut app.picker {
                    picker.query.set_focused(true);
                }
            }
            Some(Hit::PickerRow(index)) => app.apply_picker_row(index),
            _ => {}
        }
        return;
    }
    if let Some(control) = crate::app::window_control_at(lay.header, theme, point) {
        match control {
            WindowControl::Minimize => app.pending.minimize = true,
            WindowControl::Maximize => app.pending.maximize = true,
            WindowControl::Close => app.pending.close = true,
        }
        return;
    }
    if lay.header.contains(point) {
        app.pending.begin_move = true;
        return;
    }
    let Some(hit) = hit_test(app, lay, theme, point) else {
        return;
    };
    match hit {
        Hit::Next => app.next(),
        Hit::Back => app.cancel(),
        Hit::Close => app.cancel(),
        Hit::Restart => app.restart(),
        Hit::PowerOff => app.power_off(),
        Hit::SaveLog => app.save_log(),
        Hit::Retry => app.retry(),
        Hit::ToggleLog => app.show_log = !app.show_log,
        Hit::RetryNetwork => {
            crate::network::enable_wifi();
            app.refresh_network();
        }
        Hit::ConnectWifi => app.connect_selected_wifi(),
        Hit::RefreshChecks => app.refresh_checks(),
        Hit::OpenPicker(kind) => app.open_picker(kind),
        Hit::PickerClose => app.close_picker(),
        Hit::PickerRow(index) => app.apply_picker_row(index),
        Hit::PickerSearch => {}
        Hit::Language(index) => app.set_language(index),
        Hit::Layout(index) => app.select_layout(index),
        Hit::Variant(index) => app.select_variant(index),
        Hit::VariantNone => app.clear_variant(),
        Hit::Model(index) => app.select_model(index),
        Hit::Timezone(index) => app.select_timezone(index),
        Hit::Wifi(index) => {
            app.selected_wifi = Some(index);
            if app.network.wifi.get(index).is_some_and(|wifi| !wifi.open()) {
                app.set_focus(Field::WifiPassword);
            }
            if let Some(wifi) = app.network.wifi.get(index).cloned() {
                if wifi.open() {
                    app.connect_selected_wifi();
                }
            }
        }
        Hit::Disk(index) => app.select_disk(index),
        Hit::Mode(mode) => app.set_mode(mode),
        Hit::RootPartition(index) => app.set_root_partition(index),
        Hit::EfiPartition(index) => app.set_efi_partition(index),
        Hit::SwapPartition(index) => app.set_swap_partition(index),
        Hit::SwapPartitionNone => app.clear_swap_partition(),
        Hit::Filesystem(filesystem) => app.set_filesystem(filesystem),
        Hit::Swap(swap) => app.set_swap(swap),
        Hit::Bootloader(bootloader) => app.set_bootloader(bootloader),
        Hit::RootMode(mode) => app.set_root_mode(mode),
        Hit::EncryptionToggle => app.toggle_encryption(),
        Hit::AutologinToggle => app.toggle_autologin(),
        Hit::UpdateToggle => app.toggle_update(),
        Hit::ConfirmErase => app.confirm_erase = !app.confirm_erase,
        Hit::Entry(field) => app.set_focus(field),
        Hit::WindowControl(_) => {}
    }
}

fn handle_key(app: &mut App, lay: &Layout, key: &KeyEvent) {
    if app.picker.is_some() {
        handle_picker_key(app, lay, key);
        return;
    }
    if key.modifiers.ctrl && key.keysym == 0x77 {
        app.pending.close = true;
        return;
    }
    match key.keysym {
        0xff1b => {
            app.cancel();
            return;
        }
        0xff09 => {
            app.focus_step(if key.modifiers.shift { -1 } else { 1 });
            return;
        }
        0xff0d | 0xff8d => {
            if app.step == Step::Done {
                app.restart();
            } else {
                app.next();
            }
            return;
        }
        0xff52 => {
            app.scroll.on_scroll(-SCROLL_STEP, lay.viewport);
            return;
        }
        0xff54 => {
            app.scroll.on_scroll(SCROLL_STEP, lay.viewport);
            return;
        }
        _ => {}
    }
    if let Some(entry) = app.form.entry_mut(app.focus) {
        let was_search = matches!(app.focus, Field::LayoutSearch | Field::TimezoneSearch);
        if text_key(entry, key) && was_search {
            app.scroll.set_offset(0.0, lay.viewport);
        }
    }
}

fn handle_picker_key(app: &mut App, lay: &Layout, key: &KeyEvent) {
    let items_len = app.picker_items().len();
    let mut close = false;
    let mut apply = false;
    if let Some(picker) = &mut app.picker {
        match key.keysym {
            0xff1b => close = true,
            0xff52 => picker.selected = picker.selected.saturating_sub(1),
            0xff54 => {
                if items_len > 0 {
                    picker.selected = (picker.selected + 1).min(items_len - 1);
                }
            }
            0xff0d | 0xff8d => apply = true,
            _ => {
                if picker.kind.searchable() && text_key(&mut picker.query, key) {
                    picker.selected = 0;
                    if let Some(picker_layout) = &lay.picker {
                        picker.scroll.set_offset(0.0, picker_layout.list);
                    }
                }
            }
        }
    }
    if close {
        app.close_picker();
    }
    if apply {
        app.apply_picker_selected();
    }
}

fn text_key(entry: &mut Entry, key: &KeyEvent) -> bool {
    let ctrl = key.modifiers.ctrl;
    if ctrl && key.keysym == 0x61 {
        entry.select_all();
        return true;
    }
    if !ctrl {
        if let Some(text) = &key.text {
            if !text.is_empty() && !text.chars().all(char::is_control) {
                entry.insert_str(text);
                return true;
            }
        }
        if (0x20..=0x7e).contains(&key.keysym) {
            if let Some(ch) = char::from_u32(key.keysym) {
                entry.insert_char(ch);
                return true;
            }
        }
    }
    match key.keysym {
        0xff08 => entry.backspace(),
        0xffff => entry.delete_forward(),
        0xff51 => entry.move_left(),
        0xff53 => entry.move_right(),
        0xff50 => entry.home(),
        0xff57 => entry.end(),
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Step;
    use crate::disk::MIN_FREE_BYTES;
    use crate::keyboard::{XkbLists, XkbOption};
    use crate::live::LiveEnvironment;
    use crate::network::NetworkState;
    use crate::translations::LanguageOption;
    use std::path::PathBuf;

    fn app(step: Step) -> App {
        let xkb = XkbLists {
            layouts: vec![XkbOption {
                code: "pl".to_owned(),
                name: "Polish".to_owned(),
            }],
            variants: vec![],
            models: vec![XkbOption {
                code: "pc105".to_owned(),
                name: "Generic".to_owned(),
            }],
        };
        let mut app = App::with_data(
            LiveEnvironment {
                live: true,
                medium: None,
                reason: String::new(),
            },
            vec![LanguageOption {
                code: "pl".to_owned(),
                name: "Polski".to_owned(),
                locale: "pl_PL.UTF-8".to_owned(),
                coverage: 98,
            }],
            xkb,
            vec!["Europe/Warsaw".to_owned()],
            vec![crate::disk::Disk {
                name: "sda".to_owned(),
                path: "/dev/sda".to_owned(),
                size_bytes: 64_000_000_000,
                model: "Test".to_owned(),
                removable: false,
                free_bytes: MIN_FREE_BYTES,
                protection: None,
            }],
            NetworkState::default(),
            Some(PathBuf::from("/bin/true")),
        );
        app.step = step;
        app
    }

    #[test]
    fn wizard_has_footer_buttons() {
        let app = app(Step::Welcome);
        let theme = Theme::light();
        let text = TextEngine::system();
        let lay = layout(&app, Size::new(980.0, 760.0), &theme, &text);
        assert!(lay.next.is_some());
        assert!(lay.cancel.is_some());
    }

    #[test]
    fn language_step_lists_language_and_layout() {
        let app = app(Step::Language);
        let theme = Theme::light();
        let text = TextEngine::system();
        let lay = layout(&app, Size::new(980.0, 760.0), &theme, &text);
        assert!(lay.row(Hit::OpenPicker(PickerKind::Language)).is_some());
        assert!(lay.row(Hit::OpenPicker(PickerKind::Layout)).is_some());
    }

    #[test]
    fn disk_step_lists_modes() {
        let app = app(Step::Disk);
        let theme = Theme::light();
        let text = TextEngine::system();
        let lay = layout(&app, Size::new(980.0, 760.0), &theme, &text);
        assert!(lay.row(Hit::OpenPicker(PickerKind::Disk)).is_some());
        assert!(lay.row(Hit::OpenPicker(PickerKind::Mode)).is_some());
    }

    #[test]
    fn picker_layout_is_centered_and_has_rows() {
        let mut app = app(Step::Region);
        app.open_picker(PickerKind::Timezone);
        let theme = Theme::light();
        let text = TextEngine::system();
        let lay = layout(&app, Size::new(980.0, 760.0), &theme, &text);
        let picker = lay.picker.expect("picker layout");
        assert!(picker.panel.contains(lay.area.center()));
        assert!(!picker.rows.is_empty());
    }

    #[test]
    fn account_lists_entries() {
        let app = app(Step::Account);
        let theme = Theme::light();
        let text = TextEngine::system();
        let lay = layout(&app, Size::new(980.0, 760.0), &theme, &text);
        assert_eq!(lay.entries.len(), 5);
    }

    #[test]
    fn done_screen_offers_restart() {
        let mut app = app(Step::Done);
        app.finished = Some(Ok(()));
        let theme = Theme::light();
        let text = TextEngine::system();
        let lay = layout(&app, Size::new(980.0, 760.0), &theme, &text);
        assert!(lay.row(Hit::Restart).is_some());
        assert!(lay.row(Hit::PowerOff).is_some());
    }

    #[test]
    fn hit_test_finds_next() {
        let app = app(Step::Welcome);
        let theme = Theme::light();
        let text = TextEngine::system();
        let lay = layout(&app, Size::new(980.0, 760.0), &theme, &text);
        let next = lay.next.unwrap();
        assert_eq!(hit_test(&app, &lay, &theme, next.center()), Some(Hit::Next));
    }
}
