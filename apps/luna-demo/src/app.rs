//! Aplikacja demo Luny w wersji dokumentacyjnej.
//!
//! Okno pokazuje **wyłącznie** stronę „Widgety”: listę komponentów, działający
//! przykład, gotowy kod i notę „jak użyć”. Nie ma tu już makiety Ustawień —
//! pasek tytułu (CSD), wyszukiwarka, przełącznik motywu i wszystkie przykłady
//! obsługują prawdziwe zdarzenia okna.

use std::error::Error;

use libluna_core::{ButtonState, Event, KeyEvent, PointerButton, Rect, WindowEvent};
use libluna_platform::{run as run_app, AppConfig, Frame};
use libluna_widgets::{clipboard_command, TextClipboardCommand, WindowControl};

use crate::gallery_interactive::Focus;
use crate::gallery_page::{self, GalleryState};

/// Cel wklejenia ze schowka (zapamiętany przy Ctrl+V).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PasteTarget {
    Search,
    Entry,
    Password,
    ExampleSearch,
    SearchBar,
    Note,
}

/// Stan okna dokumentacji.
struct Docs {
    gallery: GalleryState,
    paste_target: Option<PasteTarget>,
}

/// Otwiera dokumentację widgetów.
pub fn run() -> Result<(), Box<dyn Error>> {
    run_docs()
}

/// Zgodność ze starą nazwą binarki `showcase` („Luna — widgety”).
pub fn run_widgets() -> Result<(), Box<dyn Error>> {
    run_docs()
}

fn run_docs() -> Result<(), Box<dyn Error>> {
    let settings = libluna_core::ThemeSettings::load();
    let preference = settings.preference();
    let system_dark = std::env::var("LUNA_SYSTEM_DARK")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);

    let config = AppConfig::new("Dokumentacja widgetów")
        .app_id("eu.paganlinux.Luna.Widgets")
        .initial_size(1200, 900)
        .settings(settings)
        .system_dark(system_dark)
        // Esc czyści fokus/pole w galerii, a nie zamyka demo.
        .escape_to_exit(false);

    let mut docs = Docs {
        gallery: GalleryState::default(),
        paste_target: None,
    };
    docs.gallery.theme_pref = preference;

    run_app(config, move |frame: &mut Frame| {
        let mut settings = frame.settings();
        let mut theme = settings.resolve_themed(frame.system_dark());
        let full = Rect::from_xywh(0.0, 0.0, frame.size.width, frame.size.height);
        let header = gallery_page::header_rect(full, &theme);
        let bar = gallery_page::header_bar();

        for event in frame.events {
            match event {
                Event::PointerButton(pe) if pe.button == PointerButton::Left => {
                    let p = pe.position;
                    match pe.state {
                        ButtonState::Pressed => {
                            if let Some(control) = bar.control_at(header, &theme, p) {
                                match control {
                                    WindowControl::Minimize => frame.minimize(),
                                    WindowControl::Maximize => frame.toggle_maximize(),
                                    WindowControl::Close => frame.close(),
                                }
                            } else if header.contains(p) {
                                // Przeciąganie okna za pasek tytułu (CSD).
                                frame.begin_move();
                            } else {
                                gallery_page::pointer_pressed(
                                    &mut docs.gallery,
                                    full,
                                    &theme,
                                    frame.text,
                                    p,
                                );
                            }
                        }
                        ButtonState::Released => {
                            gallery_page::pointer_released(&mut docs.gallery);
                        }
                    }
                }
                Event::PointerMotion { position, .. } => {
                    docs.gallery.header_hover = bar.control_at(header, &theme, *position);
                    gallery_page::pointer_motion(
                        &mut docs.gallery,
                        full,
                        &theme,
                        frame.text,
                        *position,
                    );
                }
                Event::PointerLeave => {
                    docs.gallery.header_hover = None;
                    gallery_page::pointer_left(&mut docs.gallery);
                }
                Event::Scroll(se) => {
                    gallery_page::scroll(
                        &mut docs.gallery,
                        full,
                        &theme,
                        frame.text,
                        se.position,
                        se.dy * 60.0,
                    );
                }
                Event::Key(ke) => {
                    if handle_clipboard(&mut docs, ke, frame) {
                        continue;
                    }
                    gallery_page::key(&mut docs.gallery, full, &theme, frame.text, ke);
                }
                Event::Text(text) => {
                    if docs.gallery.search.is_focused() {
                        docs.gallery.search.insert_str(text);
                        gallery_page::search_changed(&mut docs.gallery);
                    } else {
                        match docs.gallery.example.focus {
                            Focus::Entry => docs.gallery.example.entry.insert_str(text),
                            Focus::Password => docs.gallery.example.password.insert_str(text),
                            Focus::Search => docs.gallery.example.search.insert_str(text),
                            Focus::SearchBar => docs.gallery.example.bar_entry.insert_str(text),
                            Focus::Note => docs.gallery.example.note.insert_str(text),
                            Focus::None => {}
                        }
                    }
                }
                Event::Paste(text) => {
                    let Some(target) = docs.paste_target.take() else {
                        continue;
                    };
                    match target {
                        PasteTarget::Search => {
                            docs.gallery.search.paste(text);
                            gallery_page::search_changed(&mut docs.gallery);
                        }
                        PasteTarget::Entry => docs.gallery.example.entry.paste(&text),
                        PasteTarget::Password => docs.gallery.example.password.paste(&text),
                        PasteTarget::ExampleSearch => docs.gallery.example.search.paste(&text),
                        PasteTarget::SearchBar => docs.gallery.example.bar_entry.paste(&text),
                        PasteTarget::Note => docs.gallery.example.note.paste(&text),
                    }
                }
                Event::Window(WindowEvent::CloseRequested) => frame.close(),
                _ => {}
            }
        }

        // Zmiana motywu z przełącznika w stopce listy widgetów.
        if let Some(preference) = docs.gallery.theme_change.take() {
            settings.set_preference(preference);
            frame.set_preference(preference);
            theme = settings.resolve_themed(frame.system_dark());
        }

        // „Kopiuj kod” w dokumentacji → systemowy schowek.
        if let Some(text) = docs.gallery.copy_request.take() {
            frame.copy_to_clipboard(text);
        }

        let mut ctx = libluna_widgets::PaintCtx::new(frame.scene, &theme, frame.text);
        gallery_page::paint(&mut ctx, full, &docs.gallery);
    })
}

/// Które pole tekstowe ma teraz fokus (dla schowka).
fn paste_target(gallery: &GalleryState) -> Option<PasteTarget> {
    if gallery.search.is_focused() {
        return Some(PasteTarget::Search);
    }
    match gallery.example.focus {
        Focus::Entry => Some(PasteTarget::Entry),
        Focus::Password => Some(PasteTarget::Password),
        Focus::Search => Some(PasteTarget::ExampleSearch),
        Focus::SearchBar => Some(PasteTarget::SearchBar),
        Focus::Note => Some(PasteTarget::Note),
        Focus::None => None,
    }
}

/// Obsługa Ctrl+C / Ctrl+X / Ctrl+V dla aktywnego pola. Zwraca `true`, gdy zużyto.
fn handle_clipboard(docs: &mut Docs, ke: &KeyEvent, frame: &mut Frame) -> bool {
    let Some(target) = paste_target(&docs.gallery) else {
        return false;
    };
    let command = match target {
        PasteTarget::Search => clipboard_command(&mut docs.gallery.search, ke),
        PasteTarget::Entry => clipboard_command(&mut docs.gallery.example.entry, ke),
        PasteTarget::Password => clipboard_command(&mut docs.gallery.example.password, ke),
        PasteTarget::ExampleSearch => clipboard_command(&mut docs.gallery.example.search, ke),
        PasteTarget::SearchBar => clipboard_command(&mut docs.gallery.example.bar_entry, ke),
        PasteTarget::Note => clipboard_command(&mut docs.gallery.example.note, ke),
    };
    let Some(command) = command else {
        return false;
    };
    match command {
        TextClipboardCommand::Copy(text) | TextClipboardCommand::Cut(text) => {
            if let Some(text) = text {
                frame.copy_to_clipboard(text);
            }
        }
        TextClipboardCommand::Paste => {
            docs.paste_target = Some(target);
            frame.paste_from_clipboard();
        }
    }
    true
}
