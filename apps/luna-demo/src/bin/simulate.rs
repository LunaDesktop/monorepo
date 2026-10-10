//! Headlessowy „driver" demo: przepuszcza **prawdziwe zdarzenia** przez tę samą logikę
//! co okno (`ui::handle_events` + `ui::build_scene`) i renderuje kolejne klatki.
//!
//! To dowód, że demo działa interaktywnie, bez potrzeby żywego kompozytora Wayland.
//!
//! Uruchom: `cargo run -p luna-demo --bin simulate`
//! Efekt: `docs/images/demo-step-*.png` oraz `docs/images/demo-montage.png`.

use std::error::Error;

use libluna_core::{
    Accent, ButtonState, Event, KeyEvent, Modifiers, Point, PointerButton, PointerEvent, Size,
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
    // Headless: domknij trwające animacje, by klatka pokazywała stan ustalony.
    let mut demo = demo.clone();
    demo.tab_stack.advance(1.0, 1.0);
    demo.toast.advance(1.0, 1.0);
    let theme = settings.resolve_themed(false);
    let mut scene = Scene::new();
    ui::build_scene(
        &mut scene,
        &theme,
        text,
        size,
        &demo,
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
    // Katalog ze stylami (przykłady w repozytorium) — dla kroku wyboru stylu.
    let themes =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/luna/themes");
    std::env::set_var("LUNA_THEMES_DIR", themes);

    let size = Size::new(W as f64, H as f64);
    let text = TextEngine::system();
    let mut demo = Demo::default();
    let mut settings = ThemeSettings::default();

    let mut frames: Vec<Vec<u8>> = Vec::new();
    let mut labels: Vec<String> = Vec::new();

    // 1. Start.
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("1. Start (light, PL)".into());

    // Pomocniczo: aktualny layout i wysłanie kliknięcia.
    macro_rules! layout {
        () => {
            ui::layout(size, &settings.resolve_themed(false), &demo)
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
        "theme did not switch to Dark"
    );
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("2. Theme: Dark".into());

    // 3. Klik kropki akcentu (Purple = indeks 7).
    let c = layout!().accents[7].center();
    click!(c);
    assert_eq!(
        settings.accent(),
        Accent::Purple,
        "accent did not switch to Purple"
    );
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("3. Accent: Purple".into());

    // Zamknięcie powiadomienia klikiem w jego treść (akcent zostaje).
    let theme = settings.resolve(false);
    let toast = demo.toast.rect(size, &theme, &text);
    click!(Point::new(toast.x() + 8.0, toast.center().y));
    assert!(!demo.toast.is_open(), "toast did not dismiss");

    // 4. Klik pozycji „Sieć" w menu nawigacji (indeks elementu 5).
    let theme = settings.resolve(false);
    let c = make_nav().rows(layout!().sidebar, &theme)[5].rect.center();
    click!(c);
    assert_eq!(
        demo.nav_selected,
        Some(5),
        "nav item 'Network' not selected"
    );
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("4. Navigation: Network".into());

    // 5. Klik „Siatka" w przełączniku widoku.
    let c = ViewSwitcher::new(["a", "b"]).segments(layout!().view_switcher)[1].center();
    click!(c);
    assert_eq!(demo.view_mode, 1, "view did not switch to Grid");
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("5. View: Grid".into());

    // 6. Przełączenie zakładki (TabView + TabBar).
    let theme = settings.resolve(false);
    let c = ui::tab_bar(&demo).segments(layout!().tabs, &text, &theme)[2].center();
    click!(c);
    assert_eq!(demo.tabs.selected(), 2, "tab did not switch");
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("6. Tab: About".into());

    // 7. Klik ikony ◧ w HeaderBarze — zwinięcie paska bocznego.
    let c = layout!().sidebar_button.center();
    click!(c);
    assert!(!demo.sidebar_visible, "sidebar did not collapse");
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("7. Sidebar collapsed \u{25e7}".into());

    // 8. Klik ikony „☰” — otwarcie popoveru (menu nawigacji).
    let c = layout!().burger.center();
    click!(c);
    assert_eq!(demo.popover, Some(Popover::Menu), "menu did not open");
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
    labels.push("8. Menu \u{2630} open".into());

    // 9. Klik „O aplikacji” w otwartym menu → dolny panel (BottomSheet + Pager).
    let theme = settings.resolve(false);
    let about = ui::popover_menu(size, &theme, &text, &layout!(), &demo)
        .map(|(panel, menu)| menu.rows(panel, &theme)[5].rect.center())
        .expect("menu is open");
    click!(about);
    assert!(demo.sheet.is_open(), "bottom sheet did not open");

    // Pole `Entry` w panelu: fokus i wpisanie tekstu.
    let entry = ui::sheet_entry_rect(size, &settings.resolve(false));
    click!(entry.center());
    assert!(demo.sheet_entry.is_focused(), "entry not focused");
    for ch in ['A', 'd', 'a'] {
        let key = Event::Key(KeyEvent {
            keysym: ch as u32,
            text: Some(ch.to_string()),
            state: ButtonState::Pressed,
            modifiers: Modifiers::NONE,
            repeat: false,
        });
        let theme = settings.resolve(false);
        ui::handle_events(&mut settings, &[key], size, &theme, &text, &mut demo);
    }
    assert_eq!(demo.sheet_entry.text(), "Ada", "entry text mismatch");

    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("9. Bottom sheet \u{201c}About\u{201d} + Entry".into());

    // 10. Zamknięcie panelu (klik w tło) i rozwinięcie listy języków (DropDown + Menu).
    click!(Point::new(120.0, 300.0));
    assert!(!demo.sheet.is_open(), "bottom sheet did not close");
    let c = layout!().language.center();
    click!(c);
    assert_eq!(
        demo.popover,
        Some(Popover::Language),
        "language list did not open"
    );
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("10. Language selection (DropDown)".into());

    // 11. Popover „Opcje” (rozmiar ikon / sortowanie / ukryte pliki).
    click!(Point::new(450.0, 300.0)); // zamknij popover języka
    assert!(demo.popover.is_none(), "popover did not close");
    let c = layout!().options_button.center();
    click!(c);
    assert_eq!(
        demo.popover,
        Some(Popover::Options),
        "options popover did not open"
    );
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("11. Options popover".into());

    // 12. Klik popovera (zamknięcie) i zmiana akcentu → powiadomienie (Toast) z „Cofnij”.
    click!(Point::new(450.0, 300.0));
    let c = layout!().accents[1].center();
    click!(c);
    assert_eq!(
        settings.accent(),
        Accent::Teal,
        "accent did not switch to Teal"
    );
    assert!(demo.toast.is_open(), "toast did not appear");
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("12. Toast".into());

    // 13. Klik „Cofnij” w powiadomieniu → powrót akcentu, powiadomienie znika.
    let theme = settings.resolve(false);
    let undo = demo
        .toast
        .action_rect(size, &theme, &text)
        .expect("toast action")
        .center();
    click!(undo);
    assert_eq!(
        settings.accent(),
        Accent::Purple,
        "accent did not revert after Undo"
    );
    assert!(!demo.toast.is_open(), "toast did not disappear");
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("13. Accent reverted".into());

    // 14. Filtr bez wyników → ekran stanu (StatusPage) z akcją „Wyczyść”.
    let c = layout!().search.center();
    click!(c);
    assert!(demo.search.is_focused(), "search field is not focused");
    for ch in ['x', 'y', 'z'] {
        let key = Event::Key(KeyEvent {
            keysym: ch as u32,
            text: Some(ch.to_string()),
            state: ButtonState::Pressed,
            modifiers: Modifiers::NONE,
            repeat: false,
        });
        let theme = settings.resolve(false);
        ui::handle_events(&mut settings, &[key], size, &theme, &text, &mut demo);
    }
    assert!(
        ui::empty_state(&demo).is_some(),
        "status page missing for empty search"
    );
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("14. No results (StatusPage)".into());

    // 15. Klik „Wyczyść” w ekranie stanu → lista wraca.
    let theme = settings.resolve(false);
    let clear = ui::empty_state(&demo)
        .expect("status page")
        .rects(layout!().items, &theme)
        .action
        .expect("status page action")
        .center();
    click!(clear);
    assert!(demo.search.is_empty(), "search filter was not cleared");
    assert!(
        ui::empty_state(&demo).is_none(),
        "status page did not disappear"
    );
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("15. Filter cleared".into());

    // 16. Wybór stylu: ☰ → Styl → Nord (styl z pliku, steruje kolorem i geometrią).
    let c = layout!().burger.center();
    click!(c);
    assert_eq!(demo.popover, Some(Popover::Menu), "menu did not open");
    let theme = settings.resolve_themed(false);
    let style_item = ui::popover_menu(size, &theme, &text, &layout!(), &demo)
        .map(|(panel, menu)| menu.rows(panel, &theme)[4].rect.center())
        .expect("style entry");
    click!(style_item);
    assert_eq!(
        demo.popover,
        Some(Popover::Styles),
        "styles popover did not open"
    );
    let theme = settings.resolve_themed(false);
    let nord = ui::popover_menu(size, &theme, &text, &layout!(), &demo)
        .map(|(panel, menu)| menu.rows(panel, &theme)[1].rect.center())
        .expect("style list");
    click!(nord);
    assert_eq!(settings.style(), Some("nord"), "style not applied");
    frames.push(render_pixels(&settings, &demo, &text, size));
    labels.push("16. Style: Nord".into());

    // Zapis klatek.
    for (i, pixels) in frames.iter().enumerate() {
        let path = format!("docs/images/demo-step-{}.png", i + 1);
        save_png(&path, pixels)?;
        println!("saved {path}  — {}", labels[i]);
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
    for (i, pixels) in frames.iter().enumerate() {
        let src = Pixmap::from_vec(pixels.clone(), IntSize::from_wh(W, H).unwrap()).unwrap();
        let cx = (i % cols) as u32 * cell_w;
        let cy = (i / cols) as u32 * cell_h;
        // Skalowanie i przesunięcie w jednej transformacji (inaczej komórki lądują w rogu).
        let t = Transform::from_scale(cell_w as f32 / W as f32, cell_h as f32 / H as f32)
            .post_translate(cx as f32, cy as f32);
        montage.draw_pixmap(0, 0, src.as_ref(), &paint, t, None);
    }
    montage.save_png("docs/images/demo-montage.png")?;
    println!("saved docs/images/demo-montage.png");

    println!("\nAll interactions passed ({} frames).", frames.len());
    Ok(())
}
