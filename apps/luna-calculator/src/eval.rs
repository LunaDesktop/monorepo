//! Parser i obliczanie wyrażeń kalkulatora.
//!
//! Obsługujemy liczby dziesiętne i w notacji naukowej (`1e-3`), operatory
//! `+ − × ÷ ^ mod`, postfiksowe `%` i `!`, nawiasy, stałe `π`/`e` oraz funkcje
//! trygonometryczne, logarytmy i pierwiastki. Tryb kąta (`DEG`/`RAD`) dotyczy
//! wyłącznie funkcji trygonometrycznych.

use std::f64::consts::{E, PI, TAU};
use std::fmt;

/// Maksymalne zagłębienie nawiasów i funkcji — chroni przed przepełnieniem stosu.
const MAX_DEPTH: usize = 64;

/// Jednostka kąta dla funkcji trygonometrycznych.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AngleMode {
    Degrees,
    Radians,
}

impl AngleMode {
    pub fn toggle(self) -> Self {
        match self {
            Self::Degrees => Self::Radians,
            Self::Radians => Self::Degrees,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Degrees => "DEG",
            Self::Radians => "RAD",
        }
    }
}

/// Błąd obliczenia. `key` jest angielskim kluczem tłumaczenia (`libluna-i18n`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalcError {
    key: &'static str,
}

impl CalcError {
    const fn new(key: &'static str) -> Self {
        Self { key }
    }

    pub fn key(self) -> &'static str {
        self.key
    }
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key)
    }
}

impl std::error::Error for CalcError {}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Percent,
    Bang,
    LeftParen,
    RightParen,
}

fn invalid() -> CalcError {
    CalcError::new("Invalid expression")
}

fn missing_paren() -> CalcError {
    CalcError::new("Missing parenthesis")
}

fn tokenize(input: &str) -> Result<Vec<Token>, CalcError> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }

        match c {
            '+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            '−' | '-' => {
                tokens.push(Token::Minus);
                i += 1;
            }
            '×' | '*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            '÷' | '/' => {
                tokens.push(Token::Slash);
                i += 1;
            }
            '^' => {
                tokens.push(Token::Caret);
                i += 1;
            }
            '%' => {
                tokens.push(Token::Percent);
                i += 1;
            }
            '!' => {
                tokens.push(Token::Bang);
                i += 1;
            }
            '(' => {
                tokens.push(Token::LeftParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RightParen);
                i += 1;
            }
            'π' => {
                tokens.push(Token::Ident("pi".to_owned()));
                i += 1;
            }
            '√' => {
                tokens.push(Token::Ident("sqrt".to_owned()));
                i += 1;
            }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                let mut seen_dot = false;
                while i < chars.len() {
                    let c = chars[i];
                    if c.is_ascii_digit() {
                        i += 1;
                    } else if c == '.' && !seen_dot {
                        seen_dot = true;
                        i += 1;
                    } else {
                        break;
                    }
                }

                // Notacja naukowa: `2e3`, `2e-3`. Samo `e` zostaje stałą Eulera,
                // więc `2e` czyta się jako niejawne mnożenie `2 × e`.
                if i < chars.len() && matches!(chars[i], 'e' | 'E') {
                    let mut j = i + 1;
                    if j < chars.len() && matches!(chars[j], '+' | '-' | '−') {
                        j += 1;
                    }
                    if j < chars.len() && chars[j].is_ascii_digit() {
                        i = j + 1;
                        while i < chars.len() && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                }

                let raw: String = chars[start..i].iter().collect::<String>().replace('−', "-");
                let value = raw.parse::<f64>().map_err(|_| invalid())?;
                tokens.push(Token::Number(value));
            }
            c if c.is_ascii_alphabetic() => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphabetic() || chars[i] == '_') {
                    i += 1;
                }
                let name: String = chars[start..i]
                    .iter()
                    .collect::<String>()
                    .to_ascii_lowercase();
                tokens.push(Token::Ident(name));
            }
            _ => return Err(CalcError::new("Unexpected character")),
        }
    }

    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    angle: AngleMode,
    depth: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>, angle: AngleMode) -> Self {
        Self {
            tokens,
            pos: 0,
            angle,
            depth: 0,
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos).cloned();
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    fn eat_right_paren(&mut self) -> bool {
        if matches!(self.peek(), Some(Token::RightParen)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn enter(&mut self) -> Result<(), CalcError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(CalcError::new("Expression is too complex"));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn parse_expr(&mut self) -> Result<f64, CalcError> {
        let mut value = self.parse_term()?;
        loop {
            match self.peek() {
                Some(Token::Plus) => {
                    self.pos += 1;
                    value += self.parse_term()?;
                }
                Some(Token::Minus) => {
                    self.pos += 1;
                    value -= self.parse_term()?;
                }
                _ => break,
            }
        }
        Ok(value)
    }

    fn parse_term(&mut self) -> Result<f64, CalcError> {
        let mut value = self.parse_unary()?;
        loop {
            match self.peek() {
                Some(Token::Star) => {
                    self.pos += 1;
                    value *= self.parse_unary()?;
                }
                Some(Token::Slash) => {
                    self.pos += 1;
                    let rhs = self.parse_unary()?;
                    if rhs == 0.0 {
                        return Err(CalcError::new("Division by zero"));
                    }
                    value /= rhs;
                }
                Some(Token::Ident(name)) if name == "mod" => {
                    self.pos += 1;
                    let rhs = self.parse_unary()?;
                    if rhs == 0.0 {
                        return Err(CalcError::new("Division by zero"));
                    }
                    value %= rhs;
                }
                // Niejawne mnożenie: `2π`, `3(4+5)`, `2sin(30)`.
                Some(Token::Number(_)) | Some(Token::Ident(_)) | Some(Token::LeftParen) => {
                    value *= self.parse_unary()?;
                }
                _ => break,
            }
        }
        Ok(value)
    }

    fn parse_unary(&mut self) -> Result<f64, CalcError> {
        match self.peek() {
            Some(Token::Plus) => {
                self.pos += 1;
                self.parse_unary()
            }
            Some(Token::Minus) => {
                self.pos += 1;
                Ok(-self.parse_unary()?)
            }
            _ => self.parse_power(),
        }
    }

    fn parse_power(&mut self) -> Result<f64, CalcError> {
        let base = self.parse_postfix()?;
        if matches!(self.peek(), Some(Token::Caret)) {
            self.pos += 1;
            // Prawostronna łączność: `2^3^2` = `2^(3^2)`.
            let exponent = self.parse_unary()?;
            Ok(base.powf(exponent))
        } else {
            Ok(base)
        }
    }

    fn parse_postfix(&mut self) -> Result<f64, CalcError> {
        let mut value = self.parse_primary()?;
        loop {
            match self.peek() {
                Some(Token::Percent) => {
                    self.pos += 1;
                    value /= 100.0;
                }
                Some(Token::Bang) => {
                    self.pos += 1;
                    value = factorial(value)?;
                }
                _ => break,
            }
        }
        Ok(value)
    }

    fn parse_primary(&mut self) -> Result<f64, CalcError> {
        let token = self.next().ok_or_else(invalid)?;
        match token {
            Token::Number(value) => Ok(value),
            Token::LeftParen => {
                self.enter()?;
                let value = self.parse_expr()?;
                self.leave();
                if !self.eat_right_paren() {
                    return Err(missing_paren());
                }
                Ok(value)
            }
            Token::Ident(name) => {
                if let Some(value) = constant(&name) {
                    return Ok(value);
                }
                if !matches!(self.peek(), Some(Token::LeftParen)) {
                    return Err(CalcError::new("Unknown function"));
                }
                self.pos += 1;
                self.enter()?;
                let argument = self.parse_expr()?;
                self.leave();
                if !self.eat_right_paren() {
                    return Err(missing_paren());
                }
                apply_function(&name, argument, self.angle)
            }
            _ => Err(invalid()),
        }
    }
}

fn constant(name: &str) -> Option<f64> {
    match name {
        "pi" => Some(PI),
        "e" => Some(E),
        "tau" => Some(TAU),
        _ => None,
    }
}

fn apply_function(name: &str, argument: f64, angle: AngleMode) -> Result<f64, CalcError> {
    let radians = match angle {
        AngleMode::Degrees => argument.to_radians(),
        AngleMode::Radians => argument,
    };
    let to_angle = |value: f64| match angle {
        AngleMode::Degrees => value.to_degrees(),
        AngleMode::Radians => value,
    };

    let value = match name {
        "sin" => radians.sin(),
        "cos" => radians.cos(),
        "tan" => radians.tan(),
        "asin" => to_angle(argument.asin()),
        "acos" => to_angle(argument.acos()),
        "atan" => to_angle(argument.atan()),
        "ln" => argument.ln(),
        "log" => argument.log10(),
        "log2" => argument.log2(),
        "sqrt" => argument.sqrt(),
        "cbrt" => argument.cbrt(),
        "abs" => argument.abs(),
        "exp" => argument.exp(),
        "round" => argument.round(),
        "floor" => argument.floor(),
        "ceil" => argument.ceil(),
        _ => return Err(CalcError::new("Unknown function")),
    };
    Ok(value)
}

fn factorial(value: f64) -> Result<f64, CalcError> {
    if value < 0.0 || value.fract().abs() > 1e-9 || value > 170.0 {
        return Err(CalcError::new("Factorial needs a non-negative integer"));
    }
    let n = value.round() as u64;
    let mut result = 1.0;
    for i in 2..=n {
        result *= i as f64;
    }
    Ok(result)
}

/// Oblicza wyrażenie. Zwraca błąd z kluczem tłumaczenia, gdy wyrażenie jest
/// niepełne, niepoprawne albo wynik nie jest skończoną liczbą.
pub fn evaluate(input: &str, angle: AngleMode) -> Result<f64, CalcError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(invalid());
    }
    let tokens = tokenize(trimmed)?;
    let mut parser = Parser::new(tokens, angle);
    let value = parser.parse_expr()?;
    if parser.pos != parser.tokens.len() {
        return Err(invalid());
    }
    if !value.is_finite() {
        return Err(CalcError::new("Result is not a number"));
    }
    Ok(value)
}

/// Formatuje wynik do czytelnego napisu: bez zbędnych zer, z notacją naukową
/// dla bardzo dużych i bardzo małych wartości.
pub fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_owned();
    }
    if value == 0.0 {
        return "0".to_owned();
    }

    let abs = value.abs();
    if abs >= 1e12 || abs < 1e-9 {
        let raw = format!("{value:.10e}");
        return tidy_scientific(&raw);
    }

    let mut text = format!("{value:.12}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" {
        "0".to_owned()
    } else {
        text
    }
}

fn tidy_scientific(raw: &str) -> String {
    let (mantissa, exponent) = raw.split_once('e').unwrap_or((raw, "0"));
    let mut mantissa = mantissa.to_owned();
    if mantissa.contains('.') {
        while mantissa.ends_with('0') {
            mantissa.pop();
        }
        if mantissa.ends_with('.') {
            mantissa.pop();
        }
    }
    let exponent = exponent.parse::<i32>().unwrap_or(0);
    format!("{mantissa}e{exponent}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deg(input: &str) -> f64 {
        evaluate(input, AngleMode::Degrees).expect("wyrażenie powinno się obliczyć")
    }

    fn rad(input: &str) -> f64 {
        evaluate(input, AngleMode::Radians).expect("wyrażenie powinno się obliczyć")
    }

    fn err(input: &str) -> &'static str {
        evaluate(input, AngleMode::Degrees)
            .expect_err("wyrażenie powinno zwrócić błąd")
            .key()
    }

    #[test]
    fn precedence_and_parentheses() {
        assert_eq!(deg("2+3*4"), 14.0);
        assert_eq!(deg("(2+3)*4"), 20.0);
        assert_eq!(deg("2^3^2"), 512.0);
        assert_eq!(deg("-2^2"), -4.0);
        assert_eq!(deg("2^-3"), 0.125);
    }

    #[test]
    fn symbols_from_the_keypad() {
        assert_eq!(deg("7×8−6÷3"), 54.0);
        assert_eq!(deg("50%"), 0.5);
        assert_eq!(deg("5!"), 120.0);
        assert_eq!(deg("3 mod 2"), 1.0);
    }

    #[test]
    fn implicit_multiplication() {
        assert_eq!(deg("2(3+4)"), 14.0);
        assert!((rad("2pi") - std::f64::consts::TAU).abs() < 1e-12);
        assert!((rad("2e") - 2.0 * std::f64::consts::E).abs() < 1e-12);
    }

    #[test]
    fn functions_and_angles() {
        assert!((deg("sin(30)") - 0.5).abs() < 1e-12);
        assert!((rad("sin(pi/2)") - 1.0).abs() < 1e-12);
        assert!((deg("cos(60)") - 0.5).abs() < 1e-12);
        assert!((deg("asin(0.5)") - 30.0).abs() < 1e-9);
        assert!((deg("log(1000)") - 3.0).abs() < 1e-12);
        assert!((deg("ln(e)") - 1.0).abs() < 1e-12);
        assert_eq!(deg("sqrt(9)+2^3"), 11.0);
        assert_eq!(deg("abs(-5)"), 5.0);
    }

    #[test]
    fn scientific_notation() {
        assert_eq!(deg("1e3"), 1000.0);
        assert_eq!(deg("1.5e-3"), 0.0015);
        assert_eq!(deg("2.5E2"), 250.0);
    }

    #[test]
    fn errors_are_actionable() {
        assert_eq!(err("1/0"), "Division by zero");
        assert_eq!(err("2+"), "Invalid expression");
        assert_eq!(err("sin"), "Unknown function");
        assert_eq!(err("(2+3"), "Missing parenthesis");
        assert_eq!(err("2.5!"), "Factorial needs a non-negative integer");
        assert_eq!(err("sqrt(-1)"), "Result is not a number");
    }

    #[test]
    fn formatting_is_human_friendly() {
        assert_eq!(format_number(0.1 + 0.2), "0.3");
        assert_eq!(format_number(42.0), "42");
        assert_eq!(format_number(-0.0), "0");
        assert_eq!(format_number(1.0 / 3.0), "0.333333333333");
        assert_eq!(format_number(1e13), "1e13");
        assert_eq!(format_number(1e-10), "1e-10");
    }
}
