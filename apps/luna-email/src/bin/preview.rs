//! Headless-podglądy `luna-email` do dokumentacji (bez kompozytora).
//!
//! Uruchom: `cargo run --bin preview`
//! → `docs/images/email-inbox.png`, `email-compose.png`, `email-account.png`,
//!   `email-about.png`, `email-narrow.png`.

use std::error::Error;
use std::path::PathBuf;

use libluna_core::{Size, Theme};
use libluna_i18n::{set_locale, Locale};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use luna_email::app::{App, ComposeField, View};
use luna_email::ui;

const WIDE: Size = Size {
    width: 1080.0,
    height: 720.0,
};
const NARROW: Size = Size {
    width: 640.0,
    height: 620.0,
};

fn main() -> Result<(), Box<dyn Error>> {
    set_locale(Locale::new("pl"));

    let text = TextEngine::system();
    let dark = Theme::dark();

    let mut inbox = demo();
    inbox.selected = 0;
    save(&mut inbox, WIDE, &dark, &text, "email-inbox.png")?;

    let mut compose = demo();
    compose.compose_new();
    compose
        .compose
        .to
        .set_text("marta@example.org, zespol@paganlinux.org");
    compose
        .compose
        .subject
        .set_text("Re: Projekt graficzny na piątek");
    compose.compose.body.set_text(
        "Cześć Marto,\n\nprojekt wygląda świetnie — wariant ciemny jest dużo lepszy. \
         Wysyłam uwagi do ikon w załączniku.\n\nDzięki!",
    );
    compose.compose.field = ComposeField::Body;
    compose.compose.focus();
    save(&mut compose, WIDE, &dark, &text, "email-compose.png")?;

    let mut account = demo();
    account.open_account_settings();
    account.account_form.email.set_text("patryk@paganlinux.org");
    account.account_form.display.set_text("Patryk");
    account.account_form.login.set_text("patryk@paganlinux.org");
    account.account_form.password.set_text("hasło-aplikacji");
    account
        .account_form
        .imap_host
        .set_text("imap.paganlinux.org");
    account
        .account_form
        .smtp_host
        .set_text("smtp.paganlinux.org");
    save(&mut account, WIDE, &dark, &text, "email-account.png")?;

    let mut about = demo();
    about.view = View::About;
    save(&mut about, WIDE, &dark, &text, "email-about.png")?;

    let mut narrow = demo();
    narrow.sidebar_visible = false;
    narrow.selected = 1;
    save(&mut narrow, NARROW, &dark, &text, "email-narrow.png")?;

    Ok(())
}

fn demo() -> App {
    let mut app = App::demo();
    app.load_demo();
    app
}

fn save(
    app: &mut App,
    size: Size,
    theme: &Theme,
    text: &TextEngine,
    name: &str,
) -> Result<(), Box<dyn Error>> {
    let mut scene = Scene::new();
    ui::paint(app, &mut scene, size, theme, text);

    let width = size.width.round() as u32;
    let height = size.height.round() as u32;
    let mut renderer = SoftwareRenderer::new(width, height);
    renderer.draw(&scene, 1.0, theme.colors.window_bg);
    let png = renderer.encode_png().ok_or("nie udało się zakodować PNG")?;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/images")
        .join(name);
    std::fs::write(&path, png)?;
    println!("saved {}", path.display());
    Ok(())
}
