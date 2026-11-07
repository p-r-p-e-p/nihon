//! Texto-fonte, posicoes (spans) e largura de colunas.
//!
//! Os spans guardam *bytes*. Para desenhar os erros, as colunas visuais sao
//! calculadas com `display_width`, que sabe que kanji e kana ocupam 2 colunas.

use crate::width_table::{WIDE, ZERO};
use std::ops::Range;

/// Intervalo de bytes `[start, end)` dentro de um arquivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Span {
        Span { start, end }
    }

    pub fn range(&self) -> Range<usize> {
        self.start..self.end
    }
}

pub struct SourceFile {
    pub name: String,
    pub text: String,
    line_starts: Vec<usize>,
}

impl SourceFile {
    pub fn new(name: impl Into<String>, text: String) -> SourceFile {
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        SourceFile {
            name: name.into(),
            text,
            line_starts,
        }
    }

    /// Linha (comecando em 1) que contem o byte `byte`.
    pub fn line_of(&self, byte: usize) -> usize {
        match self.line_starts.binary_search(&byte) {
            Ok(i) => i + 1,
            Err(i) => i,
        }
    }

    /// Byte onde a linha `line` (1-based) comeca.
    pub fn line_start(&self, line: usize) -> usize {
        self.line_starts[line - 1]
    }

    /// Byte onde o texto da linha termina (sem `\n` nem `\r`).
    pub fn line_end(&self, line: usize) -> usize {
        let mut end = if line < self.line_starts.len() {
            self.line_starts[line] - 1
        } else {
            self.text.len()
        };
        if end > self.line_start(line) && self.text.as_bytes()[end - 1] == b'\r' {
            end -= 1;
        }
        end
    }

    pub fn line_text(&self, line: usize) -> &str {
        &self.text[self.line_start(line)..self.line_end(line)]
    }

    /// (linha, coluna) ambas comecando em 1. A coluna conta caracteres.
    pub fn line_col(&self, byte: usize) -> (usize, usize) {
        let line = self.line_of(byte);
        let start = self.line_start(line);
        let col = self.text[start..byte].chars().count() + 1;
        (line, col)
    }
}

fn in_table(table: &[(u32, u32)], cp: u32) -> bool {
    table
        .binary_search_by(|&(lo, hi)| {
            if cp < lo {
                std::cmp::Ordering::Greater
            } else if cp > hi {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// Colunas que o caractere ocupa no terminal: 0, 1 ou 2.
/// A tabulacao conta como 4 (o renderizador troca `\t` por 4 espacos).
pub fn char_width(c: char) -> usize {
    if c == '\t' {
        return 4;
    }
    let cp = c as u32;
    if cp < 0x20 || (0x7F..0xA0).contains(&cp) {
        return 0;
    }
    if cp < 0x300 {
        return 1;
    }
    if in_table(ZERO, cp) {
        0
    } else if in_table(WIDE, cp) {
        2
    } else {
        1
    }
}

pub fn display_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_of_japanese_text() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("こんにちは"), 10);
        assert_eq!(display_width("関数 主"), 7);
        assert_eq!(display_width("ﾊﾝｶｸ"), 4); // meia largura: 1 coluna cada
        assert_eq!(display_width("；"), 2); // ponto e virgula full-width
        assert_eq!(display_width("a\tb"), 6);
    }

    #[test]
    fn combining_marks_take_no_column() {
        assert_eq!(display_width("e\u{0301}"), 1);
        assert_eq!(display_width("か\u{3099}"), 2);
    }

    #[test]
    fn line_helpers() {
        let f = SourceFile::new("t.nihon", "ab\r\n関数\nz".to_string());
        assert_eq!(f.line_of(0), 1);
        assert_eq!(f.line_text(1), "ab");
        assert_eq!(f.line_text(2), "関数");
        assert_eq!(f.line_text(3), "z");
        let (l, c) = f.line_col(f.text.find('数').unwrap());
        assert_eq!((l, c), (2, 2));
    }
}
