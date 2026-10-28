//! Headless-podglądy centrum ustawień do dokumentacji (bez kompozytora).
//!
//! Uruchom z katalogu repo:
//! `cargo run -p luna-settings --bin preview`
//! → `docs/images/luna-settings-*.png`.

use std::error::Error;

use libluna_core::{Color, Size, Theme, ThemePreference, ThemeSettings};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use luna::system::BatteryStatus;
use luna_settings::app::SettingsApp;
use luna_settings::pages::{Page, PickerKind};
use luna_settings::ui;

const W: u32 = 1080;
const H: u32 = 760;

fn main() -> Result<(), Box<dyn Error>> {
    let icons = libluna_core::IconSettings::load();
    libluna_core::icons::set_active_icon_set(icons.external);
    libluna_core::icons::set_active_icon_theme(icons.theme);
    let theme = Theme::dark();
    let text = TextEngine::system();
    let settings = ThemeSettings::load();
    let mut app = SettingsApp::new(&settings);
    app.system_ready = true;
    app.volume = Some((65, false));
    app.audio_sink = Some("Głośniki wbudowane".to_owned());
    app.brightness = Some(80);
    app.battery = Some(BatteryStatus {
        percent: 82,
        charging: false,
        minutes: Some(200),
    });
    app.dnd = false;
    app.power_saver = false;

    for (name, page) in [
        ("appearance", Page::Appearance),
        ("icons", Page::Icons),
        ("notifications", Page::Notifications),
        ("language", Page::Language),
        ("login", Page::Login),
        ("lock-screen", Page::LockScreen),
        ("workspaces", Page::Workspaces),
        ("displays", Page::Displays),
        ("sound", Page::Sound),
        ("power", Page::Power),
        ("system", Page::System),
        ("about", Page::About),
    ] {
        app.go_to(page);

        let mut scene = Scene::new();
        ui::paint(
            &app,
            &mut scene,
            Size::new(W as f64, H as f64),
            &theme,
            &text,
        );

        let mut renderer = SoftwareRenderer::new(W, H);
        renderer.draw(&scene, 1.0, Color::from_rgba8(0x1a, 0x1d, 0x27, 0xff));
        let png = renderer.encode_png().ok_or("failed to encode PNG")?;
        let path = format!("docs/images/luna-settings-{name}.png");
        std::fs::write(&path, png)?;
        println!("saved {path} ({W}x{H})");
    }

    // Wersja jasna (sprawdza, czy strona nie polega na ciemnych kolorach).
    app.go_to(Page::Appearance);
    app.theme_pref = ThemePreference::Light;
    let light = Theme::light();
    let mut scene = Scene::new();
    ui::paint(
        &app,
        &mut scene,
        Size::new(W as f64, H as f64),
        &light,
        &text,
    );
    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, Color::WHITE);
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/luna-settings-appearance-light.png", png)?;
    println!("saved docs/images/luna-settings-appearance-light.png ({W}x{H})");

    // Wyszukiwany wybór języka (splitbutton + lista z filtrem).
    app.go_to(Page::Language);
    app.open_picker(PickerKind::Language);
    let mut scene = Scene::new();
    ui::paint(
        &app,
        &mut scene,
        Size::new(W as f64, H as f64),
        &theme,
        &text,
    );
    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, Color::from_rgba8(0x1a, 0x1d, 0x27, 0xff));
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/luna-settings-language-picker.png", png)?;
    println!("saved docs/images/luna-settings-language-picker.png ({W}x{H})");

    Ok(())
}
