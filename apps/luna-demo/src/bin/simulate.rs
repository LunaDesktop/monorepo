//! Headlessowy „driver" demo: przepuszcza **prawdziwe zdarzenia** przez tę samą logikę
//! co okno (`ui::handle_events` + `ui::build_scene`) i renderuje kolejne klatki.
//!
//! To dowód, że demo działa interaktywnie, bez potrzeby żywego kompozytora Wayland.
//!
//! Uruchom: `cargo run -p luna-demo --bin simulate`
//! Efekt: `docs/images/demo-step-*.png` oraz `docs/images/demo-montage.png`.

use std::error::Error;

use libluna_core::{
    Accent, ButtonState, Event, Modifiers, Point, PointerButton, PointerEvent, Size,
    ThemePreference, ThemeSettings,
};
use libluna_i18n::{set_locale, Locale};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use libluna_widgets::ViewSwitcher;
use luna_demo::ui::{self, make_nav, Demo, Popover};
use tiny_skia::{FilterQuality, IntSize, Pixmap, PixmapPaint, Transform};

const W: u32 = 900;
const H: u32 = 1028;

fn press(p: Point) -> Event {
    Event::PointerButton(PointerEvent {
        position: p,
        button: PointerButton::Left,
        state: ButtonState::Pressed,
        modifiers: Modifiers::NONE,
    })
}

fn render_pixels(settings: &ThemeSettings, demo: &Demo, text: &TextEngine, size: Size) -> Vec<u8> {
    let theme = settings.resolve(false);
    let mut scene = Scene::new();
    ui::build_scene(
        &mut scene,
        &theme,
        text,
        size,
        demo,
        settings.preference(),
        settings.accent(),
    );
    let mut r = SoftwareRenderer::new(size.width as u32, size.height as u32);
    r.draw(&scene, 1.0, theme.colors.window_bg);
    r.data().to_vec()
}

fn save_png(path: &str, pixels: &[u8]) -> Result<(), Box<dyn Error>> {
    let pixmap = Pixmap::from_vec(pixels.to_vec(), IntSize::from_wh(W, H).unwrap()).unwrap();
    pixmap.save_png(path)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    set_locale(Locale::new("pl"));

    let size = Size::new(W as f64, H as f64);
    let text = TextEngine::system();
    let mut demo = Demo::default();
    let mut settings = ThemeSettings::default();

    let mut frames: Vec<Vec<u8>> = Vec::new();
    let mut labels: Vec<String> = Vec::new();

    // 1. Start.
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("1. Start (jasny, PL)".into());

    // Pomocniczo: aktualny layout i wysłanie kliknięcia.
    macro_rules! layout {
        () => {
            ui::layout(size, &settings.resolve(false), &demo)
        };
    }
    macro_rules! click {
        ($pos:expr) => {{
            let theme = settings.resolve(false);
            ui::handle_events(
                &mut settings,
                &[press($pos)],
                size,
                &theme,
                &text,
                &mut demo,
            );
        }};
    }

    // 2. Klik „Ciemny" w przełączniku motywu.
    let c = ViewSwitcher::new(["a", "b", "c"]).segments(layout!().theme_switcher)[1].center();
    click!(c);
    assert_eq!(
        settings.preference(),
        ThemePreference::Dark,
        "motyw nie przeszedł na ciemny"
    );
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("2. Motyw: Ciemny".into());

    // 3. Klik kropki akcentu (Purple = indeks 7).
    let c = layout!().accents[7].center();
    click!(c);
    assert_eq!(
        settings.accent(),
        Accent::Purple,
        "akcent nie ustawił się na Purple"
    );
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("3. Akcent: Purple".into());

    // 4. Klik pozycji „Sieć" w menu nawigacji (indeks elementu 5).
    let theme = settings.resolve(false);
    let c = make_nav().rows(layout!().sidebar, &theme)[5].rect.center();
    click!(c);
    assert_eq!(demo.nav_selected, Some(5), "nie zaznaczono pozycji Sieć");
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("4. Nawigacja: Sieć".into());

    // 5. Klik „Siatka" w przełączniku widoku.
    let c = ViewSwitcher::new(["a", "b"]).segments(layout!().view_switcher)[1].center();
    click!(c);
    assert_eq!(demo.view_mode, 1, "widok nie przeszedł na Siatkę");
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("5. Widok: Siatka".into());

    // 6. Przełączenie zakładki (TabView + TabBar).
    let theme = settings.resolve(false);
    let c = ui::tab_bar(&demo).segments(layout!().tabs, &text, &theme)[2].center();
    click!(c);
    assert_eq!(demo.tabs.selected(), 2, "zakładka się nie przełączyła");
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("6. Zakładka: O aplikacji".into());

    // 7. Klik ikony ◧ w HeaderBarze — zwinięcie paska bocznego.
    let c = layout!().sidebar_button.center();
    click!(c);
    assert!(!demo.sidebar_visible, "pasek boczny się nie zwinął");
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("7. Pasek boczny zwinięty ◧".into());

    // 8. Klik ikony „☰” — otwarcie popoveru (menu nawigacji).
    let c = layout!().burger.center();
    click!(c);
    assert_eq!(demo.popover, Some(Popover::Menu), "menu się nie otworzyło");
    // Podświetlenie drugiej pozycji, żeby pokazać hover.
    let theme = settings.resolve(false);
    if let Some((panel, menu)) = ui::popover_menu(size, &theme, &text, &layout!(), &demo) {
        let hover = menu.rows(panel, &theme)[1].rect.center();
        ui::handle_events(
            &mut settings,
            &[Event::PointerMotion {
                position: hover,
                modifiers: Modifiers::NONE,
            }],
            size,
            &theme,
            &text,
            &mut demo,
        );
    }
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("8. Menu ☰ otwarte".into());

    // 9. Klik „O aplikacji” w otwartym menu → dolny panel (BottomSheet + Pager).
    let theme = settings.resolve(false);
    let about = ui::popover_menu(size, &theme, &text, &layout!(), &demo)
        .map(|(panel, menu)| menu.rows(panel, &theme)[4].rect.center())
        .expect("menu otwarte");
    click!(about);
    assert!(demo.sheet.is_open(), "panel się nie otworzył");
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("9. Dolny panel \u{201e}O aplikacji\u{201d}".into());

    // 10. Zamknięcie panelu (klik w tło) i rozwinięcie listy języków (DropDown + Menu).
    click!(Point::new(120.0, 300.0));
    assert!(!demo.sheet.is_open(), "panel się nie zamknął");
    let c = layout!().language.center();
    click!(c);
    assert_eq!(
        demo.popover,
        Some(Popover::Language),
        "lista języków się nie rozwinęła"
    );
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("10. Wybór języka (DropDown)".into());

    // 11. Popover „Opcje” (rozmiar ikon / sortowanie / ukryte pliki).
    click!(Point::new(450.0, 300.0)); // zamknij popover języka
    assert!(demo.popover.is_none(), "popover się nie zamknął");
    let c = layout!().options_button.center();
    click!(c);
    assert_eq!(
        demo.popover,
        Some(Popover::Options),
        "popover opcji się nie otworzył"
    );
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("11. Popover \u{201e}Opcje\u{201d}".into());

    // Zapis klatek.
    for (i, pixels) in frames.iter().enumerate() {
        let path = format!("docs/images/demo-step-{}.png", i + 1);
        save_png(&path, pixels)?;
        println!("zapisano {path}  — {}", labels[i]);
    }

    // Montaż (siatka `cols` x `rows`) na zmienną liczbę klatek.
    let cols = 3usize;
    let rows = frames.len().div_ceil(cols);
    let cell_w = W / cols as u32;
    let cell_h = (H as f64 * cell_w as f64 / W as f64).round() as u32;
    let mut montage = Pixmap::new(cell_w * cols as u32, cell_h * rows as u32).unwrap();
    let paint = PixmapPaint {
        quality: FilterQuality::Bilinear,
        ..Default::default()
    };
    let scale = Transform::from_scale(cell_w as f32 / W as f32, cell_h as f32 / H as f32);
    for (i, pixels) in frames.iter().enumerate() {
        let src = Pixmap::from_vec(pixels.clone(), IntSize::from_wh(W, H).unwrap()).unwrap();
        let cx = (i % cols) as u32 * cell_w;
        let cy = (i / cols) as u32 * cell_h;
        montage.draw_pixmap(cx as i32, cy as i32, src.as_ref(), &paint, scale, None);
    }
    montage.save_png("docs/images/demo-montage.png")?;
    println!("zapisano docs/images/demo-montage.png");

    println!(
        "\nWszystkie interakcje przeszły pomyślnie ({} klatek).",
        frames.len()
    );
    Ok(())
}
