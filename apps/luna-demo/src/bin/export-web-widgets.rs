//! Eksport biblioteki widgetów dla strony projektu.
//!
//! Renderuje każdy wpis z katalogu galerii (`gallery_catalog::DEMOS`) do PNG
//! w dwóch wariantach motywu i zapisuje metadane (nazwa, kategoria, opis,
//! przykład kodu) jako `widgets.js` dla statycznej strony.
//!
//! Uruchom z katalogu głównego repozytorium:
//!
//! ```sh
//! cargo run --manifest-path apps/luna-demo/Cargo.toml --bin export-web-widgets
//! ```
//!
//! Opcjonalnie wskaż katalog wyjściowy:
//!
//! ```sh
//! cargo run --manifest-path apps/luna-demo/Cargo.toml --bin export-web-widgets -- /tmp/web-assets
//! ```

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use libluna_core::{Rect, Size, Theme};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use libluna_widgets::PaintCtx;
use luna_demo::gallery_catalog::DEMOS;

/// Szerokość podglądu w pikselach fizycznych.
const W: u32 = 480;
/// Wysokość podglądu w pikselach fizycznych.
const H: u32 = 280;

fn main() -> Result<(), Box<dyn Error>> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."));
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root.join("web/luna.paganlinux.eu/assets"));

    fs::create_dir_all(out.join("widgets/light"))?;
    fs::create_dir_all(out.join("widgets/dark"))?;

    let text = TextEngine::system();
    let size = Size::new(W as f64, H as f64);
    let rect = Rect::from_size(size);

    for demo in DEMOS {
        let slug = slugify(demo.name);
        for (dark, dir) in [(false, "light"), (true, "dark")] {
            let theme = if dark { Theme::dark() } else { Theme::light() };
            let mut scene = Scene::new();
            {
                let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
                (demo.preview)(&mut ctx, rect);
            }

            let mut renderer = SoftwareRenderer::new(W, H);
            renderer.draw(&scene, 1.0, theme.colors.card_bg);
            let png = renderer
                .encode_png()
                .ok_or("nie udało się zakodować podglądu do PNG")?;
            write_if_changed(
                &out.join("widgets").join(dir).join(format!("{slug}.png")),
                &png,
            )?;
        }
    }

    let js = widget_data_js();
    write_if_changed(&out.join("widgets.js"), js.as_bytes())?;

    println!(
        "wyeksportowano {} widgetów do {} (jasny + ciemny)",
        DEMOS.len(),
        out.display()
    );
    Ok(())
}

/// Buduje `widgets.js` — dane konsumowane przez `/docs/widgets.html`.
fn widget_data_js() -> String {
    let mut out = String::from(
        "/* Wygenerowane przez apps/luna-demo/src/bin/export-web-widgets.rs — nie edytuj ręcznie. */\n\
         window.LUNA_WIDGETS = [\n",
    );

    for demo in DEMOS {
        out.push_str("  {\n");
        out.push_str(&format!("    slug: {},\n", json(&slugify(demo.name))));
        out.push_str(&format!("    name: {},\n", json(demo.name)));
        out.push_str(&format!("    category: {},\n", json(demo.category.label())));
        out.push_str(&format!("    description: {},\n", json(demo.description)));
        out.push_str(&format!("    source: {},\n", json(demo.source)));
        out.push_str(&format!("    code: {},\n", json(demo.code)));
        out.push_str("  },\n");
    }

    out.push_str("];\n");
    out
}

fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn json(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn write_if_changed(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Ok(existing) = fs::read(path) {
        if existing == bytes {
            return Ok(());
        }
    }
    fs::write(path, bytes)
}
