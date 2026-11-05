//! Renderuje Luna Editor do PNG — podgląd bez kompozytora Wayland.
//!
//! Użycie:
//!   `cargo run --bin editor-render -- [--dark] [--lang CODE] [--file PATH] [--find QUERY] [--about] [--no-workspace] [--tree-menu] [--prompt new-file|new-folder|rename] [--settings] [--plugins] [wyjście.png]`

use std::error::Error;
use std::path::PathBuf;

use libluna_core::{Accent, Size, ThemePreference};
use libluna_i18n::{set_locale, Locale};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use luna_editor::app::{App, PromptKind};
use luna_editor::ui;

const SAMPLE: &str = r#"// Luna Editor — podświetlanie składni bez GTK
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct Document {
    name: String,
    lines: Vec<String>,
}

impl Document {
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            lines: Vec::new(),
        }
    }

    fn word_count(&self) -> usize {
        self.lines
            .iter()
            .flat_map(|line| line.split_whitespace())
            .count()
    }
}

fn main() {
    let mut doc = Document::new("notatki.md");
    doc.lines.push("Witaj w Lunie!".to_owned());
    println!("{}: {} słów", doc.name, doc.word_count());
}
"#;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut dark = false;
    let mut lang = "pl".to_owned();
    let mut file: Option<PathBuf> = None;
    let mut folder: Option<PathBuf> = None;
    let mut preview: Option<PathBuf> = None;
    let mut browser_mode: Option<String> = None;
    let mut find: Option<String> = None;
    let mut about = false;
    let mut show_workspace = true;
    let mut tree_menu = false;
    let mut prompt: Option<String> = None;
    let mut settings = false;
    let mut plugins_open = false;
    let mut out: Option<String> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--dark" => {
                dark = true;
                index += 1;
            }
            "--lang" => {
                if let Some(value) = args.get(index + 1) {
                    lang = value.clone();
                }
                index += 2;
            }
            "--file" => {
                file = args.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            "--folder" => {
                folder = args.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            "--preview" => {
                preview = args.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            "--browser" => {
                browser_mode = args.get(index + 1).cloned();
                index += 2;
            }
            "--find" => {
                find = args.get(index + 1).cloned();
                index += 2;
            }
            "--about" => {
                about = true;
                index += 1;
            }
            "--no-workspace" => {
                show_workspace = false;
                index += 1;
            }
            "--tree-menu" => {
                tree_menu = true;
                index += 1;
            }
            "--settings" => {
                settings = true;
                index += 1;
            }
            "--plugins" => {
                plugins_open = true;
                index += 1;
            }
            "--prompt" => {
                prompt = args.get(index + 1).cloned();
                index += 2;
            }
            other => {
                out = Some(other.to_owned());
                index += 1;
            }
        }
    }

    let out = out.unwrap_or_else(|| {
        if dark {
            "luna-editor-dark.png".to_owned()
        } else {
            "luna-editor.png".to_owned()
        }
    });

    set_locale(Locale::new(&lang));
    libluna_i18n::load_catalogs_from_dirs();
    let preference = if dark {
        ThemePreference::Dark
    } else {
        ThemePreference::Light
    };
    let theme = preference.resolve(false, Accent::Blue);

    let (path, text) = match &file {
        Some(path) => (
            Some(path.clone()),
            std::fs::read_to_string(path).unwrap_or_else(|_| SAMPLE.to_owned()),
        ),
        None => (Some(PathBuf::from("example.rs")), SAMPLE.to_owned()),
    };
    let mut app = App::with_text(path, &text);
    app.view_mut().set_focused(true);
    app.view_mut().set_cursor(0);
    if !show_workspace {
        app.workspace.visible = false;
    }
    if let Some(folder) = folder {
        app.workspace.set_root(folder);
    }
    if let Some(preview) = preview {
        let path = if preview.is_absolute() {
            preview
        } else {
            app.workspace.root.join(preview)
        };
        app.workspace.reveal(&path);
    }
    match browser_mode.as_deref() {
        Some("save") => app.open_save_browser(),
        Some("folder") => app.open_folder_browser(),
        Some("open") => app.open_open_browser(),
        _ => {}
    }
    if let Some(query) = find {
        app.toggle_find();
        if let Some(find) = app.find.as_mut() {
            find.entry.set_text(query);
        }
        app.refresh_find();
    }
    if about {
        app.show_about();
    }
    let size = Size::new(1040.0, 740.0);
    if tree_menu {
        let lay = ui::layout(size, &theme, app.workspace.visible, false);
        let index = app
            .workspace
            .selected_index()
            .or_else(|| (!app.workspace.rows.is_empty()).then_some(0));
        if let Some(index) = index {
            if let Some(anchor) = ui::workspace_row_anchor(&app, &lay, index) {
                app.open_tree_menu(index, anchor);
            }
        }
    }
    if let Some(kind) = prompt.as_deref() {
        let kind = match kind {
            "new-folder" => PromptKind::NewFolder,
            "rename" => PromptKind::Rename,
            _ => PromptKind::NewFile,
        };
        let target = if kind == PromptKind::Rename {
            app.workspace
                .selected
                .clone()
                .unwrap_or_else(|| app.workspace.root.join("nowy.txt"))
        } else {
            app.workspace.target_dir()
        };
        app.begin_prompt(kind, target);
    }
    if settings {
        app.open_settings();
    }
    if plugins_open {
        app.open_plugins();
    }

    let text_engine = TextEngine::mono();
    let mut scene = Scene::new();
    ui::paint(&mut app, &mut scene, size, &theme, &text_engine);

    let mut renderer = SoftwareRenderer::new(1040, 740);
    renderer.draw(&scene, 1.0, theme.colors.window_bg);
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write(&out, png)?;
    println!("saved {out} (1040x740, lang={lang})");
    Ok(())
}
