//! # libluna-widgets
//!
//! Zestaw widgetów libLuny w modelu **immediate**: widget to zwykła struktura ze stanem,
//! a aplikacja woła dla niego `paint` (rysunek) i `*_at`/`click_at` (trafianie i zmiana
//! stanu). Dzięki temu widgety są łatwe do testowania (hit-test bez okna) i tanie.
//!
//! Docelowo (M2) powstanie nad tym retencyjne drzewo z `measure`/`allocate`; te widgety
//! staną się jego liśćmi, ale kontrakt (`paint`/`hit`) zostaje.
//!
//! Widgety:
//! - [`ViewSwitcher`] — segmentowy przełącznik (`Lista | Siatka`, `Jasny | Ciemny | Auto`).
//! - [`NavList`] — menu nawigacji (sekcje + pozycje).
//! - [`Menu`] — popover z pozycjami, separatorami i skrótami (menu `☰`).
//! - [`Button`] — przycisk z etykietą i wariantami.
//! - [`Switch`] — przełącznik wł./wył.
//! - [`Slider`] — suwak 0..1.
//! - [`DropDown`] — lista rozwijana (rozwinięcie przez [`Menu`]).
//! - [`HeaderBar`] — pasek tytułu okna (CSD) z kontrolkami okna i obszarem na treść.
//! - [`TabBar`] / [`TabView`] — pasek zakładek i model stron (dodawanie, zamykanie, wybór).
//! - [`BottomSheet`] — dolny panel (modalny) z „grabberem” i zamykaniem.
//! - [`Pager`] — karuzela stron (przewijanie w poziomie + kropki-wskaźniki).
//! - [`PathBar`] — pasek ścieżki (okruszki) z separatorami i zwijaniem w wielokropek.
//! - [`Entry`] — pole tekstowe (kursor, edycja, opcjonalna ikona i czyszczenie).
//! - [`SearchEntry`] — pole tekstowe wyszukiwania (lupka, kursor, czyszczenie).
//! - [`ItemView`] — widok elementów (pliki/katalogi) w siatce lub liście.
//! - [`ScrollView`] — przewijanie pionowe (rolka + „kciuk”).
//! - [`Stepper`] — krokowiec (`−` wartość `+`).
//! - [`CheckButton`] — pole wyboru (checkbox) z etykietą.
//! - [`RadioGroup`] — lista opcji z pojedynczym wyborem.
//! - [`Card`] — karta z nagłówkiem i obszarem treści.
//! - [`ActionRow`] — wiersz ustawień (tytuł/podtytuł + obszar na kontrolkę).
//! - [`Toast`] — krótkie powiadomienie (nakładka) z opcjonalną akcją.
//! - [`StatusPage`] — ekran stanu (pusty/powitalny): ikona, tytuł, opis, akcja.
//!
//! ```no_run
//! use libluna_widgets::{PaintCtx, ViewSwitcher};
//! # fn demo(ctx: &mut PaintCtx, rect: libluna_core::Rect) {
//! let mut switcher = ViewSwitcher::new(["Lista", "Siatka"]);
//! switcher.paint(ctx, rect);
//! # }
//! ```

pub mod action_row;
pub mod bottom_sheet;
pub mod button;
pub mod card;
pub mod check_button;
pub mod ctx;
pub mod dropdown;
pub mod entry;
pub mod header_bar;
pub mod icon;
pub mod item_view;
pub mod menu;
pub mod nav_list;
pub mod pager;
pub mod path_bar;
pub mod radio_group;
pub mod scroll_view;
pub mod search_entry;
pub mod slider;
pub mod status_page;
pub mod stepper;
pub mod switch;
pub mod tab_bar;
pub mod tab_view;
pub mod toast;
pub mod view_switcher;

pub use action_row::ActionRow;
pub use bottom_sheet::BottomSheet;
pub use button::{Button, ButtonVariant};
pub use card::Card;
pub use check_button::CheckButton;
pub use ctx::PaintCtx;
pub use dropdown::DropDown;
pub use entry::{Entry, EntryIcon, TextInput};
pub use header_bar::{ControlsLayout, ControlsSide, HeaderBar, TitleAlign, WindowControl};
pub use item_view::{Item, ItemKind, ItemView, ViewMode};
pub use menu::{Menu, MenuEntry, MenuRow};
pub use nav_list::{NavEntry, NavList, NavRow};
pub use pager::Pager;
pub use path_bar::PathBar;
pub use radio_group::RadioGroup;
pub use scroll_view::ScrollView;
pub use search_entry::SearchEntry;
pub use slider::Slider;
pub use status_page::{StatusIcon, StatusPage, StatusRects};
pub use stepper::{Stepper, StepperSide};
pub use switch::Switch;
pub use tab_bar::{Tab, TabBar};
pub use tab_view::TabView;
pub use toast::Toast;
pub use view_switcher::ViewSwitcher;
