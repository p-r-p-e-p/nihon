//! Saida `--message-format=json`: um objeto JSON por linha, um por diagnostico.
//! E o que o plugin de VS Code le para mostrar os erros e oferecer quick fixes.
//!
//! Sem dependencias: o formato e pequeno, entao o JSON e escrito a mao.
//! Posicoes sao `line`/`col` com base 1; `col` conta caracteres (code points),
//! igual ao `-->` da saida normal. `byte_start`/`byte_end` sao offsets em bytes.

use super::{Diagnostic, Level};
use crate::source::{SourceFile, Span};

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// (linha, coluna) de um byte, sem nunca entrar em panico com offsets ruins.
fn pos(file: &SourceFile, byte: usize) -> String {
    let mut b = byte.min(file.text.len());
    while !file.text.is_char_boundary(b) {
        b -= 1;
    }
    let (line, col) = file.line_col(b);
    format!("{{\"line\":{},\"col\":{}}}", line, col)
}

fn range(file: &SourceFile, span: Span) -> String {
    format!(
        "\"start\":{},\"end\":{},\"byte_start\":{},\"byte_end\":{}",
        pos(file, span.start),
        pos(file, span.end),
        span.start,
        span.end
    )
}

pub fn to_json(d: &Diagnostic, file: &SourceFile) -> String {
    let level = match d.level {
        Level::Error => "error",
        Level::Warning => "warning",
    };

    let spans: Vec<String> = d
        .labels
        .iter()
        .map(|l| {
            format!(
                "{{\"primary\":{},\"label\":{},{}}}",
                l.primary,
                esc(&l.message),
                range(file, l.span)
            )
        })
        .collect();

    let notes: Vec<String> = d.notes.iter().map(|n| esc(n)).collect();

    let helps: Vec<String> = d
        .helps
        .iter()
        .map(|h| {
            let edit = match &h.edit {
                Some(e) => format!(
                    "{{{},\"replacement\":{},\"safe\":{}}}",
                    range(file, e.span),
                    esc(&e.replacement),
                    e.safe
                ),
                None => "null".to_string(),
            };
            format!("{{\"message\":{},\"edit\":{}}}", esc(&h.message), edit)
        })
        .collect();

    format!(
        "{{\"level\":\"{}\",\"code\":{},\"message\":{},\"file\":{},\"spans\":[{}],\"notes\":[{}],\"helps\":[{}]}}",
        level,
        esc(d.code),
        esc(&d.message),
        esc(&file.name),
        spans.join(","),
        notes.join(","),
        helps.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_quotes_backslashes_and_controls() {
        assert_eq!(esc("a\"b\\c\nd\u{1}"), "\"a\\\"b\\\\c\\nd\\u0001\"");
        assert_eq!(esc("関数"), "\"関数\"");
    }

    #[test]
    fn one_line_with_positions_in_characters() {
        let file = SourceFile::new("t.nihon", "束縛 x = １;\n".to_string());
        let start = file.text.find('１').unwrap();
        let span = Span::new(start, start + '１'.len_utf8());
        let d = Diagnostic::error("E0002", "full-width")
            .primary(span, "here")
            .suggest("use ASCII", span, "1", true);
        let j = to_json(&d, &file);
        assert!(!j.contains('\n'));
        // `１` e o 8o caractere da linha (束 縛 espaco x espaco = espaco １)
        assert!(j.contains("\"start\":{\"line\":1,\"col\":8},\"end\":{\"line\":1,\"col\":9}"), "{}", j);
        assert!(j.contains("\"replacement\":\"1\",\"safe\":true"), "{}", j);
    }

    #[test]
    fn bad_offsets_do_not_panic() {
        let file = SourceFile::new("t.nihon", "関数".to_string());
        let d = Diagnostic::error("E0000", "x").primary(Span::new(1, 99), "");
        let j = to_json(&d, &file);
        assert!(j.contains("\"byte_start\":1"));
    }
}
