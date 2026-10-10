//! Renderuje scenę demo do pliku PNG — pozwala obejrzeć wynik bez kompozytora.
//!
//! Użycie:
//!   `cargo run -p luna-demo --bin render -- [--dark] [--accent NAME] [--lang CODE] [--theme NAME|PATH] [wyjście.png]`
//!
//! Przykłady:
//!   `render --lang pl docs/images/settings-light.png`
//!   `render --dark --accent purple --lang de docs/images/settings-dark.png`
//!   `render --theme examples/luna/themes/nord.theme docs/images/settings-nord.png`

use std::error::Error;

use libluna_core::{Accent, Size, ThemePreference};
use libluna_i18n::{set_locale, Locale};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use luna_demo::ui::{self, Demo};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut dark = false;
    let mut lang = "pl".to_owned();
    let mut accent = Accent::Blue;
    let mut theme_spec: Option<String> = None;
    let mut no_bottom_bar = false;
    let mut no_sidebar = false;
    let mut sidebar_width: Option<f64> = None;
    let mut out: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dark" => {
                dark = true;
                i += 1;
            }
            "--no-bottom-bar" => {
                no_bottom_bar = true;
                i += 1;
            }
            "--no-sidebar" => {
                no_sidebar = true;
                i += 1;
            }
            "--sidebar-width" => {
                if let Some(v) = args.get(i + 1).and_then(|s| s.parse::<f64>().ok()) {
                    sidebar_width = Some(v);
                }
                i += 2;
            }
            "--lang" => {
                if let Some(v) = args.get(i + 1) {
                    lang = v.clone();
                }
                i += 2;
            }
            "--accent" => {
                if let Some(v) = args.get(i + 1) {
                    accent = Accent::from_name(v).unwrap_or(Accent::Blue);
                }
                i += 2;
            }
            "--theme" => {
                if let Some(v) = args.get(i + 1) {
                    theme_spec = Some(v.clone());
                }
                i += 2;
            }
            other => {
                out = Some(other.to_owned());
                i += 1;
            }
        }
    }

    let out = out.unwrap_or_else(|| {
        if dark {
            "luna-demo-dark.png".to_owned()
        } else {
            "luna-demo.png".to_owned()
        }
    });

    set_locale(Locale::new(&lang));
    // Doładuj języki ze standardowych katalogów (nadpisują wbudowane).
    libluna_i18n::load_catalogs_from_dirs();

    let preference = if dark {
        ThemePreference::Dark
    } else {
        ThemePreference::Light
    };
    let theme = preference.resolve(false, accent);
    let theme = match theme_spec.as_deref() {
        Some(spec) => match libluna_core::theme_file::load_spec(spec) {
            Some(patch) => patch.build(theme),
            None => {
                eprintln!("theme not found: {spec}");
                theme
            }
        },
        None => theme,
    };

    let (w, h) = (900u32, 1028u32);
    let size = Size::new(w as f64, h as f64);

    let text = TextEngine::system();
    let mut scene = Scene::new();
    let demo = Demo {
        show_bottom_bar: !no_bottom_bar,
        sidebar_visible: !no_sidebar,
        sidebar_width: sidebar_width.unwrap_or(luna_demo::ui::DEFAULT_SIDEBAR_W),
        ..Demo::default()
    };
    ui::build_scene(&mut scene, &theme, &text, size, &demo, preference, accent);

    let mut renderer = SoftwareRenderer::new(w, h);
    renderer.draw(&scene, 1.0, theme.colors.window_bg);

    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write(&out, png)?;
    println!(
        "saved {out} ({w}x{h}, lang={lang}, accent={})",
        accent.name()
    );
    Ok(())
}
