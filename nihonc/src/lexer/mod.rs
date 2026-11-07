//! Lexer da Nihon: transforma o texto em tokens e reporta os erros lexicos
//! (E0001 a E0009) e os avisos W0001 e W0005.
//!
//! Regra central: japones nao separa palavras com espaco, entao o lexer le
//! primeiro um *bloco* de caracteres validos para nomes (kanji, kana, latim,
//! digitos, `_`) e so depois decide se o bloco e palavra-chave, macro ou nome.

pub mod chars;
pub mod keywords;

use crate::diag::codes::*;
use crate::diag::{Diagnostic, Handler};
use crate::source::{SourceFile, Span};
use chars::*;
use keywords::*;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Keyword(Kw),
    Ident(String),
    /// Nome de macro sem o `!` (a forma canonica em japones, se for conhecida).
    Macro(String),
    Int(u128),
    /// Texto do literal sem os `_`.
    Float(String),
    Str(String),
    Char(char),
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semi,
    Colon,
    ColonColon,
    Dot,
    DotDot,
    DotDotEq,
    Arrow,
    FatArrow,
    Eq,
    EqEq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    AndAnd,
    OrOr,
    Bang,
    Amp,
    Pipe,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub fn lex(file: &SourceFile, handler: &mut Handler) -> Vec<Token> {
    let mut lx = Lexer {
        src: &file.text,
        pos: 0,
        handler,
        tokens: Vec::new(),
    };
    lx.run();
    lx.tokens
}

struct Lexer<'a, 'h> {
    src: &'a str,
    pos: usize,
    handler: &'h mut Handler,
    tokens: Vec<Token>,
}

fn punct(s: &str) -> Option<TokenKind> {
    use TokenKind::*;
    Some(match s {
        "..=" => DotDotEq,
        "->" => Arrow,
        "=>" => FatArrow,
        "==" => EqEq,
        "!=" => NotEq,
        "<=" => LtEq,
        ">=" => GtEq,
        "&&" => AndAnd,
        "||" => OrOr,
        "+=" => PlusEq,
        "-=" => MinusEq,
        "*=" => StarEq,
        "/=" => SlashEq,
        "%=" => PercentEq,
        "::" => ColonColon,
        ".." => DotDot,
        "(" => LParen,
        ")" => RParen,
        "{" => LBrace,
        "}" => RBrace,
        "[" => LBracket,
        "]" => RBracket,
        "," => Comma,
        ";" => Semi,
        ":" => Colon,
        "." => Dot,
        "=" => Eq,
        "<" => Lt,
        ">" => Gt,
        "+" => Plus,
        "-" => Minus,
        "*" => Star,
        "/" => Slash,
        "%" => Percent,
        "!" => Bang,
        "&" => Amp,
        "|" => Pipe,
        _ => return None,
    })
}

fn number_error(span: Span, message: impl Into<String>, label: impl Into<String>) -> Diagnostic {
    Diagnostic::error(E0005, message).primary(span, label)
}

/// Sublinhados so podem ficar entre dois digitos. Devolve o intervalo (em
/// bytes, relativo a `body`) do primeiro grupo de `_` mal colocado.
fn bad_underscore(body: &str, is_digit: impl Fn(char) -> bool) -> Option<(usize, usize)> {
    let b = body.as_bytes();
    for i in 0..b.len() {
        if b[i] != b'_' {
            continue;
        }
        let prev_ok = i > 0 && is_digit(b[i - 1] as char);
        let next_ok = i + 1 < b.len() && is_digit(b[i + 1] as char);
        if !(prev_ok && next_ok) {
            let mut j = i;
            while j < b.len() && b[j] == b'_' {
                j += 1;
            }
            return Some((i, j));
        }
    }
    None
}

impl<'a, 'h> Lexer<'a, 'h> {
    // ---------------------------------------------------------------- base

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn peek_nth(&self, n: usize) -> Option<char> {
        self.src[self.pos..].chars().nth(n)
    }

    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn eat_while(&mut self, f: impl Fn(char) -> bool) {
        while let Some(c) = self.peek() {
            if !f(c) {
                break;
            }
            self.pos += c.len_utf8();
        }
    }

    fn push(&mut self, kind: TokenKind, start: usize) {
        self.tokens.push(Token {
            kind,
            span: Span::new(start, self.pos),
        });
    }

    fn emit(&mut self, d: Diagnostic) {
        self.handler.emit(d);
    }

    fn run(&mut self) {
        while let Some(c) = self.peek() {
            match c {
                ' ' | '\t' | '\r' | '\n' => self.pos += 1,
                '/' if self.rest().starts_with("//") => self.eat_while(|c| c != '\n'),
                '/' if self.rest().starts_with("/*") => self.block_comment(),
                '"' => self.string(),
                '\'' => self.char_lit(),
                c if c.is_ascii_digit() => self.number(),
                c if is_ident_start(c) => self.ident(),
                c if zenkaku(c).is_some() => self.zenkaku_run(),
                _ => self.punct_or_unknown(),
            }
        }
        let end = self.src.len();
        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::new(end, end),
        });
    }

    // ------------------------------------------------------------ comentarios

    fn block_comment(&mut self) {
        let start = self.pos;
        self.pos += 2;
        let mut depth = 1;
        let mut nested = false;
        while depth > 0 {
            let rest = self.rest();
            if rest.starts_with("/*") {
                depth += 1;
                nested = true;
                self.pos += 2;
            } else if rest.starts_with("*/") {
                depth -= 1;
                self.pos += 2;
            } else if let Some(c) = self.peek() {
                self.pos += c.len_utf8();
            } else {
                let mut d = Diagnostic::error(E0006, "unterminated block comment")
                    .primary(Span::new(start, start + 2), "this block comment is never closed")
                    .help("close it with `*/`");
                if nested {
                    d = d.note("block comments nest: every `/*` needs its own `*/`");
                }
                self.emit(d);
                return;
            }
        }
    }

    // --------------------------------------------------------------- nomes

    fn ident(&mut self) {
        let start = self.pos;
        self.eat_while(is_ident_continue);
        let text: &'a str = &self.src[start..self.pos];
        let span = Span::new(start, self.pos);
        let is_macro_call = matches!(self.rest().get(..2), Some("!(" | "![" | "!{"));

        if let Some((info, latin)) = lookup_keyword(text) {
            if latin {
                self.latin_warning(span, text, info.jp, info.meaning, false);
            }
            self.push(TokenKind::Keyword(info.kw), start);
            return;
        }

        if is_macro_call {
            let name = match lookup_macro(text) {
                Some((m, latin)) => {
                    if latin {
                        self.latin_warning(span, text, m.jp, m.meaning, true);
                    }
                    m.jp.to_string()
                }
                None => text.to_string(),
            };
            self.pos += 1; // o `!`
            self.push(TokenKind::Macro(name), start);
            return;
        }

        if let Some((info, latin, len)) = split_keyword_prefix(text) {
            let kw_text = &text[..len];
            let label = if latin {
                format!("keyword `{}` (\"{}\")", kw_text, info.meaning)
            } else {
                format!("keyword `{}` ({}, \"{}\")", kw_text, info.romaji, info.meaning)
            };
            let kw_span = Span::new(start, start + len);
            let d = Diagnostic::error(
                E0003,
                format!("identifier `{}` starts with keyword `{}`", text, kw_text),
            )
            .primary(kw_span, label)
            .suggest(
                "keywords must be separated from identifiers by a space",
                Span::new(start + len, start + len),
                " ",
                true,
            );
            self.emit(d);
            // recupera como se o espaco existisse
            self.tokens.push(Token {
                kind: TokenKind::Keyword(info.kw),
                span: kw_span,
            });
            self.tokens.push(Token {
                kind: TokenKind::Ident(text[len..].to_string()),
                span: Span::new(start + len, self.pos),
            });
            return;
        }

        if text.chars().any(is_half_kana) {
            let mut d = Diagnostic::warning(W0005, format!("half-width katakana in identifier `{}`", text))
                .primary(span, "half-width katakana (hankaku)");
            match half_to_full(text) {
                Some(full) => d = d.suggest("use full-width katakana", span, full, true),
                None => d = d.help("use full-width katakana"),
            }
            self.emit(d);
        }
        self.push(TokenKind::Ident(text.to_string()), start);
    }

    fn latin_warning(&mut self, span: Span, written: &str, jp: &str, meaning: &str, is_macro: bool) {
        if self.handler.config.romaji {
            return;
        }
        let (what, shown, jp_shown) = if is_macro {
            ("macro", format!("{}!", written), format!("{}!", jp))
        } else {
            ("keyword", written.to_string(), jp.to_string())
        };
        let d = Diagnostic::warning(W0001, format!("{} `{}` is written in latin letters", what, shown))
            .primary(span, format!("latin spelling of `{}` (\"{}\")", jp_shown, meaning))
            .note("pass `--romaji` to allow latin keywords")
            .suggest("use the canonical form", span, jp, true);
        self.emit(d);
    }

    // -------------------------------------------------------------- numeros

    fn number(&mut self) {
        let start = self.pos;
        let radix = match self.rest().get(..2) {
            Some("0x" | "0X") => 16,
            Some("0b" | "0B") => 2,
            Some("0o" | "0O") => 8,
            _ => 10,
        };
        if radix == 10 {
            self.decimal_number(start);
        } else {
            self.radix_number(start, radix);
        }
    }

    fn suffix_error(&mut self, suffix_start: usize) {
        let suffix = &self.src[suffix_start..self.pos];
        let d = number_error(
                Span::new(suffix_start, self.pos),
                format!("invalid suffix `{}` for number literal", suffix),
                "invalid suffix",
            )
            .help("number literals take no suffix; convert types with `として` (toshite, \"as\")");
        self.emit(d);
    }

    fn decimal_number(&mut self, start: usize) {
        let digit_or_us = |c: char| c.is_ascii_digit() || c == '_';
        self.eat_while(digit_or_us);
        let mut is_float = false;
        if self.peek() == Some('.') && self.peek_nth(1).map_or(false, |c| c.is_ascii_digit()) {
            self.pos += 1;
            self.eat_while(digit_or_us);
            is_float = true;
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            let has_exp = match self.peek_nth(1) {
                Some(d) if d.is_ascii_digit() => true,
                Some('+' | '-') => self.peek_nth(2).map_or(false, |d| d.is_ascii_digit()),
                _ => false,
            };
            if has_exp {
                self.pos += 1;
                if matches!(self.peek(), Some('+' | '-')) {
                    self.pos += 1;
                }
                self.eat_while(digit_or_us);
                is_float = true;
            }
        }
        let body_end = self.pos;
        self.eat_while(is_ident_continue);
        if self.pos > body_end {
            self.suffix_error(body_end);
            return;
        }
        let body = &self.src[start..body_end];
        if let Some((a, b)) = bad_underscore(body, |c| c.is_ascii_digit()) {
            let d = number_error(
                    Span::new(start + a, start + b),
                    "invalid underscore in number literal",
                    "underscores must sit between two digits",
                )
                .help("write `1_000`, not `1__000` or `1_`");
            self.emit(d);
            return;
        }
        let clean: String = body.chars().filter(|&c| c != '_').collect();
        if is_float {
            match clean.parse::<f64>() {
                Ok(v) if v.is_finite() => self.push(TokenKind::Float(clean), start),
                _ => {
                    let d = number_error(
                        Span::new(start, self.pos),
                        "float literal is too large",
                        "does not fit in a 64-bit float",
                    );
                    self.emit(d);
                }
            }
        } else {
            match clean.parse::<u128>() {
                Ok(v) => self.push(TokenKind::Int(v), start),
                Err(_) => {
                    let d = number_error(
                        Span::new(start, self.pos),
                        "integer literal is too large",
                        "does not fit in 128 bits",
                    );
                    self.emit(d);
                }
            }
        }
    }

    fn radix_number(&mut self, start: usize, radix: u32) {
        self.pos += 2;
        let digits_start = self.pos;
        let mut bad: Option<(usize, char)> = None;
        while let Some(c) = self.peek() {
            if c == '_' || c.is_digit(radix) {
                self.pos += 1;
            } else if c.is_ascii_alphanumeric() {
                if bad.is_none() {
                    bad = Some((self.pos, c));
                }
                self.pos += 1;
            } else {
                break;
            }
        }
        let digits_end = self.pos;
        self.eat_while(is_ident_continue);
        let digits = &self.src[digits_start..digits_end];
        let name = match radix {
            16 => "hexadecimal",
            8 => "octal",
            _ => "binary",
        };

        if let Some((p, c)) = bad {
            let d = number_error(
                Span::new(p, p + 1),
                format!("invalid digit `{}` for a {} literal", c, name),
                "invalid digit",
            );
            self.emit(d);
        } else if self.pos > digits_end {
            self.suffix_error(digits_end);
        } else if digits.chars().all(|c| c == '_') {
            let prefix = &self.src[start..digits_start];
            let d = number_error(
                Span::new(start, self.pos),
                "no valid digits found for number",
                format!("expected digits after `{}`", prefix),
            );
            self.emit(d);
        } else if let Some((a, b)) = bad_underscore(digits, |c| c.is_digit(radix)) {
            let d = number_error(
                    Span::new(digits_start + a, digits_start + b),
                    "invalid underscore in number literal",
                    "underscores must sit between two digits",
                )
                .help("write `0xFF_FF`, not `0x_FF` or `0xFF_`");
            self.emit(d);
        } else {
            let clean: String = digits.chars().filter(|&c| c != '_').collect();
            match u128::from_str_radix(&clean, radix) {
                Ok(v) => self.push(TokenKind::Int(v), start),
                Err(_) => {
                    let d = number_error(
                        Span::new(start, self.pos),
                        "integer literal is too large",
                        "does not fit in 128 bits",
                    );
                    self.emit(d);
                }
            }
        }
    }

    // ---------------------------------------------------- texto e caractere

    /// Chamada com `self.pos` logo depois da `\` que comecou em `esc_start`.
    fn escape(&mut self, esc_start: usize) -> Option<char> {
        let c = self.peek()?;
        let simple = match c {
            'n' => Some('\n'),
            'r' => Some('\r'),
            't' => Some('\t'),
            '0' => Some('\0'),
            '\\' => Some('\\'),
            '"' => Some('"'),
            '\'' => Some('\''),
            _ => None,
        };
        if simple.is_some() {
            self.pos += 1;
            return simple;
        }
        if c == 'u' {
            self.pos += 1;
            let bad_format = |lx: &mut Self| {
                let d = Diagnostic::error(E0008, "incorrect unicode escape sequence")
                    .primary(Span::new(esc_start, lx.pos), "invalid unicode escape")
                    .help("the format is `\\u{XXXX}` with 1 to 6 hexadecimal digits");
                lx.emit(d);
            };
            if self.peek() != Some('{') {
                bad_format(self);
                return None;
            }
            self.pos += 1;
            let digits_start = self.pos;
            self.eat_while(|c| c.is_ascii_hexdigit());
            let digits = &self.src[digits_start..self.pos];
            if self.peek() != Some('}') {
                bad_format(self);
                return None;
            }
            self.pos += 1;
            if digits.is_empty() || digits.len() > 6 {
                bad_format(self);
                return None;
            }
            return match u32::from_str_radix(digits, 16).ok().and_then(char::from_u32) {
                Some(ch) => Some(ch),
                None => {
                    let d = Diagnostic::error(E0008, "invalid unicode character escape")
                        .primary(Span::new(esc_start, self.pos), "not a valid unicode character")
                        .note("surrogates (U+D800 to U+DFFF) and values above U+10FFFF are not characters");
                    self.emit(d);
                    None
                }
            };
        }
        self.pos += c.len_utf8();
        let d = Diagnostic::error(
            E0008,
            format!("unknown character escape: `{}`", c.escape_debug()),
        )
        .primary(Span::new(esc_start, self.pos), "unknown character escape")
        .note("valid escapes are `\\n`, `\\r`, `\\t`, `\\0`, `\\\\`, `\\\"`, `\\'` and `\\u{...}`");
        self.emit(d);
        None
    }

    fn string(&mut self) {
        let start = self.pos;
        self.pos += 1;
        let mut s = String::new();
        loop {
            match self.peek() {
                None => {
                    let d = Diagnostic::error(E0001, "unterminated string literal")
                        .primary(Span::new(start, self.src.len()), "this string is never closed")
                        .note("strings may span several lines, so a missing closing `\"` swallows the rest of the file")
                        .help("add a closing `\"`");
                    self.emit(d);
                    return;
                }
                Some('"') => {
                    self.pos += 1;
                    break;
                }
                Some('\\') => {
                    let esc_start = self.pos;
                    self.pos += 1;
                    if let Some(ch) = self.escape(esc_start) {
                        s.push(ch);
                    }
                }
                Some(c) => {
                    s.push(c);
                    self.pos += c.len_utf8();
                }
            }
        }
        self.push(TokenKind::Str(s), start);
    }

    fn char_lit(&mut self) {
        let start = self.pos;
        self.pos += 1;
        let value: Option<char>;
        match self.peek() {
            None | Some('\n') | Some('\r') => {
                let d = Diagnostic::error(E0009, "unterminated character literal")
                    .primary(Span::new(start, start + 1), "this character literal is never closed");
                self.emit(d);
                return;
            }
            Some('\'') => {
                self.pos += 1;
                let d = Diagnostic::error(E0009, "empty character literal")
                    .primary(Span::new(start, self.pos), "a character literal needs one character")
                    .help("to write a quote character, escape it: `'\\''`");
                self.emit(d);
                return;
            }
            Some('\\') => {
                let esc_start = self.pos;
                self.pos += 1;
                value = self.escape(esc_start);
            }
            Some(c) => {
                self.pos += c.len_utf8();
                value = Some(c);
            }
        }
        if self.peek() == Some('\'') {
            self.pos += 1;
            if let Some(v) = value {
                self.push(TokenKind::Char(v), start);
            }
            return;
        }
        let rest = self.rest();
        let line_len = rest.find('\n').unwrap_or(rest.len());
        match rest[..line_len].find('\'') {
            Some(i) => {
                self.pos += i + 1;
                let span = Span::new(start, self.pos);
                let lit = &self.src[span.range()];
                let inner = &lit[1..lit.len() - 1];
                let mut d = Diagnostic::error(E0009, "character literal may only contain one character")
                    .primary(span, "more than one character");
                if !inner.contains('"') {
                    d = d.suggest(
                        "if you meant to write a string, use double quotes",
                        span,
                        format!("\"{}\"", inner),
                        false,
                    );
                }
                self.emit(d);
            }
            None => {
                let d = Diagnostic::error(E0009, "unterminated character literal")
                    .primary(Span::new(start, start + 1), "this character literal is never closed");
                self.emit(d);
            }
        }
    }

    // ------------------------------------------------- largura total e resto

    fn zenkaku_run(&mut self) {
        let start = self.pos;
        let mut items: Vec<(char, Zenkaku)> = Vec::new();
        while let Some(c) = self.peek() {
            match zenkaku(c) {
                Some(z) => {
                    items.push((c, z));
                    self.pos += c.len_utf8();
                }
                None => break,
            }
        }
        let span = Span::new(start, self.pos);
        let text = &self.src[start..self.pos];
        let all_known = items.iter().all(|(_, z)| z.ascii.is_some());
        let all_safe = items.iter().all(|(_, z)| z.safe);
        let ascii: String = items.iter().filter_map(|(_, z)| z.ascii).collect();

        let (message, label, help) = if items.len() > 1 {
            (
                format!("full-width characters `{}` are not allowed", text),
                "full-width characters (zenkaku)".to_string(),
                "use the ASCII equivalents".to_string(),
            )
        } else {
            let (c, z) = items[0];
            match z.ascii {
                Some(' ') => (
                    "full-width space (U+3000) is not allowed".to_string(),
                    "full-width space (zenkaku)".to_string(),
                    "use a regular space".to_string(),
                ),
                Some(a) if a.is_ascii_digit() => (
                    format!("full-width digit `{}` is not allowed", c),
                    "full-width digit (zenkaku)".to_string(),
                    "use the ASCII digit".to_string(),
                ),
                Some(a) if a.is_ascii_alphabetic() => (
                    format!("full-width letter `{}` is not allowed", c),
                    "full-width letter (zenkaku)".to_string(),
                    "use the ASCII letter".to_string(),
                ),
                Some(a) if !z.safe => (
                    format!("Japanese punctuation `{}` is not allowed", c),
                    "ideographic comma (touten)".to_string(),
                    format!("use the ASCII {}", ascii_name(a)),
                ),
                Some(a) => (
                    format!("full-width punctuation `{}` is not allowed", c),
                    format!("full-width {} (zenkaku)", ascii_name(a)),
                    format!("use the ASCII {}", ascii_name(a)),
                ),
                None => (
                    format!("Japanese punctuation `{}` is not allowed", c),
                    "no ASCII equivalent".to_string(),
                    String::new(),
                ),
            }
        };

        let mut d = Diagnostic::error(E0002, message).primary(span, label);
        if all_known {
            d = d.suggest(help, span, ascii, all_safe);
        } else {
            d = d.note("Nihon code uses only ASCII punctuation, such as `;` `,` `(` `)` `{` `}`");
        }
        self.emit(d);
    }

    fn punct_or_unknown(&mut self) {
        let start = self.pos;
        let rest = self.rest();
        for len in [3usize, 2, 1] {
            if let Some(kind) = rest.get(..len).and_then(punct) {
                self.pos += len;
                self.push(kind, start);
                return;
            }
        }
        let c = self.peek().unwrap();
        self.pos += c.len_utf8();
        // varias letras seguidas de outro alfabeto (`Привет`) viram um erro so
        if c.is_alphabetic() {
            self.eat_while(|ch| ch.is_alphabetic() && !is_ident_start(ch));
            if self.pos - start > c.len_utf8() {
                let text = &self.src[start..self.pos];
                let d = Diagnostic::error(E0007, format!("unknown start of token: `{}`", text))
                    .primary(Span::new(start, self.pos), "unknown characters")
                    .note("names may only use kanji, kana and latin letters");
                self.emit(d);
                return;
            }
        }
        let invisible = c.is_control() || c.is_whitespace() || crate::source::char_width(c) == 0;
        let shown = if invisible {
            format!("U+{:04X}", c as u32)
        } else {
            format!("`{}`", c)
        };
        let mut d = Diagnostic::error(E0007, format!("unknown start of token: {}", shown))
            .primary(Span::new(start, self.pos), "unknown character");
        if !c.is_ascii() {
            d = d.note(format!("the character is U+{:04X}", c as u32));
        }
        if c as u32 == 0x3099 || c as u32 == 0x309A || (0x300..=0x36F).contains(&(c as u32)) {
            d = d.help("combining marks are not allowed in names: use the precomposed character (for example `が`, not `か` followed by U+3099)");
        } else if c.is_alphabetic() {
            d = d.note("names may only use kanji, kana and latin letters");
        }
        self.emit(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::{Config, Level};
    use TokenKind::*;

    fn lex_with(src: &str, config: Config) -> (Vec<TokenKind>, Vec<crate::diag::Diagnostic>) {
        let file = SourceFile::new("t.nihon", src.to_string());
        let mut h = Handler::new(config);
        let toks = lex(&file, &mut h);
        let kinds = toks.into_iter().map(|t| t.kind).filter(|k| *k != Eof).collect();
        (kinds, h.diags)
    }

    fn lex_str(src: &str) -> (Vec<TokenKind>, Vec<crate::diag::Diagnostic>) {
        lex_with(src, Config::default())
    }

    fn codes(diags: &[crate::diag::Diagnostic]) -> Vec<&'static str> {
        diags.iter().map(|d| d.code).collect()
    }

    #[test]
    fn hello_world() {
        let (k, d) = lex_str("関数 主() { 表示!(\"こんにちは世界\"); }");
        assert!(d.is_empty());
        assert_eq!(
            k,
            vec![
                Keyword(Kw::Fn),
                Ident("主".into()),
                LParen,
                RParen,
                LBrace,
                Macro("表示".into()),
                LParen,
                Str("こんにちは世界".into()),
                RParen,
                Semi,
                RBrace,
            ]
        );
    }

    #[test]
    fn ranges_and_floats() {
        let (k, d) = lex_str("0..10 1..=3 1.5 2e3 1_000 0xFF_ff 0b101");
        assert!(d.is_empty());
        assert_eq!(
            k,
            vec![
                Int(0),
                DotDot,
                Int(10),
                Int(1),
                DotDotEq,
                Int(3),
                Float("1.5".into()),
                Float("2e3".into()),
                Int(1000),
                Int(0xFFFF),
                Int(5),
            ]
        );
    }

    #[test]
    fn naka_alone_is_keyword_but_longer_names_are_free() {
        let (k, d) = lex_str("各 i 中 0..3 { 中心 他人 他もし }");
        assert!(d.is_empty());
        assert_eq!(k[0], Keyword(Kw::For));
        assert_eq!(k[2], Keyword(Kw::In));
        assert_eq!(k[7], Ident("中心".into()));
        assert_eq!(k[8], Ident("他人".into()));
        assert_eq!(k[9], Keyword(Kw::ElseIf));
    }

    #[test]
    fn main_is_a_plain_name() {
        let (k, _) = lex_str("主");
        assert_eq!(k, vec![Ident("主".into())]);
    }

    #[test]
    fn macro_needs_bang_and_open_delimiter() {
        let (k, _) = lex_str("a != b 表示!(x) 表示 !x");
        assert_eq!(k[1], NotEq);
        assert_eq!(k[3], Macro("表示".into()));
        assert_eq!(k[7], Ident("表示".into()));
        assert_eq!(k[8], Bang);
    }

    #[test]
    fn comments_nest() {
        let (k, d) = lex_str("1 /* a /* b */ c */ 2 // fim\n3");
        assert!(d.is_empty());
        assert_eq!(k, vec![Int(1), Int(2), Int(3)]);
    }

    #[test]
    fn romaji_warns_unless_flag() {
        let (k, d) = lex_str("kansuu shu() { hyouji!(\"a\"); }");
        assert_eq!(k[0], Keyword(Kw::Fn));
        assert_eq!(codes(&d), vec!["W0001", "W0001"]);
        assert!(d.iter().all(|x| x.level == Level::Warning));

        let (_, d) = lex_with(
            "kansuu shu() {}",
            Config {
                romaji: true,
                ..Config::default()
            },
        );
        assert!(d.is_empty());

        let (_, d) = lex_with(
            "kansuu shu() {}",
            Config {
                strict: true,
                ..Config::default()
            },
        );
        assert_eq!(d[0].level, Level::Error);
        assert!(d[0].message.ends_with("(denied by `--strict`)"));
    }

    #[test]
    fn keyword_glued_to_name() {
        let (k, d) = lex_str("もしx > 0");
        assert_eq!(codes(&d), vec!["E0003"]);
        assert_eq!(k[0], Keyword(Kw::If));
        assert_eq!(k[1], Ident("x".into()));
    }

    #[test]
    fn fullwidth_runs_are_grouped() {
        // `（１２３）；` e uma sequencia so: um unico erro
        let (_, d) = lex_str("f（１２３）；");
        assert_eq!(codes(&d), vec!["E0002"]);
        let edit = d[0].helps[0].edit.as_ref().unwrap();
        assert_eq!(edit.replacement, "(123);");
        assert!(edit.safe);
        // separadas por ASCII, sao dois erros
        let (_, d) = lex_str("（ f ）");
        assert_eq!(codes(&d), vec!["E0002", "E0002"]);
    }

    #[test]
    fn number_errors() {
        for src in ["12abc", "1__0", "1_", "0x", "0b102", "0xZ", "3回"] {
            let (_, d) = lex_str(src);
            assert_eq!(codes(&d), vec!["E0005"], "para {:?}", src);
        }
        let (_, d) = lex_str("340282366920938463463374607431768211456");
        assert_eq!(codes(&d), vec!["E0005"]);
    }

    #[test]
    fn string_and_char_errors() {
        assert_eq!(codes(&lex_str("\"abc").1), vec!["E0001"]);
        assert_eq!(codes(&lex_str("\"a\\qb\"").1), vec!["E0008"]);
        assert_eq!(codes(&lex_str("''").1), vec!["E0009"]);
        assert_eq!(codes(&lex_str("'ab'").1), vec!["E0009"]);
        assert_eq!(codes(&lex_str("'a").1), vec!["E0009"]);
        let (k, d) = lex_str("'あ' '\\n' \"\\u{3042}\"");
        assert!(d.is_empty());
        assert_eq!(k, vec![Char('あ'), Char('\n'), Str("あ".into())]);
    }

    #[test]
    fn block_comment_and_unknown_char() {
        assert_eq!(codes(&lex_str("/* abc").1), vec!["E0006"]);
        assert_eq!(codes(&lex_str("a @ b").1), vec!["E0007"]);
        assert_eq!(codes(&lex_str("a\u{00A0}b").1), vec!["E0007"]);
        assert_eq!(codes(&lex_str("a Привет b").1), vec!["E0007"]);
    }

    #[test]
    fn half_width_katakana_warns_with_fix() {
        let (_, d) = lex_str("ﾊﾝｶｸ");
        assert_eq!(codes(&d), vec!["W0005"]);
        assert_eq!(d[0].helps[0].edit.as_ref().unwrap().replacement, "ハンカク");
    }

    #[test]
    fn clean_programs_have_no_diagnostics() {
        let src = "関数 階乗(n: 整32) -> 整32 {\n  もし n <= 1 { 返却 1; }\n  n * 階乗(n - 1)\n}\n";
        let (_, d) = lex_str(src);
        assert!(d.is_empty());
    }
}
