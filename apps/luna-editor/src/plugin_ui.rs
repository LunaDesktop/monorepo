//! Okna wtyczek: menedżer, ustawienia wtyczki i pytanie o instrukcję.

use std::collections::HashMap;

use libluna_core::style::weight;
use libluna_core::{
    ButtonState, Color, Event, Insets, KeyEvent, Point, PointerButton, Rect, Size, Theme,
};
use libluna_i18n::tr;
use libluna_render::TextEngine;
use libluna_widgets::{
    clipboard_command, hover_fill, paint_icon, Button, ButtonVariant, Entry, IconName, PaintCtx,
    TextClipboardCommand,
};

use crate::app::App;
use crate::plugins::{self, Context, Job, SettingKind};

const MAX_W: f64 = 860.0;
const MAX_H: f64 = 600.0;
const PAD: f64 = 16.0;
const ROW_H: f64 = 46.0;
const ROW_GAP: f64 = 6.0;
const LIST_W: f64 = 250.0;
const BOTTOM_H: f64 = 72.0;

/// Stan okien wtyczek.
pub struct PluginUi {
    pub open: bool,
    pub selected: usize,
    pub scroll: f64,
    pub pointer: Option<Point>,
    pub url: Entry,
    pub status: Option<String>,
    pub prompt: Option<Prompt>,
    pub form: Option<Form>,
}

/// Pytanie o instrukcję dla polecenia wtyczki.
pub struct Prompt {
    pub plugin: usize,
    pub command: usize,
    pub entry: Entry,
}

/// Formularz ustawień wtyczki.
pub struct Form {
    pub plugin: usize,
    pub entries: Vec<Option<Entry>>,
    pub selections: Vec<usize>,
    pub active: usize,
    /// Opcje wyboru dla każdego ustawienia (statyczne albo pobrane modele).
    pub options: Vec<Vec<String>>,
    /// Czy opcje są pobierane z wtyczki (`setting = ...|models|`).
    pub dynamic: Vec<bool>,
    pub loading: Vec<bool>,
    /// Czy trzeba ponownie pobrać listę (np. po zmianie klucza API).
    pub needs_refresh: Vec<bool>,
    pub error: Option<String>,
    /// Otwarty popup wyboru wartości.
    pub picker: Option<PickerPopup>,
    /// Po udanym pobraniu modeli otwórz popup automatycznie.
    pub pending_open: Option<usize>,
}

/// Popup wyboru (modele LLM, dostawca itp.) z filtrowaniem i przewijaniem.
pub struct PickerPopup {
    pub setting: usize,
    pub anchor: Rect,
    pub query: Entry,
    pub scroll: f64,
    pub hover: Option<usize>,
}

impl Default for PluginUi {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginUi {
    pub fn new() -> Self {
        let url = Entry::new(tr("https://github.com/owner/repo"));
        Self {
            open: false,
            selected: 0,
            scroll: 0.0,
            pointer: None,
            url,
            status: None,
            prompt: None,
            form: None,
        }
    }
}

struct ManagerLayout {
    card: Rect,
    close: Rect,
    list: Rect,
    rows: Vec<Rect>,
    detail: Rect,
    commands: Vec<Rect>,
    settings: Rect,
    remove: Rect,
    url: Rect,
    install: Rect,
    status: Rect,
}

fn manager_layout(app: &App, size: Size, _theme: &Theme, _text: &TextEngine) -> ManagerLayout {
    let width = (size.width - 80.0).clamp(640.0, MAX_W);
    let height = (size.height - 80.0).clamp(420.0, MAX_H);
    let card = Rect::from_xywh(
        (size.width - width) / 2.0,
        (size.height - height) / 2.0,
        width,
        height,
    );
    let close = Rect::from_xywh(card.right() - 38.0, card.y() + 12.0, 26.0, 26.0);
    let list = Rect::from_xywh(
        card.x() + PAD,
        card.y() + 54.0,
        LIST_W,
        (card.height() - 54.0 - BOTTOM_H - PAD).max(40.0),
    );
    let detail = Rect::from_xywh(
        list.right() + PAD,
        list.y(),
        (card.right() - PAD - (list.right() + PAD)).max(40.0),
        list.height(),
    );
    let status = Rect::from_xywh(
        card.x() + PAD,
        card.bottom() - BOTTOM_H + 4.0,
        card.width() - 2.0 * PAD,
        18.0,
    );
    let install_w = 132.0;
    let url = Rect::from_xywh(
        card.x() + PAD,
        card.bottom() - 46.0,
        (card.width() - 2.0 * PAD - install_w - 8.0).max(80.0),
        32.0,
    );
    let install = Rect::from_xywh(url.right() + 8.0, url.y(), install_w, 32.0);

    let count = app.plugins.plugins.len();
    let total = count as f64 * (ROW_H + ROW_GAP);
    let scroll = app
        .plugin_ui
        .scroll
        .clamp(0.0, (total - list.height()).max(0.0));
    let mut rows = Vec::with_capacity(count);
    for index in 0..count {
        let y = list.y() + index as f64 * (ROW_H + ROW_GAP) - scroll;
        rows.push(Rect::from_xywh(list.x(), y, list.width(), ROW_H));
    }

    let commands_top = detail.y() + 74.0;
    let commands_bottom = detail.bottom() - 40.0;
    let mut commands = Vec::new();
    if let Some(plugin) = app.plugins.plugins.get(app.plugin_ui.selected) {
        for index in 0..plugin.commands.len() {
            let y = commands_top + index as f64 * 40.0;
            if y + 34.0 > commands_bottom {
                break;
            }
            commands.push(Rect::from_xywh(detail.x(), y, detail.width(), 34.0));
        }
    }
    let settings = Rect::from_xywh(detail.x(), detail.bottom() - 32.0, 116.0, 30.0);
    let remove = Rect::from_xywh(settings.right() + 8.0, settings.y(), 108.0, 30.0);

    ManagerLayout {
        card,
        close,
        list,
        rows,
        detail,
        commands,
        settings,
        remove,
        url,
        install,
        status,
    }
}

struct FormLayout {
    card: Rect,
    close: Rect,
    rows: Vec<(Rect, Rect)>,
    save: Rect,
    cancel: Rect,
}

fn form_layout(app: &App, size: Size, _theme: &Theme) -> Option<FormLayout> {
    let form = app.plugin_ui.form.as_ref()?;
    let plugin = app.plugins.plugins.get(form.plugin)?;
    let width = (size.width - 120.0).clamp(420.0, 560.0);
    let height = (140.0 + plugin.settings.len() as f64 * 56.0 + 60.0)
        .min(size.height - 80.0)
        .max(220.0);
    let card = Rect::from_xywh(
        (size.width - width) / 2.0,
        (size.height - height) / 2.0,
        width,
        height,
    );
    let close = Rect::from_xywh(card.right() - 38.0, card.y() + 12.0, 26.0, 26.0);
    let mut rows = Vec::with_capacity(plugin.settings.len());
    let mut y = card.y() + 64.0;
    for _ in &plugin.settings {
        let label = Rect::from_xywh(card.x() + 18.0, y, 150.0, 34.0);
        let control = Rect::from_xywh(
            card.x() + 180.0,
            y,
            (card.width() - 18.0 - 180.0).max(80.0),
            34.0,
        );
        rows.push((label, control));
        y += 56.0;
    }
    let save = Rect::from_xywh(
        card.right() - 18.0 - 108.0,
        card.bottom() - 46.0,
        108.0,
        32.0,
    );
    let cancel = Rect::from_xywh(save.x() - 8.0 - 96.0, save.y(), 96.0, 32.0);
    Some(FormLayout {
        card,
        close,
        rows,
        save,
        cancel,
    })
}

struct PromptLayout {
    card: Rect,
    entry: Rect,
    run: Rect,
    cancel: Rect,
}

fn prompt_layout(app: &App, size: Size, _theme: &Theme) -> Option<PromptLayout> {
    app.plugin_ui.prompt.as_ref()?;
    let width = (size.width - 140.0).clamp(420.0, 560.0);
    let height = 210.0;
    let card = Rect::from_xywh(
        (size.width - width) / 2.0,
        (size.height - height) / 2.0,
        width,
        height,
    );
    let entry = Rect::from_xywh(card.x() + 18.0, card.y() + 78.0, card.width() - 36.0, 34.0);
    let run = Rect::from_xywh(
        card.right() - 18.0 - 108.0,
        card.bottom() - 46.0,
        108.0,
        32.0,
    );
    let cancel = Rect::from_xywh(run.x() - 8.0 - 96.0, run.y(), 96.0, 32.0);
    Some(PromptLayout {
        card,
        entry,
        run,
        cancel,
    })
}

/// Rysuje menedżer wtyczek i okna podrzędne.
pub fn paint(app: &App, ctx: &mut PaintCtx, size: Size) {
    if app.plugin_ui.open {
        paint_manager(app, ctx, size);
    }
    if app.plugin_ui.prompt.is_some() {
        paint_prompt(app, ctx, size);
    }
    if app.plugin_ui.form.is_some() {
        paint_form(app, ctx, size);
    }
}

fn paint_manager(app: &App, ctx: &mut PaintCtx, size: Size) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let lay = manager_layout(app, size, &theme, ctx.text);
    let pointer = app.plugin_ui.pointer;
    let hovered = |rect: Rect| pointer.is_some_and(|p| rect.contains(p));

    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(lay.card, theme.radius.large, colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.card, theme.radius.large, 1.0, colors.border);
    ctx.label_left(
        lay.card.x() + PAD,
        lay.card.y() + 27.0,
        &tr("Plugins"),
        theme.typography.heading.size,
        weight::BOLD,
        colors.text,
    );
    if hovered(lay.close) {
        ctx.scene
            .fill_rounded_rect(lay.close, theme.radius.small, hover_fill(&theme));
    }
    paint_icon(
        ctx,
        IconName::Close,
        lay.close.deflate(Insets::splat(5.0)),
        colors.dim_text,
    );

    // Lista wtyczek.
    ctx.scene.push_clip(lay.list);
    if app.plugins.plugins.is_empty() {
        ctx.label_center(
            lay.list.center().x,
            lay.list.y() + 60.0,
            &tr("No plugins installed"),
            theme.typography.body.size,
            weight::NORMAL,
            colors.dim_text,
        );
    }
    for (index, (plugin, row)) in app.plugins.plugins.iter().zip(&lay.rows).enumerate() {
        if row.bottom() < lay.list.y() || row.y() > lay.list.bottom() {
            continue;
        }
        if index == app.plugin_ui.selected {
            ctx.scene
                .fill_rounded_rect(*row, theme.radius.small, colors.accent.scale_alpha(0.18));
        } else if hovered(*row) {
            ctx.scene
                .fill_rounded_rect(*row, theme.radius.small, hover_fill(&theme));
        }
        let name = elide(
            ctx,
            &plugin.name,
            row.width() - 24.0,
            theme.typography.body.size,
        );
        ctx.label_left(
            row.x() + 12.0,
            row.y() + 17.0,
            &name,
            theme.typography.body.size,
            weight::MEDIUM,
            colors.text,
        );
        let meta = format!("v{} · {}", plugin.version, plugin.author);
        let meta = elide(
            ctx,
            &meta,
            row.width() - 24.0,
            theme.typography.caption.size,
        );
        ctx.label_left(
            row.x() + 12.0,
            row.y() + 33.0,
            &meta,
            theme.typography.caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
    }
    ctx.scene.pop_clip();

    // Szczegóły zaznaczonej wtyczki.
    if let Some(plugin) = app.plugins.plugins.get(app.plugin_ui.selected) {
        ctx.label_left(
            lay.detail.x(),
            lay.detail.y() + 16.0,
            &plugin.name,
            theme.typography.title.size,
            weight::BOLD,
            colors.text,
        );
        let meta = format!("v{} · {}", plugin.version, plugin.author);
        ctx.label_left(
            lay.detail.x(),
            lay.detail.y() + 40.0,
            &elide(
                ctx,
                &meta,
                lay.detail.width(),
                theme.typography.caption.size,
            ),
            theme.typography.caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
        let description = elide(
            ctx,
            &plugin.description,
            lay.detail.width(),
            theme.typography.caption.size,
        );
        ctx.label_left(
            lay.detail.x(),
            lay.detail.y() + 58.0,
            &description,
            theme.typography.caption.size,
            weight::NORMAL,
            colors.dim_text,
        );

        let busy = app.plugins.busy();
        for (command, rect) in plugin.commands.iter().zip(&lay.commands) {
            let mut button = Button::new(&command.title);
            button.set_hovered(!busy && hovered(*rect));
            button.paint(ctx, *rect);
        }
        let mut settings = Button::new(tr("Settings"));
        settings.set_hovered(!busy && hovered(lay.settings));
        settings.paint(ctx, lay.settings);
        let mut remove = Button::new(tr("Remove")).variant(ButtonVariant::Destructive);
        remove.set_hovered(!busy && hovered(lay.remove));
        remove.paint(ctx, lay.remove);
    } else {
        ctx.label_left(
            lay.detail.x(),
            lay.detail.y() + 16.0,
            &tr("Select a plugin"),
            theme.typography.body.size,
            weight::NORMAL,
            colors.dim_text,
        );
    }

    // Pasek instalacji.
    let mut url = app.plugin_ui.url.clone();
    url.set_focused(app.plugin_ui.url.is_focused());
    url.paint(ctx, lay.url);
    let mut install = Button::new(tr("Install")).variant(ButtonVariant::Suggested);
    install.set_hovered(!app.plugins.busy() && hovered(lay.install));
    install.paint(ctx, lay.install);
    if let Some(status) = &app.plugin_ui.status {
        ctx.label_left(
            lay.status.x(),
            lay.status.center().y,
            &elide(
                ctx,
                status,
                lay.status.width(),
                theme.typography.caption.size,
            ),
            theme.typography.caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
    }
}

fn paint_form(app: &App, ctx: &mut PaintCtx, size: Size) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let Some(form) = app.plugin_ui.form.as_ref() else {
        return;
    };
    let Some(plugin) = app.plugins.plugins.get(form.plugin) else {
        return;
    };
    let Some(lay) = form_layout(app, size, &theme) else {
        return;
    };
    let pointer = app.plugin_ui.pointer;
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(lay.card, theme.radius.large, colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.card, theme.radius.large, 1.0, colors.border);
    let title = format!("{} — {}", tr("Plugin settings"), plugin.name);
    ctx.label_left(
        lay.card.x() + 18.0,
        lay.card.y() + 26.0,
        &elide(
            ctx,
            &title,
            lay.card.width() - 64.0,
            theme.typography.heading.size,
        ),
        theme.typography.heading.size,
        weight::BOLD,
        colors.text,
    );
    if pointer.is_some_and(|p| lay.close.contains(p)) {
        ctx.scene
            .fill_rounded_rect(lay.close, theme.radius.small, hover_fill(&theme));
    }
    paint_icon(
        ctx,
        IconName::Close,
        lay.close.deflate(Insets::splat(5.0)),
        colors.dim_text,
    );

    for (index, (setting, (label, control))) in plugin.settings.iter().zip(&lay.rows).enumerate() {
        ctx.label_left(
            label.x(),
            label.center().y,
            &elide(
                ctx,
                &setting.label,
                label.width(),
                theme.typography.body.size,
            ),
            theme.typography.body.size,
            weight::NORMAL,
            colors.text,
        );
        match &setting.kind {
            SettingKind::Text | SettingKind::Secret => {
                if let Some(Some(entry)) = form.entries.get(index) {
                    let mut entry = entry.clone();
                    entry.set_focused(index == form.active);
                    entry.paint(ctx, *control);
                }
            }
            SettingKind::Select(_) | SettingKind::Models => {
                let label = if form.loading.get(index).copied().unwrap_or(false) {
                    tr("Loading models…")
                } else if let Some(option) = form
                    .options
                    .get(index)
                    .and_then(|options| options.get(*form.selections.get(index).unwrap_or(&0)))
                {
                    option.clone()
                } else if form.dynamic.get(index).copied().unwrap_or(false) {
                    tr("Select model")
                } else {
                    tr("Select")
                };
                let mut button = Button::new(label).dropdown(true);
                button.set_hovered(pointer.is_some_and(|p| control.contains(p)));
                button.paint(ctx, *control);
            }
        }
    }

    if let Some(error) = &form.error {
        ctx.label_left(
            lay.card.x() + 18.0,
            lay.card.bottom() - 70.0,
            &elide(
                ctx,
                error,
                lay.card.width() - 36.0,
                theme.typography.caption.size,
            ),
            theme.typography.caption.size,
            weight::NORMAL,
            colors.warning,
        );
    }

    let mut save = Button::new(tr("Save")).variant(ButtonVariant::Suggested);
    save.set_hovered(pointer.is_some_and(|p| lay.save.contains(p)));
    save.paint(ctx, lay.save);
    let mut cancel = Button::new(tr("Cancel"));
    cancel.set_hovered(pointer.is_some_and(|p| lay.cancel.contains(p)));
    cancel.paint(ctx, lay.cancel);

    paint_picker(app, ctx, size);
}

struct PickerLayout {
    panel: Rect,
    search: Rect,
    list: Rect,
    rows: Vec<(usize, Rect)>,
    total: usize,
    visible: usize,
}

fn filtered_options(form: &Form, setting: usize, query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    form.options
        .get(setting)
        .map(|options| {
            options
                .iter()
                .enumerate()
                .filter(|(_, option)| query.is_empty() || option.to_lowercase().contains(&query))
                .map(|(index, _)| index)
                .collect::<Vec<usize>>()
        })
        .unwrap_or_default()
}

fn picker_layout(
    form: &Form,
    size: Size,
    _theme: &Theme,
    _text: &TextEngine,
) -> Option<PickerLayout> {
    let picker = form.picker.as_ref()?;
    let filtered = filtered_options(form, picker.setting, picker.query.text());
    let total = filtered.len();
    let visible = total.clamp(1, 10);
    let width = picker
        .anchor
        .width()
        .max(300.0)
        .min((size.width - 60.0).max(220.0));
    let height = 48.0 + visible as f64 * 26.0 + 8.0;
    let x = picker
        .anchor
        .x()
        .clamp(20.0, (size.width - 20.0 - width).max(20.0));
    let mut y = picker.anchor.bottom() + 4.0;
    if y + height > size.height - 20.0 {
        y = (picker.anchor.y() - height - 4.0).max(20.0);
    }
    let panel = Rect::from_xywh(x, y, width, height);
    let search = Rect::from_xywh(panel.x() + 8.0, panel.y() + 8.0, width - 16.0, 30.0);
    let list = Rect::from_xywh(
        panel.x() + 8.0,
        search.bottom() + 6.0,
        width - 16.0,
        visible as f64 * 26.0,
    );
    let start = (picker.scroll.floor() as usize).min(total.saturating_sub(visible));
    let mut rows = Vec::with_capacity(visible);
    for (offset, option) in filtered.iter().skip(start).take(visible).enumerate() {
        rows.push((
            *option,
            Rect::from_xywh(
                list.x(),
                list.y() + offset as f64 * 26.0,
                list.width(),
                26.0,
            ),
        ));
    }
    Some(PickerLayout {
        panel,
        search,
        list,
        rows,
        total,
        visible,
    })
}

fn paint_picker(app: &App, ctx: &mut PaintCtx, size: Size) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let Some(form) = app.plugin_ui.form.as_ref() else {
        return;
    };
    let Some(picker) = form.picker.as_ref() else {
        return;
    };
    let Some(lay) = picker_layout(form, size, &theme, ctx.text) else {
        return;
    };
    ctx.scene
        .fill_rounded_rect(lay.panel, theme.radius.medium, colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.panel, theme.radius.medium, 1.0, colors.border);
    let mut query = picker.query.clone();
    query.set_focused(true);
    query.paint(ctx, lay.search);

    ctx.scene.push_clip(lay.list);
    if lay.rows.is_empty() {
        ctx.label_center(
            lay.list.center().x,
            lay.list.y() + 13.0,
            &tr("No matches"),
            theme.typography.caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
    }
    for (index, row) in &lay.rows {
        if picker.hover == Some(*index) {
            ctx.scene
                .fill_rounded_rect(*row, theme.radius.small, hover_fill(&theme));
        }
        if form.selections.get(picker.setting) == Some(index) {
            ctx.scene
                .fill_rounded_rect(*row, theme.radius.small, colors.accent.scale_alpha(0.18));
        }
        let label = form
            .options
            .get(picker.setting)
            .and_then(|options| options.get(*index))
            .cloned()
            .unwrap_or_default();
        ctx.label_left(
            row.x() + 8.0,
            row.center().y,
            &elide(ctx, &label, row.width() - 16.0, theme.typography.body.size),
            theme.typography.body.size,
            weight::NORMAL,
            colors.text,
        );
    }
    ctx.scene.pop_clip();

    if lay.total > lay.visible {
        let track = Rect::from_xywh(
            lay.panel.right() - 4.0,
            lay.list.y(),
            2.0,
            lay.list.height(),
        );
        ctx.scene.fill_rounded_rect(track, 1.0, colors.shade);
        let fraction = lay.visible as f64 / lay.total as f64;
        let knob_h = (lay.list.height() * fraction).max(18.0);
        let max_scroll = (lay.total - lay.visible) as f64;
        let offset = if max_scroll > 0.0 {
            (picker.scroll / max_scroll).clamp(0.0, 1.0)
        } else {
            0.0
        };
        ctx.scene.fill_rounded_rect(
            Rect::from_xywh(
                track.x(),
                track.y() + (track.height() - knob_h) * offset,
                track.width(),
                knob_h,
            ),
            1.0,
            colors.dim_text.scale_alpha(0.7),
        );
    }
}

fn paint_prompt(app: &App, ctx: &mut PaintCtx, size: Size) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let Some(prompt) = app.plugin_ui.prompt.as_ref() else {
        return;
    };
    let Some(plugin) = app.plugins.plugins.get(prompt.plugin) else {
        return;
    };
    let Some(command) = plugin.commands.get(prompt.command) else {
        return;
    };
    let Some(lay) = prompt_layout(app, size, &theme) else {
        return;
    };
    let pointer = app.plugin_ui.pointer;
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(lay.card, theme.radius.large, colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.card, theme.radius.large, 1.0, colors.border);
    ctx.label_left(
        lay.card.x() + 18.0,
        lay.card.y() + 28.0,
        &elide(
            ctx,
            &command.title,
            lay.card.width() - 36.0,
            theme.typography.heading.size,
        ),
        theme.typography.heading.size,
        weight::BOLD,
        colors.text,
    );
    ctx.label_left(
        lay.card.x() + 18.0,
        lay.card.y() + 56.0,
        &tr("Prompt or instruction for the plugin"),
        theme.typography.caption.size,
        weight::NORMAL,
        colors.dim_text,
    );
    let mut entry = prompt.entry.clone();
    entry.set_focused(true);
    entry.paint(ctx, lay.entry);
    let mut run = Button::new(tr("Run")).variant(ButtonVariant::Suggested);
    run.set_hovered(pointer.is_some_and(|p| lay.run.contains(p)));
    run.paint(ctx, lay.run);
    let mut cancel = Button::new(tr("Cancel"));
    cancel.set_hovered(pointer.is_some_and(|p| lay.cancel.contains(p)));
    cancel.paint(ctx, lay.cancel);
}

/// Obsługa wejścia, gdy otwarte są okna wtyczek.
pub fn handle_events(app: &mut App, event: &Event, size: Size, theme: &Theme, text: &TextEngine) {
    if app.plugin_ui.form.is_some() {
        handle_form(app, event, size, theme, text);
    } else if app.plugin_ui.prompt.is_some() {
        handle_prompt(app, event, size, theme, text);
    } else if app.plugin_ui.open {
        handle_manager(app, event, size, theme, text);
    }
}

fn handle_manager(app: &mut App, event: &Event, size: Size, theme: &Theme, _text: &TextEngine) {
    let lay = manager_layout(app, size, theme, _text);
    match event {
        Event::PointerMotion { position, .. } => {
            app.plugin_ui.pointer = Some(*position);
        }
        Event::PointerLeave => app.plugin_ui.pointer = None,
        Event::Scroll(scroll) if lay.list.contains(scroll.position) => {
            let total = app.plugins.plugins.len() as f64 * (ROW_H + ROW_GAP);
            let max = (total - lay.list.height()).max(0.0);
            app.plugin_ui.scroll = (app.plugin_ui.scroll + scroll.dy).clamp(0.0, max);
        }
        Event::PointerButton(button)
            if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
        {
            let p = button.position;
            if lay.close.contains(p) || !lay.card.contains(p) {
                app.plugin_ui.open = false;
                app.plugin_ui.url.set_focused(false);
                return;
            }
            if lay.install.contains(p) {
                start_install(app);
                return;
            }
            if lay.url.contains(p) {
                app.plugin_ui.url.set_focused(true);
                return;
            }
            if let Some(index) = lay
                .rows
                .iter()
                .position(|row| row.contains(p))
                .filter(|index| *index < app.plugins.plugins.len())
            {
                app.plugin_ui.selected = index;
                return;
            }
            if app.plugins.busy() {
                return;
            }
            if let Some(index) = lay.commands.iter().position(|rect| rect.contains(p)) {
                run_command(app, index);
                return;
            }
            if lay.settings.contains(p) {
                open_form(app);
                return;
            }
            if lay.remove.contains(p) {
                remove_selected(app);
                return;
            }
        }
        Event::Key(key) if key.state == ButtonState::Pressed => {
            if app.plugin_ui.url.is_focused() {
                let command = clipboard_command(&mut app.plugin_ui.url, key);
                if let Some(command) = command {
                    match command {
                        TextClipboardCommand::Copy(Some(value))
                        | TextClipboardCommand::Cut(Some(value)) => {
                            app.clipboard_text = Some(value)
                        }
                        TextClipboardCommand::Paste => app.paste_requested = true,
                        _ => {}
                    }
                    return;
                }
                match key.keysym {
                    0xff1b => app.plugin_ui.url.set_focused(false),
                    0xff0d | 0xff8d => start_install(app),
                    0xff08 => app.plugin_ui.url.backspace(),
                    0xffff => app.plugin_ui.url.delete_forward(),
                    0xff51 => app.plugin_ui.url.move_left(),
                    0xff53 => app.plugin_ui.url.move_right(),
                    0xff50 => app.plugin_ui.url.home(),
                    0xff57 => app.plugin_ui.url.end(),
                    0x61 if key.modifiers.ctrl => app.plugin_ui.url.select_all(),
                    _ => insert_key_text(&mut app.plugin_ui.url, key),
                }
            } else if key.keysym == 0xff1b {
                app.plugin_ui.open = false;
            }
        }
        Event::Text(value) if app.plugin_ui.url.is_focused() => {
            app.plugin_ui.url.insert_str(&plain_text(value));
        }
        Event::Paste(value) if app.plugin_ui.url.is_focused() => {
            app.plugin_ui.url.insert_str(value);
        }
        _ => {}
    }
}

fn handle_form(app: &mut App, event: &Event, size: Size, theme: &Theme, text: &TextEngine) {
    // Popup wyboru ma pierwszeństwo nad resztą formularza.
    if app
        .plugin_ui
        .form
        .as_ref()
        .is_some_and(|form| form.picker.is_some())
    {
        handle_picker(app, event, size, theme, text);
        return;
    }
    // Gdy modele właśnie się pobrały, otwórz listę przy następnym zdarzeniu.
    open_pending_picker(app, size, theme);

    let Some(lay) = form_layout(app, size, theme) else {
        return;
    };
    let Some(plugin) = app
        .plugin_ui
        .form
        .as_ref()
        .and_then(|form| app.plugins.plugins.get(form.plugin).cloned())
    else {
        return;
    };
    match event {
        Event::PointerMotion { position, .. } => app.plugin_ui.pointer = Some(*position),
        Event::PointerLeave => app.plugin_ui.pointer = None,
        Event::PointerButton(button)
            if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
        {
            let p = button.position;
            if lay.close.contains(p) || !lay.card.contains(p) {
                close_form(app);
                return;
            }
            if lay.save.contains(p) {
                save_form(app);
                return;
            }
            if lay.cancel.contains(p) {
                close_form(app);
                return;
            }
            for (index, (_, control)) in lay.rows.iter().enumerate() {
                if !control.contains(p) {
                    continue;
                }
                match &plugin.settings[index].kind {
                    SettingKind::Text | SettingKind::Secret => {
                        if let Some(form) = app.plugin_ui.form.as_mut() {
                            form.active = index;
                            focus_entries(form);
                        }
                    }
                    SettingKind::Select(_) => open_picker(app, index, *control),
                    SettingKind::Models => {
                        let (loading, refresh) = app
                            .plugin_ui
                            .form
                            .as_ref()
                            .map(|form| {
                                (
                                    form.loading.get(index).copied().unwrap_or(false),
                                    form.needs_refresh.get(index).copied().unwrap_or(true)
                                        || form.options.get(index).map_or(true, Vec::is_empty),
                                )
                            })
                            .unwrap_or((false, true));
                        if loading {
                            return;
                        }
                        if refresh {
                            fetch_models(app, index);
                        } else {
                            open_picker(app, index, *control);
                        }
                    }
                }
                return;
            }
        }
        Event::Key(key) if key.state == ButtonState::Pressed => {
            if key.keysym == 0xff1b {
                close_form(app);
                return;
            }
            if key.keysym == 0xff0d || key.keysym == 0xff8d {
                save_form(app);
                return;
            }
            if key.keysym == 0xff09 {
                cycle_form(app, !key.modifiers.shift);
                return;
            }
            let Some(form) = app.plugin_ui.form.as_mut() else {
                return;
            };
            let Some(entry) = form.entries.get_mut(form.active).and_then(Option::as_mut) else {
                return;
            };
            let command = clipboard_command(entry, key);
            if let Some(command) = command {
                match command {
                    TextClipboardCommand::Copy(Some(value))
                    | TextClipboardCommand::Cut(Some(value)) => app.clipboard_text = Some(value),
                    TextClipboardCommand::Paste => app.paste_requested = true,
                    _ => {}
                }
                return;
            }
            let before = entry.text().len();
            match key.keysym {
                0xff08 => entry.backspace(),
                0xffff => entry.delete_forward(),
                0xff51 => entry.move_left(),
                0xff53 => entry.move_right(),
                0xff50 => entry.home(),
                0xff57 => entry.end(),
                0x61 if key.modifiers.ctrl => entry.select_all(),
                _ => insert_key_text(entry, key),
            }
            if entry.text().len() != before {
                mark_models_dirty(form);
            }
        }
        Event::Text(value) => {
            let plain = plain_text(value);
            if !plain.is_empty() {
                if let Some(form) = app.plugin_ui.form.as_mut() {
                    if let Some(entry) = form.entries.get_mut(form.active).and_then(Option::as_mut)
                    {
                        entry.insert_str(&plain);
                    }
                    mark_models_dirty(form);
                }
            }
        }
        Event::Paste(value) => {
            if let Some(form) = app.plugin_ui.form.as_mut() {
                if let Some(entry) = form.entries.get_mut(form.active).and_then(Option::as_mut) {
                    entry.insert_str(value);
                }
                mark_models_dirty(form);
            }
        }
        _ => {}
    }
}

fn mark_models_dirty(form: &mut Form) {
    for (index, dynamic) in form.dynamic.iter().enumerate() {
        if *dynamic {
            form.needs_refresh[index] = true;
            form.options[index].clear();
            form.selections[index] = 0;
        }
    }
}

fn form_values(plugin: &crate::plugins::Plugin, form: &Form) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for (index, setting) in plugin.settings.iter().enumerate() {
        let value = match &setting.kind {
            SettingKind::Select(_) | SettingKind::Models => form
                .options
                .get(index)
                .and_then(|options| options.get(*form.selections.get(index).unwrap_or(&0)))
                .cloned()
                .unwrap_or_else(|| setting.default.clone()),
            SettingKind::Text | SettingKind::Secret => form
                .entries
                .get(index)
                .and_then(Option::as_ref)
                .map(|entry| entry.text().to_owned())
                .unwrap_or_else(|| setting.default.clone()),
        };
        values.insert(setting.key.clone(), value);
    }
    values
}

fn fetch_models(app: &mut App, index: usize) {
    if app.plugins.busy() {
        return;
    }
    let Some((plugin, values, plugin_index)) = app.plugin_ui.form.as_ref().and_then(|form| {
        let plugin = app.plugins.plugins.get(form.plugin)?.clone();
        Some((plugin.clone(), form_values(&plugin, form), form.plugin))
    }) else {
        return;
    };
    if let Some(form) = app.plugin_ui.form.as_mut() {
        form.loading[index] = true;
        form.error = None;
        form.needs_refresh[index] = false;
        form.pending_open = Some(index);
    }
    let receiver = plugins::spawn_models(plugin, values);
    app.plugins.job = Some(Job::Models {
        receiver,
        plugin: plugin_index,
        setting: index,
    });
    app.status = Some(tr("Loading models…"));
}

fn open_picker(app: &mut App, setting: usize, anchor: Rect) {
    let Some(form) = app.plugin_ui.form.as_mut() else {
        return;
    };
    if form.options.get(setting).map_or(true, Vec::is_empty) {
        return;
    }
    let mut query = Entry::new(tr("Search…"));
    query.set_focused(true);
    form.picker = Some(PickerPopup {
        setting,
        anchor,
        query,
        scroll: 0.0,
        hover: None,
    });
    form.pending_open = None;
}

fn open_pending_picker(app: &mut App, size: Size, theme: &Theme) {
    let ready = app.plugin_ui.form.as_ref().and_then(|form| {
        let setting = form.pending_open?;
        if form.loading.get(setting).copied().unwrap_or(false)
            || form.options.get(setting).map_or(true, Vec::is_empty)
        {
            return None;
        }
        let lay = form_layout(app, size, theme)?;
        let anchor = lay.rows.get(setting)?.1;
        Some((setting, anchor))
    });
    let Some((setting, anchor)) = ready else {
        return;
    };
    open_picker(app, setting, anchor);
}

fn close_picker(app: &mut App) {
    if let Some(form) = app.plugin_ui.form.as_mut() {
        form.picker = None;
    }
}

fn select_picker(app: &mut App, index: usize) {
    let Some(form) = app.plugin_ui.form.as_mut() else {
        return;
    };
    let Some(setting) = form.picker.as_ref().map(|picker| picker.setting) else {
        return;
    };
    let is_select = app
        .plugins
        .plugins
        .get(form.plugin)
        .and_then(|plugin| plugin.settings.get(setting))
        .is_some_and(|setting| matches!(setting.kind, SettingKind::Select(_)));
    if let Some(selection) = form.selections.get_mut(setting) {
        *selection = index;
    }
    if let Some(refresh) = form.needs_refresh.get_mut(setting) {
        *refresh = false;
    }
    form.picker = None;
    if is_select {
        // Zmiana dostawcy unieważnia listę modeli pobraną od poprzedniego.
        mark_models_dirty(form);
    }
}

fn handle_picker(app: &mut App, event: &Event, size: Size, theme: &Theme, text: &TextEngine) {
    let Some(lay) = app
        .plugin_ui
        .form
        .as_ref()
        .and_then(|form| picker_layout(form, size, theme, text))
    else {
        close_picker(app);
        return;
    };
    match event {
        Event::PointerMotion { position, .. } => {
            app.plugin_ui.pointer = Some(*position);
            let hover = lay
                .rows
                .iter()
                .find(|(_, row)| row.contains(*position))
                .map(|(index, _)| *index);
            if let Some(form) = app.plugin_ui.form.as_mut() {
                if let Some(picker) = form.picker.as_mut() {
                    picker.hover = hover;
                }
            }
        }
        Event::PointerLeave => {
            app.plugin_ui.pointer = None;
            if let Some(form) = app.plugin_ui.form.as_mut() {
                if let Some(picker) = form.picker.as_mut() {
                    picker.hover = None;
                }
            }
        }
        Event::PointerButton(button)
            if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
        {
            let p = button.position;
            if lay.search.contains(p) {
                return;
            }
            if lay.panel.contains(p) {
                if let Some((index, _)) = lay.rows.iter().find(|(_, row)| row.contains(p)) {
                    select_picker(app, *index);
                }
            } else {
                close_picker(app);
            }
        }
        Event::Scroll(scroll) if lay.panel.contains(scroll.position) => {
            let max = lay.total.saturating_sub(lay.visible) as f64;
            if let Some(form) = app.plugin_ui.form.as_mut() {
                if let Some(picker) = form.picker.as_mut() {
                    picker.scroll = (picker.scroll + scroll.dy / 26.0).clamp(0.0, max);
                }
            }
        }
        Event::Key(key) if key.state == ButtonState::Pressed => {
            match key.keysym {
                0xff1b => {
                    close_picker(app);
                    return;
                }
                0xff0d | 0xff8d => {
                    let index = app.plugin_ui.form.as_ref().and_then(|form| {
                        let picker = form.picker.as_ref()?;
                        picker
                            .hover
                            .or_else(|| lay.rows.first().map(|(index, _)| *index))
                    });
                    if let Some(index) = index {
                        select_picker(app, index);
                    }
                    return;
                }
                0xff52 | 0xff54 => {
                    move_picker(app, key.keysym == 0xff54, lay.visible);
                    return;
                }
                _ => {}
            }
            let Some(form) = app.plugin_ui.form.as_mut() else {
                return;
            };
            let Some(picker) = form.picker.as_mut() else {
                return;
            };
            let command = clipboard_command(&mut picker.query, key);
            if let Some(command) = command {
                match command {
                    TextClipboardCommand::Copy(Some(value))
                    | TextClipboardCommand::Cut(Some(value)) => app.clipboard_text = Some(value),
                    TextClipboardCommand::Paste => app.paste_requested = true,
                    _ => {}
                }
                return;
            }
            let before = picker.query.text().len();
            match key.keysym {
                0xff08 => picker.query.backspace(),
                0xffff => picker.query.delete_forward(),
                0xff51 => picker.query.move_left(),
                0xff53 => picker.query.move_right(),
                0xff50 => picker.query.home(),
                0xff57 => picker.query.end(),
                0x61 if key.modifiers.ctrl => picker.query.select_all(),
                _ => insert_key_text(&mut picker.query, key),
            }
            if picker.query.text().len() != before {
                picker.scroll = 0.0;
                picker.hover = None;
            }
        }
        Event::Text(value) => {
            let plain = plain_text(value);
            if !plain.is_empty() {
                if let Some(form) = app.plugin_ui.form.as_mut() {
                    if let Some(picker) = form.picker.as_mut() {
                        picker.query.insert_str(&plain);
                        picker.scroll = 0.0;
                        picker.hover = None;
                    }
                }
            }
        }
        Event::Paste(value) => {
            if let Some(form) = app.plugin_ui.form.as_mut() {
                if let Some(picker) = form.picker.as_mut() {
                    picker.query.insert_str(value);
                    picker.scroll = 0.0;
                    picker.hover = None;
                }
            }
        }
        _ => {}
    }
}

fn move_picker(app: &mut App, forward: bool, visible: usize) {
    let Some((filtered, current)) = app.plugin_ui.form.as_ref().and_then(|form| {
        let picker = form.picker.as_ref()?;
        let filtered = filtered_options(form, picker.setting, picker.query.text());
        let current = picker
            .hover
            .and_then(|hover| filtered.iter().position(|index| *index == hover))
            .unwrap_or(0);
        Some((filtered, current))
    }) else {
        return;
    };
    if filtered.is_empty() {
        return;
    }
    let next = if forward {
        (current + 1).min(filtered.len() - 1)
    } else {
        current.saturating_sub(1)
    };
    let Some(form) = app.plugin_ui.form.as_mut() else {
        return;
    };
    let Some(picker) = form.picker.as_mut() else {
        return;
    };
    picker.hover = Some(filtered[next]);
    let start = picker.scroll as usize;
    if next < start {
        picker.scroll = next as f64;
    } else if next >= start + visible {
        picker.scroll = (next + 1 - visible) as f64;
    }
}

fn handle_prompt(app: &mut App, event: &Event, size: Size, theme: &Theme, _text: &TextEngine) {
    let Some(lay) = prompt_layout(app, size, theme) else {
        return;
    };
    match event {
        Event::PointerMotion { position, .. } => app.plugin_ui.pointer = Some(*position),
        Event::PointerLeave => app.plugin_ui.pointer = None,
        Event::PointerButton(button)
            if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
        {
            let p = button.position;
            if lay.run.contains(p) {
                submit_prompt(app);
            } else if lay.cancel.contains(p) || !lay.card.contains(p) {
                app.plugin_ui.prompt = None;
            } else if lay.entry.contains(p) {
                // Pole jest zawsze aktywne — klik tylko ustawia kursor na końcu.
                if let Some(prompt) = app.plugin_ui.prompt.as_mut() {
                    prompt.entry.set_focused(true);
                }
            }
        }
        Event::Key(key) if key.state == ButtonState::Pressed => {
            if key.keysym == 0xff1b {
                app.plugin_ui.prompt = None;
                return;
            }
            if key.keysym == 0xff0d || key.keysym == 0xff8d {
                submit_prompt(app);
                return;
            }
            let Some(prompt) = app.plugin_ui.prompt.as_mut() else {
                return;
            };
            let command = clipboard_command(&mut prompt.entry, key);
            if let Some(command) = command {
                match command {
                    TextClipboardCommand::Copy(Some(value))
                    | TextClipboardCommand::Cut(Some(value)) => app.clipboard_text = Some(value),
                    TextClipboardCommand::Paste => app.paste_requested = true,
                    _ => {}
                }
                return;
            }
            match key.keysym {
                0xff08 => prompt.entry.backspace(),
                0xffff => prompt.entry.delete_forward(),
                0xff51 => prompt.entry.move_left(),
                0xff53 => prompt.entry.move_right(),
                0xff50 => prompt.entry.home(),
                0xff57 => prompt.entry.end(),
                0x61 if key.modifiers.ctrl => prompt.entry.select_all(),
                _ => insert_key_text(&mut prompt.entry, key),
            }
        }
        Event::Text(value) => {
            if let Some(prompt) = app.plugin_ui.prompt.as_mut() {
                prompt.entry.insert_str(&plain_text(value));
            }
        }
        Event::Paste(value) => {
            if let Some(prompt) = app.plugin_ui.prompt.as_mut() {
                prompt.entry.insert_str(value);
            }
        }
        _ => {}
    }
}

fn insert_key_text(entry: &mut Entry, key: &KeyEvent) {
    if key.modifiers.ctrl || key.modifiers.alt {
        return;
    }
    if let Some(value) = &key.text {
        let plain = plain_text(value);
        if !plain.is_empty() {
            entry.insert_str(&plain);
        }
    }
}

fn plain_text(value: &str) -> String {
    value.chars().filter(|ch| !ch.is_control()).collect()
}

fn start_install(app: &mut App) {
    if app.plugins.busy() {
        return;
    }
    let url = app.plugin_ui.url.text().trim().to_owned();
    if url.is_empty() {
        app.plugin_ui.status = Some(tr("Enter a plugin URL"));
        return;
    }
    app.plugin_ui.status = Some(tr("Installing…"));
    app.plugins.job = Some(Job::Install(plugins::spawn_install(url)));
}

fn remove_selected(app: &mut App) {
    let Some(plugin) = app.plugins.plugins.get(app.plugin_ui.selected).cloned() else {
        return;
    };
    match plugins::remove(&plugin) {
        Ok(()) => {
            app.plugin_ui.status = Some(format!("{}: {}", plugin.name, tr("Removed")));
            app.plugins.reload();
            app.rebuild_menu();
            if app.plugin_ui.selected >= app.plugins.plugins.len() {
                app.plugin_ui.selected = app.plugins.plugins.len().saturating_sub(1);
            }
        }
        Err(error) => app.show_error(error),
    }
}

fn open_form(app: &mut App) {
    let Some(plugin) = app.plugins.plugins.get(app.plugin_ui.selected) else {
        return;
    };
    let values = plugin.values();
    let count = plugin.settings.len();
    let mut entries = Vec::with_capacity(count);
    let mut selections = vec![0; count];
    let mut options = vec![Vec::new(); count];
    let mut dynamic = vec![false; count];
    let mut needs_refresh = vec![false; count];
    let mut active = None;
    for (index, setting) in plugin.settings.iter().enumerate() {
        let value = values.get(&setting.key).cloned().unwrap_or_default();
        match &setting.kind {
            SettingKind::Select(choices) => {
                entries.push(None);
                options[index] = choices.clone();
                selections[index] = choices
                    .iter()
                    .position(|option| option == &value)
                    .unwrap_or(0);
            }
            SettingKind::Models => {
                entries.push(None);
                options[index] = if value.is_empty() {
                    Vec::new()
                } else {
                    vec![value]
                };
                dynamic[index] = true;
                needs_refresh[index] = true;
            }
            SettingKind::Text | SettingKind::Secret => {
                let mut entry = Entry::new(setting.label.clone());
                if setting.kind == SettingKind::Secret {
                    entry = entry.masked('•');
                }
                entry.set_text(value);
                entries.push(Some(entry));
                if active.is_none() {
                    active = Some(index);
                }
            }
        }
    }
    let active = active.unwrap_or(0);
    let mut form = Form {
        plugin: app.plugin_ui.selected,
        entries,
        selections,
        active,
        options,
        dynamic,
        loading: vec![false; count],
        needs_refresh,
        error: None,
        picker: None,
        pending_open: None,
    };
    focus_entries(&mut form);
    app.plugin_ui.form = Some(form);
}

fn focus_entries(form: &mut Form) {
    for (index, entry) in form.entries.iter_mut().enumerate() {
        if let Some(entry) = entry {
            entry.set_focused(index == form.active);
        }
    }
}

fn cycle_form(app: &mut App, forward: bool) {
    let Some(form) = app.plugin_ui.form.as_mut() else {
        return;
    };
    let count = form.entries.len();
    if count == 0 {
        return;
    }
    for step in 1..=count {
        let index = if forward {
            (form.active + step) % count
        } else {
            (form.active + count - step) % count
        };
        if form.entries.get(index).is_some_and(|entry| entry.is_some()) {
            form.active = index;
            focus_entries(form);
            return;
        }
    }
}

fn save_form(app: &mut App) {
    let Some(form) = app.plugin_ui.form.as_ref() else {
        return;
    };
    let Some(plugin) = app.plugins.plugins.get(form.plugin).cloned() else {
        close_form(app);
        return;
    };
    let values = form_values(&plugin, form);
    match plugin.save_values(&values) {
        Ok(()) => {
            app.plugin_ui.status = Some(format!("{}: {}", plugin.name, tr("Settings saved")));
            close_form(app);
        }
        Err(error) => app.show_error(error),
    }
}

fn close_form(app: &mut App) {
    if let Some(form) = app.plugin_ui.form.as_mut() {
        for entry in form.entries.iter_mut().flatten() {
            entry.set_focused(false);
        }
    }
    app.plugin_ui.form = None;
}

fn run_command(app: &mut App, command_index: usize) {
    start(app, app.plugin_ui.selected, command_index);
}

/// Uruchamia polecenie wtyczki (z pytaniem o instrukcję, jeśli manifest tego wymaga).
pub fn start(app: &mut App, plugin: usize, command: usize) {
    let Some(spec) = app
        .plugins
        .plugins
        .get(plugin)
        .and_then(|plugin| plugin.commands.get(command))
        .cloned()
    else {
        return;
    };
    if spec.prompt {
        let mut entry = Entry::new(tr("Write an instruction…"));
        entry.set_focused(true);
        app.plugin_ui.open = false;
        app.plugin_ui.prompt = Some(Prompt {
            plugin,
            command,
            entry,
        });
    } else {
        app.start_plugin(plugin, command, String::new());
    }
}

fn submit_prompt(app: &mut App) {
    let Some(prompt) = app.plugin_ui.prompt.take() else {
        return;
    };
    let instruction = prompt.entry.text().to_owned();
    app.start_plugin(prompt.plugin, prompt.command, instruction);
}

/// Uruchamia polecenie wtyczki (wywoływane też przez testy i menu).
pub fn execute(app: &mut App, plugin: usize, command: usize, instruction: String) {
    app.start_plugin(plugin, command, instruction);
}

fn elide(ctx: &PaintCtx, text: &str, width: f64, size: f64) -> String {
    if width <= 0.0 || ctx.measure(text, size, weight::NORMAL) <= width {
        return text.to_owned();
    }
    let dots = "…";
    let dots_w = ctx.measure(dots, size, weight::NORMAL);
    let mut out = String::new();
    for ch in text.chars() {
        let mut probe = out.clone();
        probe.push(ch);
        if ctx.measure(&probe, size, weight::NORMAL) + dots_w > width {
            break;
        }
        out = probe;
    }
    out.push('…');
    out
}

// Uzupełnienie brakujących elementów kontekstu przy uruchamianiu.
pub(crate) fn payload_for(
    app: &App,
    context: Context,
) -> (Option<std::path::PathBuf>, String, String) {
    let doc = app.doc();
    let payload = match context {
        Context::Document => doc.view.text().to_owned(),
        Context::Selection => doc
            .view
            .selected_text()
            .unwrap_or_else(|| doc.view.text())
            .to_owned(),
        Context::None => String::new(),
    };
    (doc.path.clone(), doc.language.name().to_owned(), payload)
}

/// Otwiera popup modeli, gdy lista właśnie się wczytała (wołane co klatkę).
pub fn tick(app: &mut App, size: Size, theme: &Theme) {
    open_pending_picker(app, size, theme);
}

/// Czy któryś z dialogów wtyczek jest otwarty (blokuje edytor).
pub fn is_open(app: &App) -> bool {
    app.plugin_ui.open || app.plugin_ui.prompt.is_some() || app.plugin_ui.form.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::{Context, Output, Plugin, PluginCommand};
    use libluna_core::{ButtonState, KeyEvent, Modifiers};
    use libluna_render::Scene;

    fn demo() -> Plugin {
        Plugin {
            dir: std::path::PathBuf::from("/tmp/demo"),
            id: "demo".into(),
            name: "Demo".into(),
            version: "1.0".into(),
            author: "Luna".into(),
            description: "Demo plugin".into(),
            entry: "run.sh".into(),
            interpreter: Some("sh".into()),
            commands: vec![PluginCommand {
                id: "run".into(),
                title: "Uruchom".into(),
                prompt: false,
                context: Context::Document,
                output: Output::Insert,
            }],
            settings: Vec::new(),
            chat: false,
        }
    }

    fn key(keysym: u32) -> Event {
        Event::Key(KeyEvent {
            keysym,
            keycode: 0,
            text: None,
            state: ButtonState::Pressed,
            modifiers: Modifiers::NONE,
            repeat: false,
        })
    }

    #[test]
    fn manager_paints_empty_and_filled_and_closes_with_escape() {
        let theme = Theme::light();
        let text = TextEngine::none();
        let size = Size::new(1040.0, 740.0);
        let mut app = App::new();
        app.plugins.plugins.clear();
        app.open_plugins();
        assert!(app.plugin_ui.open);
        let mut scene = Scene::new();
        let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
        paint(&app, &mut ctx, size);

        app.plugins.plugins = vec![demo()];
        let mut scene = Scene::new();
        let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
        paint(&app, &mut ctx, size);

        handle_events(&mut app, &key(0xff1b), size, &theme, &text);
        assert!(!app.plugin_ui.open);
    }

    #[test]
    fn models_setting_starts_with_default_and_marks_refresh() {
        let mut app = App::new();
        let mut plugin = demo();
        plugin.settings = vec![crate::plugins::PluginSetting {
            key: "model".into(),
            label: "Model".into(),
            kind: SettingKind::Models,
            default: "gemini-2.0-flash".into(),
        }];
        app.plugins.plugins = vec![plugin];
        app.plugin_ui.selected = 0;
        open_form(&mut app);
        let form = app.plugin_ui.form.as_ref().unwrap();
        assert!(form.dynamic[0]);
        assert!(form.needs_refresh[0]);
        assert_eq!(form.options[0], vec!["gemini-2.0-flash"]);
        assert!(form.entries[0].is_none());
    }

    #[test]
    fn form_uses_manifest_defaults() {
        let mut app = App::new();
        let mut plugin = demo();
        plugin.settings = vec![
            crate::plugins::PluginSetting {
                key: "provider".into(),
                label: "Provider".into(),
                kind: SettingKind::Select(vec!["a".into(), "b".into()]),
                default: "b".into(),
            },
            crate::plugins::PluginSetting {
                key: "key".into(),
                label: "Key".into(),
                kind: SettingKind::Secret,
                default: String::new(),
            },
        ];
        app.plugins.plugins = vec![plugin];
        app.plugin_ui.selected = 0;
        open_form(&mut app);
        let form = app.plugin_ui.form.as_ref().unwrap();
        assert_eq!(form.selections[0], 1);
        assert!(form.entries[0].is_none());
        assert!(form.entries[1].is_some());
        assert_eq!(form.active, 1);
    }
}
