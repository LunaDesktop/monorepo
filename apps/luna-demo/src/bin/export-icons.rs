//! Eksport pastelowego zestawu `luna-icons` do formatu freedesktop (PNG + SVG).
//!
//! Ikony w środowisku są rysowane wektorowo, ale ten sam kod można wyeksportować
//! do prawdziwego pakietu ikon, którego użyją też aplikacje GTK/Qt/KDE:
//!
//! ```sh
//! cargo run --manifest-path apps/luna-demo/Cargo.toml --bin export-icons
//! # → assets/icons/Luna-Icons/{16x16,…,512x512}/{apps,places,…}/*.png
//! # → assets/icons/Luna-Icons/scalable/{apps,places,…}/*.svg
//! ```
//!
//! SVG powstaje z tej samej sceny co PNG, więc paski aplikacji, launcher
//! i ikony systemowe mają identyczne kształty oraz paletę.
//! Aliasy (np. `x-office-document` → `text-x-generic`) są dowiązaniami
//! symbolicznymi, więc pakiet nie powiela tych samych plików.

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use libluna_core::{Color, IconTheme, Rect, Theme};
use libluna_render::{scene_to_svg, Scene, SoftwareRenderer, TextEngine};
use libluna_widgets::{icon, paint_icon, IconName, PaintCtx};

/// Rozmiary katalogów zgodne z konwencją freedesktop.
const SIZES: [u32; 8] = [16, 24, 32, 48, 64, 128, 256, 512];

/// Nominalny rozmiar siatki ikon wektorowych (wartość `viewBox` w SVG).
const SVG_SIZE: u32 = 24;

/// Co narysować dla danej pozycji pakietu.
#[derive(Clone, Copy)]
enum Art {
    Icon(IconName),
    Folder,
}

/// Jedna ikona kanoniczna + aliasy w innych katalogach/kontekstach.
struct Export {
    art: Art,
    context: &'static str,
    name: &'static str,
    /// `(kontekst, nazwa)` — dowiązania do ikony kanonicznej.
    aliases: &'static [(&'static str, &'static str)],
}

/// Zakres zestawu: aplikacje, miejsca, typy plików, urządzenia i status.
///
/// Nazwy pochodzą ze specyfikacji freedesktop i pokrywają się z tym, czego
/// szuka `libluna-widgets::paint_icon` oraz typowe aplikacje GTK/Qt.
const EXPORTS: &[Export] = &[
    Export {
        art: Art::Icon(IconName::Files),
        context: "apps",
        name: "system-file-manager",
        aliases: &[],
    },
    Export {
        art: Art::Folder,
        context: "places",
        name: "folder",
        aliases: &[("places", "inode-directory")],
    },
    Export {
        art: Art::Icon(IconName::Home),
        context: "places",
        name: "user-home",
        aliases: &[("places", "folder-home")],
    },
    Export {
        art: Art::Icon(IconName::Settings),
        context: "apps",
        name: "preferences-system",
        aliases: &[("categories", "preferences-desktop")],
    },
    Export {
        art: Art::Icon(IconName::Terminal),
        context: "apps",
        name: "utilities-terminal",
        aliases: &[("apps", "terminal")],
    },
    Export {
        art: Art::Icon(IconName::Music),
        context: "mimetypes",
        name: "audio-x-generic",
        aliases: &[("apps", "multimedia-audio-player")],
    },
    Export {
        art: Art::Icon(IconName::Document),
        context: "mimetypes",
        name: "text-x-generic",
        aliases: &[
            ("mimetypes", "x-office-document"),
            ("mimetypes", "application-pdf"),
        ],
    },
    Export {
        art: Art::Icon(IconName::Image),
        context: "mimetypes",
        name: "image-x-generic",
        aliases: &[("apps", "image-viewer")],
    },
    Export {
        art: Art::Icon(IconName::Video),
        context: "mimetypes",
        name: "video-x-generic",
        aliases: &[("apps", "video-player")],
    },
    Export {
        art: Art::Icon(IconName::Archive),
        context: "mimetypes",
        name: "package-x-generic",
        aliases: &[],
    },
    Export {
        art: Art::Icon(IconName::Code),
        context: "mimetypes",
        name: "text-x-script",
        aliases: &[("apps", "applications-development")],
    },
    Export {
        art: Art::Icon(IconName::Calculator),
        context: "apps",
        name: "accessories-calculator",
        aliases: &[],
    },
    Export {
        art: Art::Icon(IconName::Game),
        context: "apps",
        name: "applications-games",
        aliases: &[("devices", "input-gaming")],
    },
    Export {
        art: Art::Icon(IconName::Store),
        context: "apps",
        name: "system-software-install",
        aliases: &[],
    },
    Export {
        art: Art::Icon(IconName::Globe),
        context: "apps",
        name: "applications-internet",
        aliases: &[("apps", "web-browser")],
    },
    Export {
        art: Art::Icon(IconName::Mail),
        context: "apps",
        name: "internet-mail",
        aliases: &[("apps", "mail-message-new")],
    },
    Export {
        art: Art::Icon(IconName::Chat),
        context: "apps",
        name: "internet-chat",
        aliases: &[("status", "user-available")],
    },
    Export {
        art: Art::Icon(IconName::Clock),
        context: "apps",
        name: "preferences-system-time",
        aliases: &[("apps", "appointment-new"), ("apps", "clock")],
    },
    Export {
        art: Art::Icon(IconName::Edit),
        context: "apps",
        name: "accessories-text-editor",
        aliases: &[("apps", "text-editor")],
    },
    Export {
        art: Art::Icon(IconName::Star),
        context: "emblems",
        name: "emblem-favorite",
        aliases: &[("actions", "starred")],
    },
    Export {
        art: Art::Icon(IconName::User),
        context: "apps",
        name: "system-users",
        aliases: &[("apps", "user-info")],
    },
    Export {
        art: Art::Icon(IconName::Info),
        context: "apps",
        name: "help-about",
        aliases: &[("status", "dialog-information")],
    },
    Export {
        art: Art::Icon(IconName::Wifi),
        context: "status",
        name: "network-wireless",
        aliases: &[("status", "network-wireless-signal-excellent")],
    },
    Export {
        art: Art::Icon(IconName::Volume),
        context: "status",
        name: "audio-volume-high",
        aliases: &[("status", "audio-speakers")],
    },
    Export {
        art: Art::Icon(IconName::Battery),
        context: "status",
        name: "battery-good",
        aliases: &[("status", "battery-full")],
    },
    Export {
        art: Art::Icon(IconName::Power),
        context: "actions",
        name: "system-shutdown",
        aliases: &[("actions", "system-log-out")],
    },
    Export {
        art: Art::Icon(IconName::Printer),
        context: "devices",
        name: "printer",
        aliases: &[],
    },
    Export {
        art: Art::Icon(IconName::Bluetooth),
        context: "devices",
        name: "bluetooth",
        aliases: &[("devices", "bluetooth-active")],
    },
    Export {
        art: Art::Icon(IconName::Monitor),
        context: "devices",
        name: "video-display",
        aliases: &[("devices", "computer")],
    },
    Export {
        art: Art::Icon(IconName::Screenshot),
        context: "apps",
        name: "applets-screenshooter",
        aliases: &[("devices", "camera-photo")],
    },
    Export {
        art: Art::Icon(IconName::Sliders),
        context: "apps",
        name: "preferences-other",
        aliases: &[],
    },
    Export {
        art: Art::Icon(IconName::Play),
        context: "actions",
        name: "media-playback-start",
        aliases: &[],
    },
    Export {
        art: Art::Icon(IconName::Pause),
        context: "actions",
        name: "media-playback-pause",
        aliases: &[],
    },
    Export {
        art: Art::Icon(IconName::Refresh),
        context: "actions",
        name: "view-refresh",
        aliases: &[],
    },
    Export {
        art: Art::Icon(IconName::Moon),
        context: "status",
        name: "weather-clear-night",
        aliases: &[("status", "night-light")],
    },
    Export {
        art: Art::Icon(IconName::Auto),
        context: "apps",
        name: "theme-auto",
        aliases: &[],
    },
    Export {
        art: Art::Icon(IconName::Brightness),
        context: "status",
        name: "display-brightness",
        aliases: &[],
    },
    Export {
        art: Art::Icon(IconName::App),
        context: "apps",
        name: "application-x-executable",
        aliases: &[],
    },
];

fn main() -> Result<(), Box<dyn Error>> {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("assets/icons/Luna-Icons"));

    // Eksportujemy zawsze pastelowy, wbudowany zestaw — bez zależności od
    // konfiguracji użytkownika i bez wybranego zestawu zewnętrznego.
    libluna_core::icons::set_active_icon_set(None);
    libluna_core::icons::set_active_icon_theme(IconTheme::Luna);

    let theme = Theme::light();
    let text = TextEngine::none();
    let contexts: BTreeSet<&str> = EXPORTS
        .iter()
        .flat_map(|e| {
            std::iter::once(e.context).chain(e.aliases.iter().map(|(context, _)| *context))
        })
        .collect();

    let mut rendered = 0usize;
    let mut linked = 0usize;
    for size in SIZES {
        let mut renderer = SoftwareRenderer::new(size, size);
        let mut scene = Scene::new();
        for export in EXPORTS {
            let dir = root.join(format!("{size}x{size}")).join(export.context);
            fs::create_dir_all(&dir)?;

            scene.clear();
            {
                let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
                draw_art(&mut ctx, export.art, size);
            }
            renderer.draw(&scene, 1.0, Color::TRANSPARENT);
            let png = renderer.encode_png().ok_or("nie udało się zakodować PNG")?;
            fs::write(dir.join(format!("{}.png", export.name)), png)?;
            rendered += 1;

            for (alias_context, alias_name) in export.aliases {
                let alias_dir = root.join(format!("{size}x{size}")).join(alias_context);
                fs::create_dir_all(&alias_dir)?;
                let link = alias_dir.join(format!("{alias_name}.png"));
                let _ = fs::remove_file(&link);
                let target = if *alias_context == export.context {
                    format!("{}.png", export.name)
                } else {
                    format!("../{}/{}.png", export.context, export.name)
                };
                make_symlink(Path::new(&target), &link)?;
                linked += 1;
            }
        }
    }

    write_index(&root, &contexts)?;
    let (svg_written, svg_linked) = export_svg(&root, &theme, &text)?;
    println!(
        "luna-icons: {} PNG + {} SVG + {} dowiązań w {}",
        rendered,
        svg_written,
        linked + svg_linked,
        root.display()
    );
    Ok(())
}

/// Eksportuje skalowalne wersje ikon (`scalable/<kontekst>/<nazwa>.svg`).
///
/// Rysujemy w nominalnej siatce 24×24 — tej samej, w której powstają ikony
/// interfejsu — więc współrzędne i grubości linii są identyczne z PNG.
fn export_svg(
    root: &Path,
    theme: &Theme,
    text: &TextEngine,
) -> Result<(usize, usize), Box<dyn Error>> {
    let mut written = 0usize;
    let mut linked = 0usize;
    for export in EXPORTS {
        let dir = root.join("scalable").join(export.context);
        fs::create_dir_all(&dir)?;

        let mut scene = Scene::new();
        {
            let mut ctx = PaintCtx::new(&mut scene, theme, text);
            draw_art(&mut ctx, export.art, SVG_SIZE);
        }
        let svg = scene_to_svg(&scene, SVG_SIZE as f64, SVG_SIZE as f64);
        fs::write(dir.join(format!("{}.svg", export.name)), svg)?;
        written += 1;

        for (alias_context, alias_name) in export.aliases {
            let alias_dir = root.join("scalable").join(alias_context);
            fs::create_dir_all(&alias_dir)?;
            let link = alias_dir.join(format!("{alias_name}.svg"));
            let _ = fs::remove_file(&link);
            let target = if *alias_context == export.context {
                format!("{}.svg", export.name)
            } else {
                format!("../{}/{}.svg", export.context, export.name)
            };
            make_symlink(Path::new(&target), &link)?;
            linked += 1;
        }
    }
    Ok((written, linked))
}

/// Rysuje pojedynczą ikonę w kwadracie `size × size`.
fn draw_art(ctx: &mut PaintCtx, art: Art, size: u32) {
    let s = size as f64;
    match art {
        Art::Icon(name) => paint_icon(ctx, name, Rect::from_xywh(0.0, 0.0, s, s), Color::BLACK),
        Art::Folder => icon::folder(
            ctx,
            Color::BLACK,
            Rect::from_xywh(s * 0.125, s * 0.21, s * 0.75, s * 0.625),
        ),
    }
}

#[cfg(unix)]
fn make_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
fn make_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    // Poza Uniksem nie ma tanich dowiązań — kopiujemy plik docelowy.
    fs::copy(link.parent().unwrap_or(Path::new(".")).join(target), link).map(|_| ())
}

/// Zapisuje `index.theme` z listą katalogów i opisem kontekstów.
fn write_index(root: &Path, contexts: &BTreeSet<&str>) -> std::io::Result<()> {
    let mut dirs = Vec::new();
    for size in SIZES {
        for context in contexts {
            dirs.push(format!("{size}x{size}/{context}"));
        }
    }
    for context in contexts {
        dirs.push(format!("scalable/{context}"));
    }

    let mut text = String::from("[Icon Theme]\nName=Luna Icons\n");
    text.push_str("Comment=Pastelowe ikony środowiska Luna\n");
    text.push_str("Inherits=hicolor\n");
    text.push_str("Example=folder\n");
    text.push_str(&format!("Directories={}\n", dirs.join(",")));

    for size in SIZES {
        for context in contexts {
            text.push_str(&format!(
                "\n[{size}x{size}/{context}]\nSize={size}\nContext={}\nType=Fixed\n",
                context_name(context)
            ));
        }
    }
    for context in contexts {
        text.push_str(&format!(
            "\n[scalable/{context}]\nSize={SVG_SIZE}\nMinSize=16\nMaxSize=512\nContext={}\nType=Scalable\n",
            context_name(context)
        ));
    }
    fs::write(root.join("index.theme"), text)
}

fn context_name(context: &str) -> &'static str {
    match context {
        "apps" => "Applications",
        "places" => "Places",
        "devices" => "Devices",
        "mimetypes" => "MimeTypes",
        "actions" => "Actions",
        "status" => "Status",
        "categories" => "Categories",
        "emblems" => "Emblems",
        _ => "Applications",
    }
}
