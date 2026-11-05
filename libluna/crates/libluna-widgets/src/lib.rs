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
//! - [`Spinner`] — wskaźnik zajętości (obracający się łuk).
//! - [`Tooltip`] — dymek podpowiedzi nad elementem.
//! - [`Banner`] — pasek komunikatu (info/ostrzeżenie/błąd) z akcjami.
//! - [`Avatar`] — okrągły awatar z inicjałami.
//! - [`ShortcutLabel`] — skrót klawiszowy jako klawisze (`Ctrl` `+` `N`).
//! - [`ToggleGroup`] — grupa przełączników (wielokrotny wybór).
//! - [`SplitButton`] — przycisk dzielony (akcja + strzałka).
//! - [`Bin`] — kontener jednego dziecka (wcięcie + wyrównanie).
//! - [`Row`] — wiersz listy (switch/combo/expander/entry/spin…) — rodzina „boxed list”.
//! - [`PreferencesGroup`] / [`PreferencesPage`] — grupa i strona ustawień.
//! - [`Dialog`] (+ [`AlertDialog`], [`AboutDialog`], [`ShortcutsDialog`], [`PreferencesDialog`]) — okna modalne.
//! - [`NavigationView`] / [`NavigationSplitView`] / [`OverlaySplitView`] / [`Sidebar`] — nawigacja.
//! - [`Label`], [`Separator`], [`ProgressBar`], [`LevelBar`], [`Frame`], [`Revealer`], [`Overlay`],
//!   [`AspectFrame`] — widgety prezentacji (GTK4 Display).
//! - [`ToggleButton`], [`LinkButton`], [`ColorButton`] — pozostałe przyciski (GTK4).
//! - [`PasswordEntry`], [`EditableLabel`] — pola specjalne (GTK4).
//! - [`CenterBox`], [`Grid`], [`Fixed`], [`Paned`], [`Stack`], [`ActionBar`], [`PopoverMenuBar`] —
//!   kontenery GTK4 (`Box` = stos drzewa układu: [`Node::hbox`]/[`Node::vbox`]).
//! - [`MenuButton`], [`SearchBar`], [`Expander`], [`Statusbar`], [`Calendar`] — kolejne widgety GTK4.
//! - [`Image`], [`TextView`] — obrazek (ikony wektorowe) i pole wielolinijkowe (GTK4).
//! - [`CodeView`] — edytor kodu: kolorowane spany, gutter z numerami linii,
//!   bieżąca linia, przewijanie w obu osiach i trafianie kursorem w tekst.
//! - [`Popover`], [`ColumnView`], [`Assistant`] — generyczny popover, tabela i kreator kroków (GTK4).
//! - [`Scrollbar`]/[`Adjustment`], [`ColorChooser`] — przewijanie (obie osie) i paleta kolorów.
//! - Animacje (`libluna-core::motion`): `Easing`/`lerp`/`Animation`; użyte w [`Revealer`],
//!   [`Toast`] i przejściach [`Stack`] ([`StackTransition`]).
//! - [`FlowBox`], [`SpinButton`], [`EmojiChooser`], [`NavigationRail`] — kolejne widgety libLuny.
//! - [`Hover`] — śledzenie kursora i spójne podświetlanie elementów (panel, launcher, listy).
//! - [`Viewport`], [`WindowHandle`] — pomocniki okna/obszaru (GTK4).
//! - Aliasy w duchu GTK4: [`Carousel`] = [`Pager`], [`ScrolledWindow`] = [`ScrollView`],
//!   [`StackSwitcher`] = [`ViewSwitcher`], [`Notebook`] = [`TabView`], [`ListBox`] = [`NavList`].
//! - [`Carousel`] (alias [`Pager`]), [`ToolbarView`], [`WindowTitle`], [`Clamp`], [`WrapBox`],
//!   [`BreakpointBin`], [`ButtonContent`] — pomocniki.
//!
//! Układ (retencyjne drzewo, `measure`/`allocate`): [`layout`] ([`Node`], [`Layout`], [`Constraints`]).
//!
//! ```no_run
//! use libluna_widgets::{PaintCtx, ViewSwitcher};
//! # fn demo(ctx: &mut PaintCtx, rect: libluna_core::Rect) {
//! let mut switcher = ViewSwitcher::new(["Lista", "Siatka"]);
//! switcher.paint(ctx, rect);
//! # }
//! ```

pub mod action_row;
pub mod assistant;
pub mod avatar;
pub mod banner;
pub mod bin;
pub mod bottom_sheet;
pub mod button;
pub mod buttons;
pub mod calendar;
pub mod card;
pub mod check_button;
pub mod clipboard;
pub mod code_view;
pub mod color_chooser;
pub mod column_view;
pub mod containers;
pub mod ctx;
pub mod dialogs;
pub mod display;
pub mod dropdown;
pub mod emoji_chooser;
pub mod entries;
pub mod entry;
pub mod expander;
pub mod flow_box;
pub mod header_bar;
pub mod helpers;
pub mod hover;
pub mod icon;
pub mod image;
pub mod infobar;
pub mod item_view;
pub mod layout;
pub mod media_controls;
pub mod menu;
pub mod menu_button;
pub mod nav_list;
pub mod navigation;
pub mod navigation_rail;
pub mod pager;
pub mod path_bar;
pub mod picture;
pub mod popover;
pub mod preferences;
pub mod radio_group;
pub mod rows;
pub mod scroll_view;
pub mod scrollbar;
pub mod search_bar;
pub mod search_entry;
pub mod shortcut_label;
pub mod slider;
pub mod spin_button;
pub mod spinner;
pub mod split_button;
pub mod status_page;
pub mod statusbar;
pub mod stepper;
pub mod switch;
pub mod tab_bar;
pub mod tab_view;
pub mod text_view;
pub mod toast;
pub mod toggle_group;
pub mod tooltip;
pub mod view_switcher;

pub use action_row::ActionRow;
pub use assistant::Assistant;
pub use avatar::Avatar;
pub use banner::{Banner, BannerKind, BannerRects};
pub use bin::{Bin, BinAlign};
pub use bottom_sheet::BottomSheet;
pub use button::{Button, ButtonVariant};
pub use buttons::{ColorButton, LinkButton, ToggleButton};
pub use calendar::Calendar;
pub use card::Card;
pub use check_button::CheckButton;
pub use clipboard::{clipboard_command, TextClipboardCommand};
pub use code_view::{token_color, CodeMetrics, CodeView, Span, TokenKind};
pub use color_chooser::ColorChooser;
pub use column_view::{Column, ColumnView};
pub use containers::{
    ActionBar, CenterBox, Fixed, Grid, Paned, PopoverMenuBar, Stack, StackFrame, StackTransition,
    Viewport,
};
pub use ctx::PaintCtx;
pub use dialogs::{
    AboutDialog, AlertDialog, AlertRects, Dialog, DialogRects, PreferencesDialog, ShortcutsDialog,
};
pub use display::{
    AspectFrame, Frame, Label, LabelAlign, LevelBar, Orientation, Overlay, ProgressBar, Revealer,
    Separator,
};
pub use dropdown::DropDown;
pub use emoji_chooser::EmojiChooser;
pub use entries::{EditableLabel, PasswordEntry};
pub use entry::{Entry, EntryIcon, TextInput};
pub use expander::Expander;
pub use flow_box::FlowBox;
pub use header_bar::{ControlsLayout, ControlsSide, HeaderBar, TitleAlign, WindowControl};
pub use helpers::{
    BreakpointBin, ButtonContent, Clamp, ToolbarRects, ToolbarView, WindowHandle, WindowTitle,
    WindowTitleRects, WrapBox,
};
pub use hover::{hover_fill, Hover, Hoverable};
pub use icon::{paint_icon, paint_icon_themed, paint_named_icon, sidebar_icon, IconName};
pub use image::{Image, ImageIcon};
pub use infobar::{InfoBar, InfoBarKind, InfoBarRects};
pub use item_view::{Item, ItemKind, ItemView, ViewMode};
pub use layout::{Constraints, Layout, LayoutCtx, Node};
pub use media_controls::{MediaControls, MediaRects};
pub use menu::{Menu, MenuEntry, MenuRow};
pub use menu_button::MenuButton;
/// `ListBox` — nazwa [`NavList`] w duchu GTK4.
pub use nav_list::NavList as ListBox;
pub use nav_list::{NavEntry, NavList, NavRow};
pub use navigation::{NavigationSplitView, NavigationView, OverlaySplitView, Sidebar};
pub use navigation_rail::NavigationRail;
pub use pager::Pager;
/// `Carousel` — nazwa [`Pager`] w duchu libadwaita.
pub use pager::Pager as Carousel;
pub use path_bar::PathBar;
pub use picture::{ContentFit, Picture};
pub use popover::{Popover, PopoverPlacement};
pub use preferences::{PreferencesGroup, PreferencesPage};
pub use radio_group::RadioGroup;
pub use rows::{Row, RowRects, RowSuffix};
pub use scroll_view::ScrollView;
/// `ScrolledWindow` — nazwa [`ScrollView`] w duchu GTK4.
pub use scroll_view::ScrollView as ScrolledWindow;
pub use scrollbar::{Adjustment, Scrollbar};
pub use search_bar::SearchBar;
pub use search_entry::SearchEntry;
pub use shortcut_label::ShortcutLabel;
pub use slider::Slider;
pub use spin_button::{SpinArrow, SpinButton};
pub use spinner::Spinner;
pub use split_button::{SplitButton, SplitRects};
pub use status_page::{StatusIcon, StatusPage, StatusRects};
pub use statusbar::Statusbar;
pub use stepper::{Stepper, StepperSide};
pub use switch::Switch;
pub use tab_bar::{ScrollDir, Tab, TabBar};
pub use tab_view::TabView;
/// `Notebook` — nazwa [`TabView`] w duchu GTK4.
pub use tab_view::TabView as Notebook;
pub use text_view::TextView;
pub use toast::Toast;
pub use toggle_group::ToggleGroup;
pub use tooltip::Tooltip;
/// `StackSwitcher` — nazwa [`ViewSwitcher`] jako przełącznik stron stosu (GTK4).
pub use view_switcher::ViewSwitcher as StackSwitcher;
pub use view_switcher::ViewSwitcher;
