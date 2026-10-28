//! Wspólna obsługa schowka w polach tekstowych.
//!
//! Widgety nie znają `libluna-platform`, więc nie mogą same sięgnąć do
//! systemowego schowka. Zamiast tego rozpoznają skrót i zwracają intencję:
//! aplikacja wysyła tekst przez `Frame::copy_to_clipboard`, a przy `Paste`
//! prosi o odczyt i wpisuje wynik do właściwego pola po `Event::Paste`.
//!
//! Dzięki temu logika klawiszy jest identyczna we wszystkich polach
//! (`Entry`, `SearchEntry`, `PasswordEntry`, `SpinButton`) i testowalna bez okna.

use libluna_core::event::{ButtonState, KeyEvent};

use crate::entry::TextInput;

/// Co aplikacja ma zrobić ze schowkiem po naciśnięciu Ctrl+C/X/V.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextClipboardCommand {
    /// Skopiuj zaznaczenie; `None`, gdy nic nie jest zaznaczone.
    Copy(Option<String>),
    /// Wytnij zaznaczenie; `None`, gdy nic nie jest zaznaczone.
    Cut(Option<String>),
    /// Poproś o zawartość schowka i wklej ją do aktywnego pola.
    Paste,
}

/// Rozpoznaje Ctrl+C / Ctrl+X / Ctrl+V dla pola tekstowego.
///
/// Zwraca `Some` także wtedy, gdy nie ma zaznaczenia (`Copy(None)`/`Cut(None)`),
/// żeby skrót nie „przeciekał” do akcji aplikacji (np. kopiowania plików w
/// menedżerze plików). Zwraca `None` dla pozostałych klawiszy.
pub fn clipboard_command<T: TextInput>(
    entry: &mut T,
    event: &KeyEvent,
) -> Option<TextClipboardCommand> {
    if event.state != ButtonState::Pressed || !event.modifiers.ctrl {
        return None;
    }
    match event.keysym {
        0x43 | 0x63 => {
            let text = entry.copy();
            Some(TextClipboardCommand::Copy(text))
        }
        0x58 | 0x78 => {
            let text = entry.cut();
            Some(TextClipboardCommand::Cut(text))
        }
        0x56 | 0x76 => Some(TextClipboardCommand::Paste),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_core::event::{ButtonState, Modifiers};

    use crate::Entry;

    fn key(keysym: u32, ctrl: bool) -> KeyEvent {
        KeyEvent {
            keysym,
            keycode: 0,
            text: None,
            state: ButtonState::Pressed,
            modifiers: Modifiers {
                ctrl,
                ..Modifiers::NONE
            },
            repeat: false,
        }
    }

    #[test]
    fn recognizes_shortcuts_and_returns_selected_text() {
        let mut entry = Entry::new("Name");
        entry.set_text("hello");
        entry.select_all();

        assert_eq!(
            clipboard_command(&mut entry, &key(0x63, true)),
            Some(TextClipboardCommand::Copy(Some("hello".to_owned())))
        );

        assert_eq!(
            clipboard_command(&mut entry, &key(0x78, true)),
            Some(TextClipboardCommand::Cut(Some("hello".to_owned())))
        );
        assert_eq!(entry.text(), "");

        assert_eq!(
            clipboard_command(&mut entry, &key(0x76, true)),
            Some(TextClipboardCommand::Paste)
        );
    }

    #[test]
    fn consumes_shortcut_even_without_selection() {
        let mut entry = Entry::new("Name");
        assert_eq!(
            clipboard_command(&mut entry, &key(0x43, true)),
            Some(TextClipboardCommand::Copy(None))
        );
        assert_eq!(
            clipboard_command(&mut entry, &key(0x58, true)),
            Some(TextClipboardCommand::Cut(None))
        );
    }

    #[test]
    fn plain_letters_are_not_clipboard_commands() {
        let mut entry = Entry::new("Name");
        assert_eq!(clipboard_command(&mut entry, &key(0x63, false)), None);
        assert_eq!(clipboard_command(&mut entry, &key(0x61, true)), None);
    }
}
