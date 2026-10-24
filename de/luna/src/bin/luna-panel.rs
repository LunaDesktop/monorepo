//! Podgląd powłoki Luny (**bez kompozytora**) — „zrzuty ekranu” do weryfikacji
//! wyglądu i układu bez uruchamiania sesji DRM.
//!
//! Renderuje **ten sam** pasek, launcher i popupy, których używa `luna-shell`, przez
//! `libluna-render` — w rozmiarze realnej powierzchni „chrome” (1920×680). Na wierzchu
//! kładzie tło pulpitu, żeby widać było półprzezroczyste przyciemnienie pod popupem.
//!
//! Uruchom: `cargo run -p luna --bin luna-panel`
//! Efekt:   `docs/images/luna-*.png`
//!
//! To jest narzędzie diagnostyczne: każda zmiana wyglądu paska/launchera powinna tu
//! zostawić ślad, który da się obejrzeć bez przejmowania ekranu.

use std::error::Error;

use libluna_core::{Color, Rect, Size, Theme};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use libluna_widgets::{IconName, PaintCtx};
use luna::notifications::{Notification, Notifications};
use luna::osd::Osd;
use luna::panel::{self, App, Launcher, LauncherView, PanelState, LAUNCHER_H, THICKNESS};
use luna::system::WifiNetwork;

/// Szerokość powierzchni (Full HD).
const W: u32 = 1920;
/// Wysokość powierzchni „chrome” — taka sama jak `CHROME_H` w `luna-shell`.
const H: u32 = 680;

fn main() -> Result<(), Box<dyn Error>> {
    let theme = Theme::dark();
    let apps = App::sample();
    let bar = Rect::from_xywh(0.0, 0.0, W as f64, THICKNESS);
    let mut st = PanelState::sample();
    st.tasks.clear();

    let area = |w: f64| {
        let lw = w.min((W as f64 - 80.0).max(320.0));
        Rect::from_xywh((W as f64 - lw) / 2.0, THICKNESS + 24.0, lw, LAUNCHER_H)
    };

    // ── 1) Pasek w spoczynku ────────────────────────────────────────────────────
    save(&theme, "docs/images/luna-panel-idle.png", |ctx| {
        panel::paint_panel(&st, ctx, bar);
    })?;

    // ── 2) Launcher w trzech widokach ───────────────────────────────────────────
    for (name, view) in [
        ("list", LauncherView::List),
        ("grid", LauncherView::Grid),
        ("carousel", LauncherView::Carousel),
    ] {
        let mut launcher = Launcher::new();
        launcher.view = view;
        launcher.carousel = 2;
        launcher.selected = Some(if view == LauncherView::List { 4 } else { 2 });
        launcher.scroll = if view == LauncherView::List {
            96.0
        } else {
            0.0
        };
        let larea = area(panel::LAUNCHER_W);
        save(
            &theme,
            &format!("docs/images/luna-launcher-{name}.png"),
            |ctx| {
                panel::paint_panel(&st, ctx, bar);
                panel::paint_launcher(&apps, &launcher, ctx, larea);
            },
        )?;
    }

    // ── 3) Popupy: Wi-Fi, dźwięk, kalendarz, sesja ──────────────────────────────
    let networks = vec![
        WifiNetwork {
            ssid: "PaganNet".to_owned(),
            signal: 92,
            active: true,
        },
        WifiNetwork {
            ssid: "Dom".to_owned(),
            signal: 71,
            active: false,
        },
        WifiNetwork {
            ssid: "Sąsiad_5G".to_owned(),
            signal: 44,
            active: false,
        },
    ];
    save(&theme, "docs/images/luna-popup-wifi.png", |ctx| {
        panel::paint_panel(&st, ctx, bar);
        let r = panel::wifi_popup_rect(bar, ctx.theme, ctx.text, &st);
        panel::paint_wifi_popup(ctx, r, true, Some("PaganNet"), &networks);
    })?;

    save(&theme, "docs/images/luna-popup-volume.png", |ctx| {
        panel::paint_panel(&st, ctx, bar);
        let r = panel::volume_popup_rect(bar, ctx.theme, ctx.text, &st);
        panel::paint_volume_popup(ctx, r, 65, false);
    })?;

    save(&theme, "docs/images/luna-popup-calendar.png", |ctx| {
        panel::paint_panel(&st, ctx, bar);
        let r = panel::calendar_popup_rect(bar, ctx.theme);
        panel::paint_calendar_popup(ctx, r, &st.clock, &st.date, 2026, 10, 2026, 10, 8, Some(8));
    })?;

    save(&theme, "docs/images/luna-popup-session.png", |ctx| {
        panel::paint_panel(&st, ctx, bar);
        let items = luna::power::items();
        panel::paint_session_menu(ctx, bar, &items);
    })?;

    // ── 4) OSD (głośność) + powiadomienia ───────────────────────────────────────
    save(&theme, "docs/images/luna-osd.png", |ctx| {
        panel::paint_panel(&st, ctx, bar);
        let mut osd = Osd::new(IconName::Volume, "Głośność", 0.65);
        osd.advance(0.3);
        osd.paint(
            ctx,
            Rect::from_xywh((W as f64 - 340.0) / 2.0, THICKNESS + 16.0, 340.0, 72.0),
        );
        let mut notes = Notifications::new();
        notes.push(
            Notification::new("Pliki", "Kopiowanie zakończone", "12 plików → Dokumenty")
                .with_icon(IconName::Files),
        );
        notes.advance(0.3);
        notes.paint(ctx, Rect::from_xywh(0.0, THICKNESS + 8.0, W as f64, 200.0));
    })?;

    Ok(())
}

/// Rysuje tło pulpitu + zawartość + zapisuje PNG.
fn save(theme: &Theme, path: &str, draw: impl FnOnce(&mut PaintCtx)) -> Result<(), Box<dyn Error>> {
    let text = TextEngine::system();
    let mut scene = Scene::new();
    {
        let mut ctx = PaintCtx::new(&mut scene, theme, &text);
        // Tło pulpitu (pod półprzezroczystą powierzchnią chrome).
        ctx.scene
            .fill_rect(Rect::from_size(Size::new(W as f64, H as f64)), desktop_bg());
        draw(&mut ctx);
    }
    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, theme.colors.window_bg);
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write(path, png)?;
    println!("saved {path} ({W}x{H})");
    Ok(())
}

/// Kolor „pulpitu” pod powierzchnią chrome (żeby przyciemnienie było widoczne).
fn desktop_bg() -> Color {
    Color::from_rgba8(0x1a, 0x1d, 0x27, 0xff)
}
