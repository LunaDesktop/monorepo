//! Stan kalkulatora i pętla aplikacji.

use std::error::Error;

use libluna_core::{Point, Rect, ThemeSettings};
use libluna_i18n::{load_catalogs_from_dirs, preferred_locale, set_locale, tr};
use libluna_platform::{run as run_app, AppConfig, Frame};
use libluna_widgets::{AlertDialog, BottomSheet, Menu, MenuEntry, ScrollView, WindowControl};

use crate::eval::{evaluate, format_number, AngleMode};
use crate::history::History;
use crate::prefs::Prefs;

/// Tryb pracy kalkulatora.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Standard,
    Scientific,
}

impl Mode {
    pub const ALL: [Mode; 2] = [Mode::Standard, Mode::Scientific];

    pub fn index(self) -> usize {
        match self {
            Self::Standard => 0,
            Self::Scientific => 1,
        }
    }

    pub fn from_index(index: usize) -> Self {
        Self::ALL[index.min(Self::ALL.len() - 1)]
    }

    pub fn title_key(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Scientific => "Scientific",
        }
    }
}

/// Klawisz kalkulatora. Część klawiszy wstawia tekst, część wykonuje akcję.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Digit(u8),
    Dot,
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Percent,
    Factorial,
    OpenParen,
    CloseParen,
    Pi,
    Euler,
    Ans,
    Exp,
    TenPow,
    Mod,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Ln,
    Log,
    Sqrt,
    Abs,
    Square,
    Reciprocal,
    Equals,
    Clear,
    Backspace,
    Negate,
    Angle,
}

impl Key {
    /// Etykieta przycisku. `angle` wpływa tylko na klawisz `DEG`/`RAD`.
    pub fn label(self, angle: AngleMode) -> &'static str {
        match self {
            Self::Digit(0) => "0",
            Self::Digit(1) => "1",
            Self::Digit(2) => "2",
            Self::Digit(3) => "3",
            Self::Digit(4) => "4",
            Self::Digit(5) => "5",
            Self::Digit(6) => "6",
            Self::Digit(7) => "7",
            Self::Digit(8) => "8",
            Self::Digit(9) => "9",
            Self::Digit(_) => "?",
            Self::Dot => ".",
            Self::Add => "+",
            Self::Sub => "−",
            Self::Mul => "×",
            Self::Div => "÷",
            Self::Pow => "xʸ",
            Self::Percent => "%",
            Self::Factorial => "!",
            Self::OpenParen => "(",
            Self::CloseParen => ")",
            Self::Pi => "π",
            Self::Euler => "e",
            Self::Ans => "Ans",
            Self::Exp => "EXP",
            Self::TenPow => "10ˣ",
            Self::Mod => "mod",
            Self::Sin => "sin",
            Self::Cos => "cos",
            Self::Tan => "tan",
            Self::Asin => "asin",
            Self::Acos => "acos",
            Self::Atan => "atan",
            Self::Ln => "ln",
            Self::Log => "log",
            Self::Sqrt => "√",
            Self::Abs => "abs",
            Self::Square => "x²",
            Self::Reciprocal => "1/x",
            Self::Equals => "=",
            Self::Clear => "C",
            Self::Backspace => "⌫",
            Self::Negate => "±",
            Self::Angle => angle.label(),
        }
    }

    /// Czy klawisz rozpoczyna nową wartość (po `=` czyści poprzedni wynik).
    fn starts_value(self) -> bool {
        matches!(
            self,
            Self::Digit(_)
                | Self::Dot
                | Self::Pi
                | Self::Euler
                | Self::Ans
                | Self::OpenParen
                | Self::Sin
                | Self::Cos
                | Self::Tan
                | Self::Asin
                | Self::Acos
                | Self::Atan
                | Self::Ln
                | Self::Log
                | Self::Sqrt
                | Self::Abs
        )
    }

    /// Tekst wstawiany do wyrażenia (dla klawiszy, które nie mają własnej akcji).
    fn insertion(self) -> Option<&'static str> {
        match self {
            Self::Digit(0) => Some("0"),
            Self::Digit(1) => Some("1"),
            Self::Digit(2) => Some("2"),
            Self::Digit(3) => Some("3"),
            Self::Digit(4) => Some("4"),
            Self::Digit(5) => Some("5"),
            Self::Digit(6) => Some("6"),
            Self::Digit(7) => Some("7"),
            Self::Digit(8) => Some("8"),
            Self::Digit(9) => Some("9"),
            Self::Digit(_) => None,
            Self::Dot => Some("."),
            Self::Add => Some("+"),
            Self::Sub => Some("−"),
            Self::Mul => Some("×"),
            Self::Div => Some("÷"),
            Self::Pow => Some("^"),
            Self::Percent => Some("%"),
            Self::Factorial => Some("!"),
            Self::OpenParen => Some("("),
            Self::CloseParen => Some(")"),
            Self::Pi => Some("π"),
            Self::Euler => Some("e"),
            Self::Exp => None,
            Self::TenPow => Some("10^"),
            Self::Mod => Some(" mod "),
            Self::Sin => Some("sin("),
            Self::Cos => Some("cos("),
            Self::Tan => Some("tan("),
            Self::Asin => Some("asin("),
            Self::Acos => Some("acos("),
            Self::Atan => Some("atan("),
            Self::Ln => Some("ln("),
            Self::Log => Some("log("),
            Self::Sqrt => Some("√("),
            Self::Abs => Some("abs("),
            Self::Square => Some("^2"),
            Self::Reciprocal => Some("^-1"),
            Self::Ans
            | Self::Equals
            | Self::Clear
            | Self::Backspace
            | Self::Negate
            | Self::Angle => None,
        }
    }

    fn is_postfix(self) -> bool {
        matches!(
            self,
            Self::Percent | Self::Factorial | Self::Square | Self::Reciprocal | Self::TenPow
        )
    }
}

/// Akcja interfejsu zebrana podczas rysowania klatki.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Mode(Mode),
    Press(Key),
    History(usize),
    ClearHistory,
}

/// Pozycja menu głównego.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    History,
    Settings,
    About,
}

/// Otwarte menu pod przyciskiem „burger”.
pub struct MenuState {
    pub menu: Menu,
    pub anchor: Rect,
    pub actions: Vec<Option<MenuAction>>,
}

/// Stan aplikacji.
pub struct App {
    pub mode: Mode,
    pub prefs: Prefs,
    /// Czy zapisywać preferencje (w testach wyłączone).
    pub persist_prefs: bool,
    pub menu: Option<MenuState>,
    pub settings_open: bool,
    pub about: Option<AlertDialog>,
    pub history: History,
    pub history_sheet: BottomSheet,
    pub history_scroll: ScrollView,
    pub expression: String,
    pub result: Option<f64>,
    pub last_result: Option<f64>,
    pub error: Option<&'static str>,
    pub angle: AngleMode,
    /// Ostatnia akcja to `=`: kolejny klawisz wartości zaczyna nowe wyrażenie.
    pub just_evaluated: bool,
    pub pointer: Option<Point>,
    pub controls_hover: Option<WindowControl>,
    pub pending_close: bool,
    pub pending_minimize: bool,
    pub pending_maximize: bool,
    pub pending_move: bool,
    pub clipboard_text: Option<String>,
    pub pending_paste: bool,
    /// Prostokąty i akcje z ostatniej klatki.
    pub hits: Vec<(Action, Rect)>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            mode: Mode::Standard,
            prefs: Prefs::default(),
            persist_prefs: false,
            menu: None,
            settings_open: false,
            about: None,
            history: History::empty(),
            history_sheet: {
                let mut sheet = BottomSheet::new();
                sheet.set_height(300.0);
                sheet.set_title(Some(tr("History")));
                sheet
            },
            history_scroll: ScrollView::new(),
            expression: String::new(),
            result: None,
            last_result: None,
            error: None,
            angle: AngleMode::Degrees,
            just_evaluated: false,
            pointer: None,
            controls_hover: None,
            pending_close: false,
            pending_minimize: false,
            pending_maximize: false,
            pending_move: false,
            clipboard_text: None,
            pending_paste: false,
            hits: Vec::new(),
        }
    }

    pub fn with_prefs(prefs: Prefs) -> Self {
        let mut app = Self::new();
        app.prefs = prefs;
        app.angle = prefs.angle;
        app.mode = prefs.start_mode;
        app
    }

    /// Szerokość okna dobrana do zawartości trybu.
    pub fn preferred_size(&self) -> (u32, u32) {
        match self.mode {
            Mode::Standard => (640, 680),
            Mode::Scientific => (900, 680),
        }
    }

    pub fn window_title(&self) -> String {
        tr("Calculator")
    }

    /// Zapisuje preferencje tylko wtedy, gdy aplikacja działa normalnie.
    pub fn save_prefs(&self) {
        if self.persist_prefs {
            self.prefs.save();
        }
    }

    pub fn set_angle(&mut self, angle: AngleMode) {
        self.angle = angle;
        self.prefs.angle = angle;
        self.save_prefs();
    }

    pub fn set_start_mode(&mut self, mode: Mode) {
        self.prefs.start_mode = mode;
        self.save_prefs();
    }

    /// Otwiera menu główne pod podanym przyciskiem.
    pub fn open_menu(&mut self, anchor: Rect) {
        let entries = vec![
            MenuEntry::action(tr("History")).shortcut("Ctrl+H"),
            MenuEntry::action(tr("Settings")).shortcut("Ctrl+,"),
            MenuEntry::separator(),
            MenuEntry::action(tr("About Calculator")).shortcut("F1"),
        ];
        self.menu = Some(MenuState {
            menu: Menu::new(entries),
            anchor,
            actions: vec![
                Some(MenuAction::History),
                Some(MenuAction::Settings),
                None,
                Some(MenuAction::About),
            ],
        });
    }

    pub fn run_menu(&mut self, index: usize) {
        let action = self
            .menu
            .as_ref()
            .and_then(|state| state.actions.get(index).copied().flatten());
        self.menu = None;
        match action {
            Some(MenuAction::History) => self.open_history(),
            Some(MenuAction::Settings) => self.open_settings(),
            Some(MenuAction::About) => self.show_about(),
            None => {}
        }
    }

    pub fn open_history(&mut self) {
        self.menu = None;
        self.history_sheet.set_title(Some(tr("History")));
        self.history_sheet.set_open(true);
    }

    pub fn close_history(&mut self) {
        self.history_sheet.set_open(false);
    }

    pub fn use_history(&mut self, index: usize) {
        let Some(result) = self
            .history
            .entries()
            .get(index)
            .map(|entry| entry.result.clone())
        else {
            return;
        };
        self.close_history();
        self.insert_value(&result);
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.history_scroll = ScrollView::new();
    }

    pub fn open_settings(&mut self) {
        self.menu = None;
        self.close_history();
        self.settings_open = true;
    }

    pub fn show_about(&mut self) {
        self.menu = None;
        self.close_history();
        let body = format!(
            "Luna Calculator {}\n{}\n\n{}\n\n© 2026 Luna Project · GPL-3.0-or-later",
            env!("CARGO_PKG_VERSION"),
            tr("Standard and scientific calculator for the Luna desktop."),
            tr("All calculations run locally. No history is saved and no network requests are made."),
        );
        self.about = Some(
            AlertDialog::new(tr("About Calculator"), body)
                .width(480.0)
                .button(tr("Done")),
        );
    }

    /// Podgląd wyniku podczas pisania (bez zatwierdzania `=`).
    pub fn preview(&self) -> Option<String> {
        if self.just_evaluated || self.expression.trim().is_empty() {
            return None;
        }
        let value = evaluate(&self.expression, self.angle).ok()?;
        let text = format_number(value);
        (text != self.expression).then_some(text)
    }

    /// Napis do schowka: wynik, a gdy go nie ma — bieżące wyrażenie.
    pub fn copy_text(&self) -> String {
        self.result
            .or(self.last_result)
            .map(format_number)
            .unwrap_or_else(|| self.expression.clone())
    }

    pub fn press(&mut self, key: Key) {
        match key {
            Key::Clear => self.clear(),
            Key::Backspace => self.backspace(),
            Key::Equals => self.equals(),
            Key::Negate => self.negate(),
            Key::Angle => {
                let angle = self.angle.toggle();
                self.set_angle(angle);
            }
            Key::Ans => {
                if let Some(value) = self.last_result {
                    self.insert_value(&format_number(value));
                }
            }
            Key::Exp => self.insert_exponent(),
            _ => {
                let Some(text) = key.insertion() else {
                    return;
                };
                if key.starts_value() {
                    self.insert_value(text);
                } else if key.is_postfix() {
                    self.insert_postfix(text);
                } else {
                    self.insert_operator(text);
                }
            }
        }
    }

    fn clear(&mut self) {
        self.expression.clear();
        self.result = None;
        self.error = None;
        self.just_evaluated = false;
    }

    fn equals(&mut self) {
        if self.just_evaluated || self.expression.trim().is_empty() {
            return;
        }
        let input = self.expression.clone();
        match evaluate(&self.expression, self.angle) {
            Ok(value) => {
                let text = format_number(value);
                self.history.add(&input, &text);
                self.expression = text;
                self.last_result = Some(value);
                self.result = Some(value);
                self.error = None;
                self.just_evaluated = true;
            }
            Err(err) => {
                self.error = Some(err.key());
                self.result = None;
                self.just_evaluated = false;
            }
        }
    }

    fn negate(&mut self) {
        self.error = None;
        self.result = None;
        self.just_evaluated = false;
        if self.expression.is_empty() {
            self.expression.push('−');
        } else if let Some(inner) = self
            .expression
            .strip_prefix("−(")
            .and_then(|rest| rest.strip_suffix(')'))
        {
            self.expression = inner.to_owned();
        } else if let Some(rest) = self.expression.strip_prefix('−').map(str::to_owned) {
            if rest.contains('−') {
                self.expression = format!("−({})", self.expression);
            } else {
                self.expression = rest;
            }
        } else {
            self.expression = format!("−({})", self.expression);
        }
    }

    fn backspace(&mut self) {
        self.error = None;
        self.result = None;
        self.just_evaluated = false;
        const SUFFIXES: [&str; 15] = [
            "asin(", "acos(", "atan(", "×10^", "sin(", "cos(", "tan(", "sqrt(", "abs(", "ln(",
            "log(", " mod ", "10^", "√(", "^-1",
        ];
        for suffix in SUFFIXES {
            if self.expression.ends_with(suffix) {
                self.expression
                    .truncate(self.expression.len() - suffix.len());
                return;
            }
        }
        self.expression.pop();
    }

    fn insert_value(&mut self, text: &str) {
        if self.just_evaluated || self.error.is_some() {
            self.expression.clear();
            self.result = None;
            self.error = None;
            self.just_evaluated = false;
        }
        if needs_multiplication(&self.expression, text) {
            self.expression.push('×');
        }
        self.expression.push_str(text);
        self.result = None;
        self.error = None;
    }

    fn insert_postfix(&mut self, text: &str) {
        self.error = None;
        self.result = None;
        if self.expression.trim().is_empty() {
            if text == "10^" {
                self.expression.push_str(text);
                self.just_evaluated = false;
                return;
            }
            let Some(value) = self.last_result else {
                return;
            };
            self.expression = format_number(value);
        }
        if text == "10^" {
            let last = self.expression.chars().last();
            if matches!(last, Some('0'..='9' | ')' | 'π' | 'e' | '%' | '!')) {
                self.expression.push('×');
            }
        }
        self.just_evaluated = false;
        self.expression.push_str(text);
    }

    /// `EXP` dopisuje wykładnik do bieżącej liczby (`5` → `5e` → `5e3`).
    /// Po nawiasie lub operatorze przechodzi na jawne `×10^`.
    fn insert_exponent(&mut self) {
        self.error = None;
        self.result = None;
        if self.expression.trim().is_empty() {
            if let Some(value) = self.last_result {
                self.expression = format_number(value);
            } else {
                self.expression.push_str("10^");
                self.just_evaluated = false;
                return;
            }
        }
        self.just_evaluated = false;
        match self.expression.chars().last() {
            Some(last) if last.is_ascii_digit() || last == '.' => self.expression.push('e'),
            Some('e') => self.expression.push_str("×10^"),
            Some('+' | '−' | '×' | '÷' | '^' | '(') => self.expression.push_str("10^"),
            _ => self.expression.push_str("×10^"),
        }
    }

    fn insert_operator(&mut self, text: &str) {
        self.error = None;
        self.result = None;
        if self.expression.trim().is_empty() {
            if text == "−" {
                self.expression.push('−');
                self.just_evaluated = false;
            } else if let Some(value) = self.last_result {
                self.expression = format_number(value);
                self.expression.push_str(text);
                self.just_evaluated = false;
            }
            return;
        }
        self.just_evaluated = false;

        let last = self.expression.chars().last().unwrap_or(' ');
        let trailing_operator = matches!(last, '+' | '−' | '×' | '÷' | '^');
        if trailing_operator {
            // `5×−2` ma sens (minus jednoargumentowy), ale `5+×2` już nie.
            if !(text == "−" && matches!(last, '×' | '÷' | '^')) {
                self.expression.pop();
            }
        }
        self.expression.push_str(text);
    }

    /// Wstawia tekst z klawiatury lub schowka, filtrując niedozwolone znaki.
    pub fn insert_text(&mut self, input: &str) {
        let mut clean = String::new();
        for c in input.chars() {
            let c = match c {
                '*' => '×',
                '/' => '÷',
                '-' => '−',
                '×' | '÷' | '−' | '+' | '^' | '%' | '!' | '(' | ')' | '.' | 'π' | '√' => c,
                c if c.is_ascii_digit() => c,
                c if c.is_ascii_alphabetic() => c.to_ascii_lowercase(),
                _ => continue,
            };
            clean.push(c);
        }
        if clean.is_empty() {
            return;
        }

        if self.just_evaluated || self.error.is_some() {
            let first = clean.chars().next().unwrap_or(' ');
            let starts_new = !matches!(first, '+' | '−' | '×' | '÷' | '^' | '%' | '!');
            if starts_new || self.error.is_some() {
                self.expression.clear();
            }
            self.error = None;
            self.just_evaluated = false;
        }

        // `e` po liczbie jest stałą Eulera, a `1e3` — notacją naukową; parser
        // rozróżnia je po obecności cyfr po `e`. Nie dokładamy więc `×`.
        self.expression.push_str(&clean);
        self.result = None;
        self.error = None;
    }
}

fn needs_multiplication(expression: &str, text: &str) -> bool {
    let Some(last) = expression.chars().last() else {
        return false;
    };
    let Some(first) = text.chars().next() else {
        return false;
    };
    // Cyfra lub kropka kontynuuje bieżącą liczbę (`12` + `3`, `1` + `.5`),
    // a po operatorze lub nawiasie otwierającym zaczyna nową wartość.
    if first.is_ascii_digit() || first == '.' {
        // `5e` + `3` to notacja naukowa (`5e3`), nie mnożenie.
        return matches!(last, ')' | 'π' | '%' | '!');
    }
    matches!(last, '0'..='9' | ')' | 'π' | 'e' | '%' | '!')
}

/// Uruchamia aplikację.
pub fn run() -> Result<(), Box<dyn Error>> {
    set_locale(preferred_locale());
    load_catalogs_from_dirs();

    let mut app = App::with_prefs(Prefs::load());
    app.persist_prefs = true;
    app.history = History::load();
    if std::env::args().any(|arg| arg == "--scientific") {
        app.mode = Mode::Scientific;
    }

    let settings = ThemeSettings::load();
    let system_dark = libluna_core::appearance::system_dark();
    let (width, height) = app.preferred_size();
    let config = AppConfig::new(tr("Calculator"))
        .app_id("eu.luna.Calculator")
        .initial_size(width, height)
        .settings(settings)
        .system_dark(system_dark)
        .escape_to_exit(false);

    let mut last_title = String::new();
    run_app(config, move |frame: &mut Frame| {
        let theme = frame.settings().resolve_themed(frame.system_dark());
        crate::ui::handle_events(&mut app, frame.events, frame.size, &theme, frame.text);

        let title = app.window_title();
        if title != last_title {
            frame.set_title(title.clone());
            last_title = title;
        }

        crate::ui::paint(&mut app, frame.scene, frame.size, &theme, frame.text);

        if let Some(text) = app.clipboard_text.take() {
            frame.copy_to_clipboard(text);
        }
        if app.pending_paste {
            app.pending_paste = false;
            frame.paste_from_clipboard();
        }
        if app.pending_close {
            app.pending_close = false;
            app.save_prefs();
            frame.close();
        }
        if app.pending_minimize {
            app.pending_minimize = false;
            frame.minimize();
        }
        if app.pending_maximize {
            app.pending_maximize = false;
            frame.toggle_maximize();
        }
        if app.pending_move {
            app.pending_move = false;
            frame.begin_move();
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_keys(app: &mut App, keys: &[Key]) {
        for key in keys {
            app.press(*key);
        }
    }

    #[test]
    fn standard_calculation() {
        let mut app = App::new();
        type_keys(
            &mut app,
            &[
                Key::Digit(2),
                Key::Add,
                Key::Digit(3),
                Key::Mul,
                Key::Digit(4),
                Key::Equals,
            ],
        );
        assert_eq!(app.expression, "14");
        assert_eq!(app.last_result, Some(14.0));
        assert!(app.just_evaluated);
    }

    #[test]
    fn operator_after_equals_continues_result() {
        let mut app = App::new();
        type_keys(
            &mut app,
            &[Key::Digit(5), Key::Add, Key::Digit(5), Key::Equals],
        );
        app.press(Key::Mul);
        app.press(Key::Digit(2));
        app.press(Key::Equals);
        assert_eq!(app.expression, "20");
    }

    #[test]
    fn digit_after_equals_starts_new_expression() {
        let mut app = App::new();
        type_keys(&mut app, &[Key::Digit(5), Key::Equals, Key::Digit(7)]);
        assert_eq!(app.expression, "7");
        assert!(!app.just_evaluated);
    }

    #[test]
    fn scientific_keys_and_angle_toggle() {
        let mut app = App::new();
        type_keys(
            &mut app,
            &[
                Key::Sin,
                Key::Digit(3),
                Key::Digit(0),
                Key::CloseParen,
                Key::Equals,
            ],
        );
        assert_eq!(app.expression, "0.5");

        app.press(Key::Angle);
        assert_eq!(app.angle, AngleMode::Radians);
        app.press(Key::Clear);
        type_keys(
            &mut app,
            &[
                Key::Sin,
                Key::Pi,
                Key::Div,
                Key::Digit(2),
                Key::CloseParen,
                Key::Equals,
            ],
        );
        assert_eq!(app.expression, "1");
    }

    #[test]
    fn postfix_and_constants() {
        let mut app = App::new();
        type_keys(&mut app, &[Key::Digit(5), Key::Factorial, Key::Equals]);
        assert_eq!(app.expression, "120");

        app.press(Key::Clear);
        type_keys(&mut app, &[Key::Pi, Key::Equals]);
        assert_eq!(app.expression, "3.14159265359");
    }

    #[test]
    fn backspace_removes_whole_function() {
        let mut app = App::new();
        type_keys(&mut app, &[Key::Sin, Key::Digit(3)]);
        app.press(Key::Backspace);
        assert_eq!(app.expression, "sin(");
        app.press(Key::Backspace);
        assert_eq!(app.expression, "");
    }

    #[test]
    fn negate_wraps_and_unwraps() {
        let mut app = App::new();
        type_keys(&mut app, &[Key::Digit(5), Key::Negate]);
        assert_eq!(app.expression, "−(5)");
        app.press(Key::Negate);
        assert_eq!(app.expression, "5");
    }

    #[test]
    fn errors_are_shown_until_next_key() {
        let mut app = App::new();
        type_keys(
            &mut app,
            &[Key::Digit(1), Key::Div, Key::Digit(0), Key::Equals],
        );
        assert_eq!(app.error, Some("Division by zero"));
        app.press(Key::Digit(2));
        assert_eq!(app.error, None);
        assert_eq!(app.expression, "2");
    }

    #[test]
    fn keyboard_text_is_filtered_and_normalized() {
        let mut app = App::new();
        app.insert_text("2*3+4");
        assert_eq!(app.expression, "2×3+4");
        app.press(Key::Equals);
        assert_eq!(app.expression, "10");
        app.insert_text("abc");
        assert_eq!(app.expression, "abc");
    }

    #[test]
    fn ans_inserts_last_result() {
        let mut app = App::new();
        type_keys(
            &mut app,
            &[Key::Digit(9), Key::Add, Key::Digit(1), Key::Equals],
        );
        app.press(Key::Ans);
        assert_eq!(app.expression, "10");
        app.press(Key::Add);
        app.press(Key::Ans);
        app.press(Key::Equals);
        assert_eq!(app.expression, "20");
    }

    #[test]
    fn exp_enters_scientific_notation() {
        let mut app = App::new();
        type_keys(
            &mut app,
            &[Key::Digit(5), Key::Exp, Key::Digit(3), Key::Equals],
        );
        assert_eq!(app.expression, "5000");
    }

    #[test]
    fn ten_pow_multiplies_current_expression() {
        let mut app = App::new();
        type_keys(
            &mut app,
            &[Key::Digit(5), Key::TenPow, Key::Digit(2), Key::Equals],
        );
        assert_eq!(app.expression, "500");
    }

    #[test]
    fn menu_actions_open_settings_and_about() {
        let mut app = App::new();
        let anchor = Rect::from_xywh(700.0, 8.0, 30.0, 30.0);
        app.open_menu(anchor);
        app.run_menu(1);
        assert!(app.settings_open);
        assert!(app.menu.is_none());

        app.open_menu(anchor);
        app.run_menu(3);
        assert!(app.about.is_some());
        assert!(app.menu.is_none());
    }

    #[test]
    fn preferred_width_follows_mode() {
        let mut app = App::new();
        assert_eq!(app.preferred_size(), (640, 680));
        app.mode = Mode::Scientific;
        assert_eq!(app.preferred_size(), (900, 680));
    }

    #[test]
    fn history_and_reuse() {
        let mut app = App::new();
        type_keys(
            &mut app,
            &[Key::Digit(2), Key::Add, Key::Digit(2), Key::Equals],
        );
        assert_eq!(app.history.len(), 1);
        app.press(Key::Clear);
        app.use_history(0);
        assert_eq!(app.expression, "4");
        app.clear_history();
        assert!(app.history.is_empty());
    }

    #[test]
    fn prefs_apply_on_start() {
        let prefs = crate::prefs::Prefs {
            angle: AngleMode::Radians,
            start_mode: Mode::Scientific,
        };
        let app = App::with_prefs(prefs);
        assert_eq!(app.angle, AngleMode::Radians);
        assert_eq!(app.mode, Mode::Scientific);
    }

    #[test]
    fn preview_is_available_while_typing() {
        let mut app = App::new();
        type_keys(
            &mut app,
            &[
                Key::Digit(2),
                Key::Add,
                Key::Digit(3),
                Key::Mul,
                Key::Digit(4),
            ],
        );
        assert_eq!(app.preview().as_deref(), Some("14"));
    }
}
