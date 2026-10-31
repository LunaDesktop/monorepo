//! Prosty, bezstanowy highlighter składni.
//!
//! Nie jest to pełny parser — leksery są celowo płaskie (słowa kluczowe,
//! komentarze, napisy, liczby, operatory). Wystarczają do czytelnego
//! podświetlania i są w pełni testowalne bez okna.

use std::path::Path;

use libluna_widgets::{Span, TokenKind};

/// Obsługiwane języki i ich nazwy w pasku statusu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Toml,
    Json,
    Markdown,
    Shell,
    C,
    Cpp,
    Python,
    Html,
    Xml,
    Css,
    JavaScript,
    TypeScript,
    Php,
    Yaml,
    Plain,
}

impl Language {
    pub fn from_path(path: &Path) -> Self {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        Self::from_name(name)
    }

    pub fn from_name(name: &str) -> Self {
        let name = name.to_lowercase();
        let extension = name.rsplit('.').next().unwrap_or("");
        match extension {
            "rs" => Self::Rust,
            "toml" => Self::Toml,
            "json" => Self::Json,
            "md" | "markdown" | "mdown" => Self::Markdown,
            "sh" | "bash" | "zsh" | "fish" | "ksh" => Self::Shell,
            "c" | "h" => Self::C,
            "cc" | "cpp" | "cxx" | "hpp" | "hxx" => Self::Cpp,
            "py" | "pyw" => Self::Python,
            "html" | "htm" | "xhtml" => Self::Html,
            "xml" | "svg" | "xsl" | "xslt" | "plist" => Self::Xml,
            "css" | "scss" | "sass" | "less" => Self::Css,
            "js" | "mjs" | "cjs" | "jsx" => Self::JavaScript,
            "ts" | "mts" | "cts" | "tsx" => Self::TypeScript,
            "php" | "phtml" => Self::Php,
            "yaml" | "yml" => Self::Yaml,
            _ => {
                if name == "makefile" || name == "dockerfile" {
                    Self::Shell
                } else if name == "cargo.lock" {
                    Self::Toml
                } else {
                    Self::Plain
                }
            }
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Toml => "TOML",
            Self::Json => "JSON",
            Self::Markdown => "Markdown",
            Self::Shell => "Shell",
            Self::C => "C",
            Self::Cpp => "C++",
            Self::Python => "Python",
            Self::Html => "HTML",
            Self::Xml => "XML",
            Self::Css => "CSS",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Php => "PHP",
            Self::Yaml => "YAML",
            Self::Plain => "Plain text",
        }
    }
}

#[derive(Default)]
struct Rules {
    keywords: &'static [&'static str],
    types: &'static [&'static str],
    constants: &'static [&'static str],
    line_comments: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    nested_comments: bool,
    quotes: &'static [char],
    triple_quotes: bool,
    char_literal: bool,
    rust_raw: bool,
    preprocessor: bool,
    shell_vars: bool,
    uppercase_types: bool,
    /// `@media`, `@import` itp. — słowo po małpie jako słowo kluczowe.
    at_keywords: bool,
    /// Znaczniki `<?php` i `?>`.
    php_tags: bool,
    /// Znak po identyfikatorze, który czyni z niego klucz (TOML `=`, YAML/CSS `:`).
    key_separator: Option<char>,
}

fn rules(language: Language) -> Rules {
    match language {
        Language::Rust => Rules {
            keywords: &[
                "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else",
                "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match",
                "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct",
                "super", "trait", "true", "type", "unsafe", "use", "where", "while",
            ],
            types: &[
                "bool", "char", "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "str",
                "u8", "u16", "u32", "u64", "u128", "usize",
            ],
            constants: &["true", "false", "None", "Some", "Ok", "Err", "Self"],
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            nested_comments: true,
            quotes: &['"', '\''],
            triple_quotes: false,
            char_literal: true,
            rust_raw: true,
            preprocessor: false,
            shell_vars: false,
            uppercase_types: true,
            ..Rules::default()
        },
        Language::Toml => Rules {
            keywords: &["true", "false"],
            types: &[],
            constants: &["true", "false"],
            line_comments: &["#"],
            block_comment: None,
            nested_comments: false,
            quotes: &['"', '\''],
            triple_quotes: true,
            char_literal: false,
            rust_raw: false,
            preprocessor: false,
            shell_vars: false,
            uppercase_types: false,
            key_separator: Some('='),
            ..Rules::default()
        },
        Language::Json => Rules {
            keywords: &[],
            types: &[],
            constants: &["true", "false", "null"],
            line_comments: &[],
            block_comment: None,
            nested_comments: false,
            quotes: &['"'],
            triple_quotes: false,
            char_literal: false,
            rust_raw: false,
            preprocessor: false,
            shell_vars: false,
            uppercase_types: false,
            ..Rules::default()
        },
        Language::Shell => Rules {
            keywords: &[
                "case", "do", "done", "elif", "else", "esac", "fi", "for", "function", "if", "in",
                "select", "then", "until", "while", "export", "local", "readonly", "return", "set",
                "unset",
            ],
            types: &[],
            constants: &["true", "false"],
            line_comments: &["#"],
            block_comment: None,
            nested_comments: false,
            quotes: &['"', '\''],
            triple_quotes: false,
            char_literal: false,
            rust_raw: false,
            preprocessor: false,
            shell_vars: true,
            uppercase_types: false,
            ..Rules::default()
        },
        Language::C => Rules {
            keywords: &[
                "auto", "break", "case", "const", "continue", "default", "do", "else", "enum",
                "extern", "for", "goto", "if", "inline", "register", "restrict", "return",
                "sizeof", "static", "struct", "switch", "typedef", "union", "volatile", "while",
            ],
            types: &[
                "char", "double", "float", "int", "long", "short", "signed", "unsigned", "void",
                "_Bool", "size_t", "ssize_t", "int8_t", "int16_t", "int32_t", "int64_t", "uint8_t",
                "uint16_t", "uint32_t", "uint64_t", "FILE", "NULL",
            ],
            constants: &["NULL", "true", "false"],
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            nested_comments: false,
            quotes: &['"', '\''],
            triple_quotes: false,
            char_literal: true,
            rust_raw: false,
            preprocessor: true,
            shell_vars: false,
            uppercase_types: false,
            ..Rules::default()
        },
        Language::Cpp => Rules {
            keywords: &[
                "alignas",
                "alignof",
                "and",
                "asm",
                "auto",
                "break",
                "case",
                "catch",
                "class",
                "concept",
                "const",
                "consteval",
                "constexpr",
                "continue",
                "co_await",
                "co_return",
                "co_yield",
                "decltype",
                "default",
                "delete",
                "do",
                "else",
                "enum",
                "explicit",
                "export",
                "extern",
                "false",
                "for",
                "friend",
                "goto",
                "if",
                "inline",
                "mutable",
                "namespace",
                "new",
                "noexcept",
                "not",
                "nullptr",
                "operator",
                "or",
                "private",
                "protected",
                "public",
                "register",
                "requires",
                "return",
                "sizeof",
                "static",
                "struct",
                "switch",
                "template",
                "this",
                "throw",
                "true",
                "try",
                "typedef",
                "typename",
                "union",
                "using",
                "virtual",
                "void",
                "volatile",
                "while",
            ],
            types: &[
                "bool", "char", "double", "float", "int", "long", "short", "signed", "unsigned",
                "string", "size_t", "int8_t", "int16_t", "int32_t", "int64_t", "uint8_t",
                "uint16_t", "uint32_t", "uint64_t",
            ],
            constants: &["nullptr", "true", "false", "NULL"],
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            nested_comments: false,
            quotes: &['"', '\''],
            triple_quotes: false,
            char_literal: true,
            rust_raw: false,
            preprocessor: true,
            shell_vars: false,
            uppercase_types: false,
            ..Rules::default()
        },
        Language::Python => Rules {
            keywords: &[
                "and", "as", "assert", "async", "await", "break", "class", "continue", "def",
                "del", "elif", "else", "except", "False", "finally", "for", "from", "global", "if",
                "import", "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise",
                "return", "True", "try", "while", "with", "yield",
            ],
            types: &[
                "bool",
                "bytes",
                "dict",
                "float",
                "frozenset",
                "int",
                "list",
                "object",
                "set",
                "str",
                "tuple",
            ],
            constants: &["True", "False", "None"],
            line_comments: &["#"],
            block_comment: None,
            nested_comments: false,
            quotes: &['"', '\''],
            triple_quotes: true,
            char_literal: false,
            rust_raw: false,
            preprocessor: false,
            shell_vars: false,
            uppercase_types: false,
            ..Rules::default()
        },
        Language::Html | Language::Xml => Rules::default(),
        Language::Css => Rules {
            block_comment: Some(("/*", "*/")),
            quotes: &['"', '\''],
            at_keywords: true,
            key_separator: Some(':'),
            ..Rules::default()
        },
        Language::JavaScript => Rules {
            keywords: &[
                "async",
                "await",
                "break",
                "case",
                "catch",
                "class",
                "const",
                "continue",
                "debugger",
                "default",
                "delete",
                "do",
                "else",
                "export",
                "extends",
                "finally",
                "for",
                "from",
                "function",
                "get",
                "if",
                "import",
                "in",
                "instanceof",
                "let",
                "new",
                "of",
                "return",
                "set",
                "static",
                "super",
                "switch",
                "this",
                "throw",
                "try",
                "typeof",
                "var",
                "void",
                "while",
                "with",
                "yield",
            ],
            constants: &["true", "false", "null", "undefined", "NaN", "Infinity"],
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            quotes: &['"', '\'', '`'],
            ..Rules::default()
        },
        Language::TypeScript => Rules {
            keywords: &[
                "abstract",
                "as",
                "async",
                "await",
                "break",
                "case",
                "catch",
                "class",
                "const",
                "continue",
                "declare",
                "default",
                "delete",
                "do",
                "else",
                "enum",
                "export",
                "extends",
                "finally",
                "for",
                "from",
                "function",
                "get",
                "if",
                "implements",
                "import",
                "in",
                "infer",
                "instanceof",
                "interface",
                "keyof",
                "let",
                "namespace",
                "new",
                "of",
                "override",
                "private",
                "protected",
                "public",
                "readonly",
                "return",
                "satisfies",
                "set",
                "static",
                "super",
                "switch",
                "this",
                "throw",
                "try",
                "type",
                "typeof",
                "var",
                "void",
                "while",
                "with",
                "yield",
            ],
            types: &[
                "any", "bigint", "boolean", "never", "number", "object", "string", "symbol",
                "unknown",
            ],
            constants: &["true", "false", "null", "undefined", "NaN", "Infinity"],
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            quotes: &['"', '\'', '`'],
            ..Rules::default()
        },
        Language::Php => Rules {
            keywords: &[
                "abstract",
                "and",
                "array",
                "as",
                "break",
                "callable",
                "case",
                "catch",
                "class",
                "clone",
                "const",
                "continue",
                "declare",
                "default",
                "do",
                "echo",
                "else",
                "elseif",
                "empty",
                "enddeclare",
                "endfor",
                "endforeach",
                "endif",
                "endswitch",
                "endwhile",
                "eval",
                "exit",
                "extends",
                "final",
                "finally",
                "fn",
                "for",
                "foreach",
                "function",
                "global",
                "goto",
                "if",
                "implements",
                "include",
                "include_once",
                "instanceof",
                "insteadof",
                "interface",
                "isset",
                "list",
                "match",
                "namespace",
                "new",
                "or",
                "print",
                "private",
                "protected",
                "public",
                "readonly",
                "require",
                "require_once",
                "return",
                "static",
                "switch",
                "throw",
                "trait",
                "try",
                "unset",
                "use",
                "var",
                "while",
                "xor",
                "yield",
            ],
            types: &[
                "bool", "float", "int", "iterable", "mixed", "object", "string", "void",
            ],
            constants: &["true", "false", "null", "TRUE", "FALSE", "NULL"],
            line_comments: &["//", "#"],
            block_comment: Some(("/*", "*/")),
            quotes: &['"', '\''],
            shell_vars: true,
            php_tags: true,
            ..Rules::default()
        },
        Language::Yaml => Rules {
            constants: &["true", "false", "null", "yes", "no", "on", "off"],
            line_comments: &["#"],
            quotes: &['"', '\''],
            key_separator: Some(':'),
            ..Rules::default()
        },
        Language::Markdown | Language::Plain => Rules {
            keywords: &[],
            types: &[],
            constants: &[],
            line_comments: &[],
            block_comment: None,
            nested_comments: false,
            quotes: &[],
            triple_quotes: false,
            char_literal: false,
            rust_raw: false,
            preprocessor: false,
            shell_vars: false,
            uppercase_types: false,
            ..Rules::default()
        },
    }
}

/// Zwraca spany podświetlania dla całego tekstu.
pub fn highlight(text: &str, language: Language) -> Vec<Span> {
    match language {
        Language::Plain => Vec::new(),
        Language::Markdown => markdown(text),
        Language::Html | Language::Xml => markup(text),
        _ => lex(text, language),
    }
}

fn push(spans: &mut Vec<Span>, start: usize, end: usize, kind: TokenKind) {
    if start >= end {
        return;
    }
    if let Some(last) = spans.last_mut() {
        if last.end == start && last.kind == kind {
            last.end = end;
            return;
        }
    }
    spans.push(Span::new(start, end, kind));
}

fn char_at(text: &str, index: usize) -> Option<char> {
    text.get(index..)?.chars().next()
}

fn next_non_space(text: &str, mut index: usize) -> Option<char> {
    while let Some(ch) = char_at(text, index) {
        if !ch.is_whitespace() {
            return Some(ch);
        }
        index += ch.len_utf8();
    }
    None
}

fn lex(text: &str, language: Language) -> Vec<Span> {
    let rules = rules(language);
    let mut spans = Vec::new();
    let mut index = 0;
    let mut line_start = true;

    while index < text.len() {
        if rules.php_tags {
            let mut matched = None;
            for tag in ["<?php", "<?=", "<?", "?>"] {
                if text[index..].starts_with(tag) {
                    matched = Some(tag.len());
                    break;
                }
            }
            if let Some(len) = matched {
                push(&mut spans, index, index + len, TokenKind::Keyword);
                index += len;
                line_start = false;
                continue;
            }
        }
        if let Some((open, close)) = rules.block_comment {
            if text[index..].starts_with(open) {
                let end = scan_block_comment(text, index, open, close, rules.nested_comments);
                push(&mut spans, index, end, TokenKind::Comment);
                index = end;
                continue;
            }
        }
        if rules
            .line_comments
            .iter()
            .any(|marker| text[index..].starts_with(marker))
        {
            let end = text[index..]
                .find('\n')
                .map(|offset| index + offset)
                .unwrap_or(text.len());
            push(&mut spans, index, end, TokenKind::Comment);
            index = end;
            continue;
        }

        let Some(ch) = char_at(text, index) else {
            break;
        };

        if ch == '\n' {
            index += 1;
            line_start = true;
            continue;
        }
        if ch.is_whitespace() {
            index += ch.len_utf8();
            continue;
        }

        if rules.preprocessor && ch == '#' && line_start {
            let end = text[index..]
                .find('\n')
                .map(|offset| index + offset)
                .unwrap_or(text.len());
            push(&mut spans, index, end, TokenKind::Comment);
            index = end;
            continue;
        }

        if rules.shell_vars && ch == '$' {
            let end = scan_shell_variable(text, index);
            push(&mut spans, index, end, TokenKind::Variable);
            index = end;
            line_start = false;
            continue;
        }

        if rules.at_keywords && ch == '@' {
            let end = scan_identifier(text, index + 1);
            if end > index + 1 {
                push(&mut spans, index, end, TokenKind::Keyword);
                index = end;
                line_start = false;
                continue;
            }
        }

        if rules.rust_raw && ch == 'r' {
            if let Some(end) = scan_raw_string(text, index) {
                push(&mut spans, index, end, TokenKind::String);
                index = end;
                line_start = false;
                continue;
            }
        }

        if rules.char_literal && ch == '\'' {
            if let Some(end) = scan_char_literal(text, index) {
                push(&mut spans, index, end, TokenKind::String);
                index = end;
                line_start = false;
                continue;
            }
        }

        if rules.quotes.contains(&ch) {
            let triple =
                rules.triple_quotes && text[index..].starts_with(&ch.to_string().repeat(3));
            let end = scan_string(text, index, ch, triple);
            let kind = if language == Language::Json && next_non_space(text, end) == Some(':') {
                TokenKind::Variable
            } else {
                TokenKind::String
            };
            push(&mut spans, index, end, kind);
            index = end;
            line_start = false;
            continue;
        }

        if ch.is_ascii_digit() {
            let end = scan_number(text, index);
            push(&mut spans, index, end, TokenKind::Number);
            index = end;
            line_start = false;
            continue;
        }

        if ch.is_alphabetic() || ch == '_' {
            let end = scan_identifier(text, index);
            let ident = &text[index..end];
            let kind = if rules.constants.contains(&ident) {
                Some(TokenKind::Constant)
            } else if rules.keywords.contains(&ident) {
                Some(TokenKind::Keyword)
            } else if rules.types.contains(&ident)
                || (rules.uppercase_types && ident.chars().next().is_some_and(char::is_uppercase))
            {
                Some(TokenKind::TypeName)
            } else if rules
                .key_separator
                .is_some_and(|separator| next_non_space(text, end) == Some(separator))
            {
                Some(TokenKind::Variable)
            } else if next_non_space(text, end) == Some('(') {
                Some(TokenKind::Function)
            } else {
                None
            };
            if let Some(kind) = kind {
                push(&mut spans, index, end, kind);
            }
            index = end;
            line_start = false;
            continue;
        }

        if "+-*/%=<>!&|^~?:".contains(ch) {
            push(
                &mut spans,
                index,
                index + ch.len_utf8(),
                TokenKind::Operator,
            );
        }
        index += ch.len_utf8();
        line_start = false;
    }
    spans
}

fn markup(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut index = 0;
    while index < text.len() {
        if text[index..].starts_with("<!--") {
            let end = text[index + 4..]
                .find("-->")
                .map(|offset| index + 4 + offset + 3)
                .unwrap_or(text.len());
            push(&mut spans, index, end, TokenKind::Comment);
            index = end;
            continue;
        }
        if text[index..].starts_with("<!") {
            let end = text[index..]
                .find('>')
                .map(|offset| index + offset + 1)
                .unwrap_or(text.len());
            push(&mut spans, index, end, TokenKind::Keyword);
            index = end;
            continue;
        }
        let closes = text[index..].starts_with("</");
        let opens = text[index..].starts_with('<')
            && char_at(text, index + 1).is_some_and(|ch| ch.is_alphabetic() || ch == '_');
        if closes || opens {
            push(&mut spans, index, index + 1, TokenKind::Operator);
            index += 1;
            if closes {
                push(&mut spans, index, index + 1, TokenKind::Operator);
                index += 1;
            }
            let name_end = scan_identifier(text, index);
            if name_end > index {
                push(&mut spans, index, name_end, TokenKind::Keyword);
                index = name_end;
            }
            while index < text.len() {
                let Some(ch) = char_at(text, index) else {
                    break;
                };
                if ch == '>' {
                    push(&mut spans, index, index + 1, TokenKind::Operator);
                    index += 1;
                    break;
                }
                if ch == '/' || ch == '=' {
                    push(&mut spans, index, index + 1, TokenKind::Operator);
                    index += 1;
                    continue;
                }
                if ch.is_whitespace() {
                    index += ch.len_utf8();
                    continue;
                }
                if ch == '"' || ch == '\'' {
                    let end = scan_string(text, index, ch, false);
                    push(&mut spans, index, end, TokenKind::String);
                    index = end;
                    continue;
                }
                let start = index;
                while let Some(value) = char_at(text, index) {
                    if value.is_whitespace() || value == '=' || value == '>' || value == '/' {
                        break;
                    }
                    index += value.len_utf8();
                }
                push(&mut spans, start, index, TokenKind::Variable);
            }
            continue;
        }
        if text[index..].starts_with('&') {
            if let Some(offset) = text[index..].find(';') {
                let end = index + offset + 1;
                if offset > 1 && offset <= 12 {
                    push(&mut spans, index, end, TokenKind::Constant);
                    index = end;
                    continue;
                }
            }
        }
        index += char_at(text, index).map(char::len_utf8).unwrap_or(1);
    }
    spans
}

fn scan_block_comment(text: &str, start: usize, open: &str, close: &str, nested: bool) -> usize {
    let mut depth = 1usize;
    let mut index = start + open.len();
    while index < text.len() {
        if nested && text[index..].starts_with(open) {
            depth += 1;
            index += open.len();
        } else if text[index..].starts_with(close) {
            depth -= 1;
            index += close.len();
            if depth == 0 {
                return index;
            }
        } else {
            index += char_at(text, index).map(char::len_utf8).unwrap_or(1);
        }
    }
    text.len()
}

fn scan_string(text: &str, start: usize, quote: char, triple: bool) -> usize {
    let delimiter = if triple {
        quote.to_string().repeat(3)
    } else {
        quote.to_string()
    };
    let mut index = start + delimiter.len();
    while index < text.len() {
        if text[index..].starts_with(&delimiter) {
            return index + delimiter.len();
        }
        let Some(ch) = char_at(text, index) else {
            break;
        };
        if ch == '\\' {
            index += ch.len_utf8();
            if let Some(next) = char_at(text, index) {
                index += next.len_utf8();
            }
            continue;
        }
        if ch == '\n' && !triple {
            return index;
        }
        index += ch.len_utf8();
    }
    text.len()
}

fn scan_char_literal(text: &str, start: usize) -> Option<usize> {
    let mut index = start + 1;
    let ch = char_at(text, index)?;
    if ch == '\n' || ch == '\'' {
        return None;
    }
    index += ch.len_utf8();
    if ch == '\\' {
        let escaped = char_at(text, index)?;
        if escaped == '\n' {
            return None;
        }
        index += escaped.len_utf8();
    }
    (char_at(text, index) == Some('\'')).then_some(index + 1)
}

fn scan_raw_string(text: &str, start: usize) -> Option<usize> {
    let mut index = start + 1;
    let mut hashes = 0;
    while char_at(text, index) == Some('#') {
        hashes += 1;
        index += 1;
    }
    if char_at(text, index) != Some('"') {
        return None;
    }
    index += 1;
    let closing = format!("\"{}", "#".repeat(hashes));
    text[index..]
        .find(&closing)
        .map(|offset| index + offset + closing.len())
}

fn scan_shell_variable(text: &str, start: usize) -> usize {
    let mut index = start + 1;
    if char_at(text, index) == Some('{') {
        return text[index..]
            .find('}')
            .map(|offset| index + offset + 1)
            .unwrap_or(text.len());
    }
    while let Some(ch) = char_at(text, index) {
        if ch.is_alphanumeric() || ch == '_' {
            index += ch.len_utf8();
        } else {
            break;
        }
    }
    index
}

fn scan_number(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    let mut index = start;
    if bytes.get(index) == Some(&b'0')
        && matches!(
            bytes.get(index + 1),
            Some(b'x' | b'X' | b'b' | b'B' | b'o' | b'O')
        )
    {
        index += 2;
        while let Some(&byte) = bytes.get(index) {
            if byte.is_ascii_alphanumeric() || byte == b'_' {
                index += 1;
            } else {
                break;
            }
        }
        return index;
    }
    while let Some(&byte) = bytes.get(index) {
        if byte.is_ascii_digit() || byte == b'_' {
            index += 1;
        } else {
            break;
        }
    }
    if bytes.get(index) == Some(&b'.') && bytes.get(index + 1).is_some_and(u8::is_ascii_digit) {
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        let mut probe = index + 1;
        if matches!(bytes.get(probe), Some(b'+' | b'-')) {
            probe += 1;
        }
        if bytes.get(probe).is_some_and(u8::is_ascii_digit) {
            index = probe;
            while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
        }
    }
    index
}

fn scan_identifier(text: &str, start: usize) -> usize {
    let mut index = start;
    while let Some(ch) = char_at(text, index) {
        if ch.is_alphanumeric() || ch == '_' {
            index += ch.len_utf8();
        } else {
            break;
        }
    }
    index
}

fn markdown(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut offset = 0;
    let mut fenced = false;
    for line in text.split_inclusive('\n') {
        let content_end = line.trim_end_matches('\n').trim_end_matches('\r').len();
        let content = &line[..content_end];
        let trimmed = content.trim_start();
        let indent = content.len() - trimmed.len();

        if trimmed.starts_with("```") {
            push(
                &mut spans,
                offset,
                offset + content.len(),
                TokenKind::String,
            );
            fenced = !fenced;
            offset += line.len();
            continue;
        }
        if fenced {
            push(
                &mut spans,
                offset,
                offset + content.len(),
                TokenKind::String,
            );
            offset += line.len();
            continue;
        }
        if trimmed.starts_with('#') {
            push(
                &mut spans,
                offset,
                offset + content.len(),
                TokenKind::Keyword,
            );
            offset += line.len();
            continue;
        }
        if trimmed.starts_with('>') {
            push(
                &mut spans,
                offset,
                offset + content.len(),
                TokenKind::Comment,
            );
            offset += line.len();
            continue;
        }
        let marker_len = list_marker(trimmed);
        if marker_len > 0 {
            push(
                &mut spans,
                offset + indent,
                offset + indent + marker_len,
                TokenKind::Operator,
            );
        }
        inline_markdown(&mut spans, content, offset);
        offset += line.len();
    }
    spans.sort_by_key(|span| (span.start, span.end));
    merge_adjacent(spans)
}

fn list_marker(line: &str) -> usize {
    let bytes = line.as_bytes();
    if matches!(bytes.first(), Some(b'-' | b'*' | b'+'))
        && bytes.get(1).is_some_and(u8::is_ascii_whitespace)
    {
        return 1;
    }
    let digits = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits > 0
        && matches!(bytes.get(digits), Some(b'.' | b')'))
        && bytes.get(digits + 1).is_some_and(u8::is_ascii_whitespace)
    {
        return digits + 1;
    }
    0
}

fn inline_markdown(spans: &mut Vec<Span>, line: &str, offset: usize) {
    let mut index = 0;
    while index < line.len() {
        let Some(ch) = line[index..].chars().next() else {
            break;
        };
        if ch == '`' {
            if let Some(relative) = line[index + 1..].find('`') {
                let end = index + 1 + relative + 1;
                push(spans, offset + index, offset + end, TokenKind::String);
                index = end;
                continue;
            }
        }
        if ch == ']' && line[index..].starts_with("](") {
            if let Some(close) = line[index + 2..].find(')') {
                let end = index + 2 + close + 1;
                push(
                    spans,
                    offset + index + 2,
                    offset + end - 1,
                    TokenKind::Constant,
                );
                index = end;
                continue;
            }
        }
        if (ch == '*' || ch == '_' || ch == '~')
            && line[index..].starts_with(&ch.to_string().repeat(2))
        {
            push(
                spans,
                offset + index,
                offset + index + 2,
                TokenKind::Operator,
            );
            index += 2;
            continue;
        }
        index += ch.len_utf8();
    }
}

fn merge_adjacent(spans: Vec<Span>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::with_capacity(spans.len());
    for span in spans {
        if let Some(last) = out.last_mut() {
            if last.end == span.start && last.kind == span.kind {
                last.end = span.end;
                continue;
            }
        }
        out.push(span);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind_at(spans: &[Span], text: &str, needle: &str) -> Option<TokenKind> {
        let start = text.find(needle)?;
        spans
            .iter()
            .find(|span| span.start <= start && span.end >= start + needle.len())
            .map(|span| span.kind)
    }

    #[test]
    fn rust_highlights_keywords_strings_comments_and_types() {
        let text =
            "// komentarz\nfn main() {\n    let name = \"Luna\";\n    let n: usize = 42;\n}\n";
        let spans = highlight(text, Language::Rust);
        assert_eq!(kind_at(&spans, text, "komentarz"), Some(TokenKind::Comment));
        assert_eq!(kind_at(&spans, text, "fn"), Some(TokenKind::Keyword));
        assert_eq!(kind_at(&spans, text, "main"), Some(TokenKind::Function));
        assert_eq!(kind_at(&spans, text, "\"Luna\""), Some(TokenKind::String));
        assert_eq!(kind_at(&spans, text, "usize"), Some(TokenKind::TypeName));
        assert_eq!(kind_at(&spans, text, "42"), Some(TokenKind::Number));
    }

    #[test]
    fn rust_raw_strings_and_lifetimes_do_not_swallow_code() {
        let text = "let s = r#\"a\"b\"#; fn x<'a>(v: &'a str) {}\n";
        let spans = highlight(text, Language::Rust);
        assert_eq!(
            kind_at(&spans, text, "r#\"a\"b\"#"),
            Some(TokenKind::String)
        );
        assert_eq!(kind_at(&spans, text, "fn"), Some(TokenKind::Keyword));
        assert_eq!(kind_at(&spans, text, "'a"), Some(TokenKind::String));
    }

    #[test]
    fn json_marks_keys_and_values_differently() {
        let text = "{\"name\": \"Luna\", \"count\": 3, \"on\": true}";
        let spans = highlight(text, Language::Json);
        assert_eq!(kind_at(&spans, text, "\"name\""), Some(TokenKind::Variable));
        assert_eq!(kind_at(&spans, text, "\"Luna\""), Some(TokenKind::String));
        assert_eq!(kind_at(&spans, text, "3"), Some(TokenKind::Number));
        assert_eq!(kind_at(&spans, text, "true"), Some(TokenKind::Constant));
    }

    #[test]
    fn toml_highlights_keys_and_comments() {
        let text = "# config\nname = \"Luna\"\nversion = 1\n";
        let spans = highlight(text, Language::Toml);
        assert_eq!(kind_at(&spans, text, "config"), Some(TokenKind::Comment));
        assert_eq!(kind_at(&spans, text, "name"), Some(TokenKind::Variable));
        assert_eq!(kind_at(&spans, text, "\"Luna\""), Some(TokenKind::String));
    }

    #[test]
    fn shell_marks_variables_and_keywords() {
        let text = "#!/bin/sh\nfor f in $HOME/*; do echo $f; done\n";
        let spans = highlight(text, Language::Shell);
        assert_eq!(kind_at(&spans, text, "for"), Some(TokenKind::Keyword));
        assert_eq!(kind_at(&spans, text, "$HOME"), Some(TokenKind::Variable));
        assert_eq!(kind_at(&spans, text, "$f"), Some(TokenKind::Variable));
    }

    #[test]
    fn markdown_highlights_headings_and_code() {
        let text = "# Tytuł\n\n```rust\nfn main() {}\n```\n\n`inline`\n";
        let spans = highlight(text, Language::Markdown);
        assert_eq!(kind_at(&spans, text, "# Tytuł"), Some(TokenKind::Keyword));
        assert_eq!(
            kind_at(&spans, text, "fn main() {}"),
            Some(TokenKind::String)
        );
        assert_eq!(kind_at(&spans, text, "`inline`"), Some(TokenKind::String));
    }

    #[test]
    fn html_marks_tags_attributes_and_comments() {
        let text = "<!-- komentarz -->\n<div class=\"box\" id='x'>Treść</div>\n";
        let spans = highlight(text, Language::Html);
        assert_eq!(kind_at(&spans, text, "div"), Some(TokenKind::Keyword));
        assert_eq!(kind_at(&spans, text, "class"), Some(TokenKind::Variable));
        assert_eq!(kind_at(&spans, text, "\"box\""), Some(TokenKind::String));
        assert_eq!(kind_at(&spans, text, "komentarz"), Some(TokenKind::Comment));
    }

    #[test]
    fn css_marks_properties_at_rules_and_comments() {
        let text = "/* c */\n.box { color: #fff; margin: 4px; }\n@media screen {}\n";
        let spans = highlight(text, Language::Css);
        assert_eq!(kind_at(&spans, text, "color"), Some(TokenKind::Variable));
        assert_eq!(kind_at(&spans, text, "4"), Some(TokenKind::Number));
        assert_eq!(kind_at(&spans, text, "media"), Some(TokenKind::Keyword));
        assert_eq!(kind_at(&spans, text, "c "), Some(TokenKind::Comment));
    }

    #[test]
    fn php_marks_tags_variables_and_keywords() {
        let text = "<?php\nfunction greet($name) { echo \"Hi\"; }\n?>";
        let spans = highlight(text, Language::Php);
        assert_eq!(kind_at(&spans, text, "<?php"), Some(TokenKind::Keyword));
        assert_eq!(kind_at(&spans, text, "function"), Some(TokenKind::Keyword));
        assert_eq!(kind_at(&spans, text, "$name"), Some(TokenKind::Variable));
        assert_eq!(kind_at(&spans, text, "greet"), Some(TokenKind::Function));
    }

    #[test]
    fn javascript_marks_keywords_and_template_strings() {
        let text = "const x = `hi`; // ok\nasync function run() {}";
        let spans = highlight(text, Language::JavaScript);
        assert_eq!(kind_at(&spans, text, "const"), Some(TokenKind::Keyword));
        assert_eq!(kind_at(&spans, text, "`hi`"), Some(TokenKind::String));
        assert_eq!(kind_at(&spans, text, "async"), Some(TokenKind::Keyword));
        assert_eq!(kind_at(&spans, text, "ok"), Some(TokenKind::Comment));
    }

    #[test]
    fn yaml_marks_keys_and_constants() {
        let text = "# config\nname: Luna\ncount: 2\nenabled: true\n";
        let spans = highlight(text, Language::Yaml);
        assert_eq!(kind_at(&spans, text, "name"), Some(TokenKind::Variable));
        assert_eq!(kind_at(&spans, text, "2"), Some(TokenKind::Number));
        assert_eq!(kind_at(&spans, text, "true"), Some(TokenKind::Constant));
        assert_eq!(kind_at(&spans, text, "config"), Some(TokenKind::Comment));
    }

    #[test]
    fn new_languages_are_detected_by_extension() {
        assert_eq!(Language::from_name("index.html"), Language::Html);
        assert_eq!(Language::from_name("icon.svg"), Language::Xml);
        assert_eq!(Language::from_name("style.css"), Language::Css);
        assert_eq!(Language::from_name("app.js"), Language::JavaScript);
        assert_eq!(Language::from_name("app.tsx"), Language::TypeScript);
        assert_eq!(Language::from_name("index.php"), Language::Php);
        assert_eq!(Language::from_name("config.yml"), Language::Yaml);
        assert_eq!(Language::from_name("notes.txt"), Language::Plain);
    }

    #[test]
    fn plain_text_has_no_spans_and_languages_detection_works() {
        assert!(highlight("anything", Language::Plain).is_empty());
        assert_eq!(Language::from_name("main.rs"), Language::Rust);
        assert_eq!(Language::from_name("Cargo.toml"), Language::Toml);
        assert_eq!(Language::from_name("README.md"), Language::Markdown);
        assert_eq!(Language::from_name("script.sh"), Language::Shell);
        assert_eq!(Language::from_name("a.cpp"), Language::Cpp);
        assert_eq!(Language::from_name("x.py"), Language::Python);
    }

    #[test]
    fn unterminated_tokens_do_not_panic_on_multibyte_text() {
        let text = "let s = \"zażółć\"; /* bez końca\nfn x() {}";
        let spans = highlight(text, Language::Rust);
        assert!(spans.iter().any(|span| span.end <= text.len()));
    }
}
