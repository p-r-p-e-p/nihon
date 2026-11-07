//! Desenha um `Diagnostic` no estilo do rustc.
//!
//! Detalhe importante: as colunas sao medidas com `display_width`, entao os
//! `^^^^` ficam alinhados mesmo embaixo de kanji (2 colunas cada).

use super::{Diagnostic, Level};
use crate::source::{display_width, SourceFile};
use std::collections::BTreeMap;

const BLUE: &str = "1;34";
const BOLD: &str = "1";
const CYAN: &str = "1;36";
const GREEN: &str = "1;32";

fn paint(color: bool, code: &str, s: &str) -> String {
    if color {
        format!("\x1b[{}m{}\x1b[0m", code, s)
    } else {
        s.to_string()
    }
}

fn expand_tabs(s: &str) -> String {
    s.replace('\t', "    ")
}

struct Mark {
    col: usize,
    width: usize,
    primary: bool,
    msg: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Sty {
    Plain,
    Primary,
    Secondary,
}

fn sty_of(m: &Mark) -> Sty {
    if m.primary {
        Sty::Primary
    } else {
        Sty::Secondary
    }
}

/// Uma linha de marcadores (`^^^^`, `----`, `|`) com texto opcional no fim.
struct Row {
    cells: Vec<(char, Sty)>,
    tail: Option<(usize, String, Sty)>,
}

impl Row {
    fn new() -> Row {
        Row {
            cells: Vec::new(),
            tail: None,
        }
    }

    fn put(&mut self, col: usize, ch: char, sty: Sty) {
        while self.cells.len() <= col {
            self.cells.push((' ', Sty::Plain));
        }
        self.cells[col] = (ch, sty);
    }

    fn render(&self, color: bool, level_code: &str) -> String {
        let style = |sty: Sty, text: &str| match sty {
            Sty::Plain => text.to_string(),
            Sty::Primary => paint(color, level_code, text),
            Sty::Secondary => paint(color, BLUE, text),
        };
        let mut s = String::new();
        for (ch, sty) in &self.cells {
            s.push_str(&style(*sty, &ch.to_string()));
        }
        if let Some((col, text, sty)) = &self.tail {
            let pad = col.saturating_sub(self.cells.len());
            s.push_str(&" ".repeat(pad));
            s.push_str(&style(*sty, text));
        }
        s
    }
}

/// Monta as linhas de marcadores de uma linha de codigo. A mensagem do rotulo
/// mais a direita fica na mesma linha; as outras descem, ligadas por `|`.
fn mark_rows(marks: &[Mark]) -> Vec<Row> {
    let mut main = Row::new();
    for m in marks.iter().filter(|m| !m.primary) {
        for k in 0..m.width {
            main.put(m.col + k, '-', Sty::Secondary);
        }
    }
    for m in marks.iter().filter(|m| m.primary) {
        for k in 0..m.width {
            main.put(m.col + k, '^', Sty::Primary);
        }
    }

    let right = marks
        .iter()
        .enumerate()
        .max_by_key(|(i, m)| (m.col, *i))
        .map(|(i, _)| i)
        .unwrap_or(0);

    let mut pending: Vec<usize> = Vec::new();
    for (i, m) in marks.iter().enumerate() {
        if m.msg.is_empty() {
            continue;
        }
        if i == right {
            main.tail = Some((main.cells.len() + 1, m.msg.clone(), sty_of(m)));
        } else {
            pending.push(i);
        }
    }

    let mut rows = vec![main];
    if !pending.is_empty() {
        pending.sort_by_key(|&i| marks[i].col);
        let mut conn = Row::new();
        for &i in &pending {
            conn.put(marks[i].col, '|', sty_of(&marks[i]));
        }
        rows.push(conn);
        for k in (0..pending.len()).rev() {
            let mut r = Row::new();
            for &j in &pending[..k] {
                r.put(marks[j].col, '|', sty_of(&marks[j]));
            }
            let m = &marks[pending[k]];
            r.tail = Some((m.col, m.msg.clone(), sty_of(m)));
            rows.push(r);
        }
    }
    rows
}

struct Snippet {
    message: String,
    line: usize,
    new_text: String,
    col: usize,
    marker: Option<(char, usize)>,
}

pub fn render(d: &Diagnostic, file: &SourceFile, color: bool) -> String {
    let (level_name, level_code) = match d.level {
        Level::Error => ("error", "1;31"),
        Level::Warning => ("warning", "1;33"),
    };

    // ---- rotulos agrupados por linha ----
    let mut by_line: BTreeMap<usize, Vec<Mark>> = BTreeMap::new();
    for l in &d.labels {
        let line = file.line_of(l.span.start.min(file.text.len()));
        let ls = file.line_start(line);
        let le = file.line_end(line);
        let s = l.span.start.clamp(ls, le);
        let e = l.span.end.clamp(s, le);
        let col = display_width(&file.text[ls..s]);
        let width = display_width(&file.text[s..e]).max(1);
        by_line.entry(line).or_default().push(Mark {
            col,
            width,
            primary: l.primary,
            msg: l.message.clone(),
        });
    }

    // ---- dicas: com trecho de codigo ou so texto ----
    let mut snippets: Vec<Snippet> = Vec::new();
    let mut text_helps: Vec<&str> = Vec::new();
    for h in &d.helps {
        let Some(e) = &h.edit else {
            text_helps.push(&h.message);
            continue;
        };
        let line = file.line_of(e.span.start.min(file.text.len()));
        let ls = file.line_start(line);
        let le = file.line_end(line);
        let one_line = e.span.start >= ls && e.span.end <= le && !e.replacement.contains('\n');
        if !one_line {
            text_helps.push(&h.message);
            continue;
        }
        let text = &file.text[ls..le];
        let (s, en) = (e.span.start - ls, e.span.end - ls);
        let new_text = format!("{}{}{}", &text[..s], e.replacement, &text[en..]);
        let width = display_width(&e.replacement);
        let marker = if width == 0 {
            None
        } else if s == en {
            Some(('+', width))
        } else {
            Some(('~', width))
        };
        snippets.push(Snippet {
            message: h.message.clone(),
            line,
            new_text,
            col: display_width(&text[..s]),
            marker,
        });
    }

    let max_line = by_line
        .keys()
        .copied()
        .chain(snippets.iter().map(|s| s.line))
        .max()
        .unwrap_or(1);
    let gw = max_line.to_string().len();
    let pad = " ".repeat(gw);
    let bar = paint(color, BLUE, "|");
    let blank = format!("{} {}\n", pad, bar);
    let src_line = |line: usize, text: &str| -> String {
        let ln = paint(color, BLUE, &format!("{:>w$}", line, w = gw));
        if text.is_empty() {
            format!("{} {}\n", ln, bar)
        } else {
            format!("{} {} {}\n", ln, bar, text)
        }
    };
    let marker_line = |content: &str| -> String {
        if content.is_empty() {
            format!("{} {}\n", pad, bar)
        } else {
            format!("{} {} {}\n", pad, bar, content)
        }
    };

    let mut out = String::new();
    out.push_str(&paint(color, level_code, &format!("{}[{}]", level_name, d.code)));
    out.push_str(&paint(color, BOLD, &format!(": {}", d.message)));
    out.push('\n');

    let loc = d.labels.iter().find(|l| l.primary).or(d.labels.first());
    if let Some(l) = loc {
        let (line, col) = file.line_col(l.span.start.min(file.text.len()));
        out.push_str(&format!(
            "{}{} {}:{}:{}\n",
            pad,
            paint(color, BLUE, "-->"),
            file.name,
            line,
            col
        ));
    }

    // ---- trecho de codigo com os marcadores ----
    if !by_line.is_empty() {
        out.push_str(&blank);
        let mut prev: Option<usize> = None;
        for (line, marks) in &by_line {
            if let Some(p) = prev {
                let gap = line - p;
                if gap == 2 {
                    out.push_str(&src_line(p + 1, &expand_tabs(file.line_text(p + 1))));
                } else if gap > 2 {
                    out.push_str("...\n");
                }
            }
            out.push_str(&src_line(*line, &expand_tabs(file.line_text(*line))));
            for row in mark_rows(marks) {
                out.push_str(&marker_line(&row.render(color, level_code)));
            }
            prev = Some(*line);
        }
    }

    // ---- notas e dicas em texto ----
    let has_more = !d.notes.is_empty() || !text_helps.is_empty() || !snippets.is_empty();
    if has_more {
        out.push_str(&blank);
    }
    let eq = paint(color, BLUE, "=");
    for n in &d.notes {
        out.push_str(&format!("{} {} {} {}\n", pad, eq, paint(color, BOLD, "note:"), n));
    }
    for h in &text_helps {
        out.push_str(&format!("{} {} {} {}\n", pad, eq, paint(color, BOLD, "help:"), h));
    }

    // ---- dicas com o codigo ja corrigido ----
    for s in &snippets {
        out.push_str(&format!(
            "{} {}\n",
            paint(color, CYAN, "help:"),
            paint(color, BOLD, &s.message)
        ));
        out.push_str(&blank);
        out.push_str(&src_line(s.line, &expand_tabs(&s.new_text)));
        if let Some((ch, n)) = s.marker {
            let mut content = " ".repeat(s.col);
            content.push_str(&paint(color, GREEN, &ch.to_string().repeat(n)));
            out.push_str(&marker_line(&content));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Span;

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut it = s.chars();
        while let Some(c) = it.next() {
            if c == '\x1b' {
                for d in it.by_ref() {
                    if d == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn color_only_adds_escapes() {
        let file = SourceFile::new("t.nihon", "束縛 x = 5;\nx = 6;\n".to_string());
        let d = Diagnostic::error("E0401", "cannot assign twice")
            .secondary(Span::new(7, 8), "first assignment")
            .primary(Span::new(file.line_start(2), file.line_start(2) + 5), "cannot assign twice")
            .note("a note")
            .suggest("make it mutable", Span::new(7, 7), "可変 ", true);
        let plain = render(&d, &file, false);
        let colored = render(&d, &file, true);
        assert!(colored.contains("\x1b["));
        assert_eq!(strip_ansi(&colored), plain);
        assert!(plain.contains("1 |"));
        assert!(plain.contains("2 |"));
    }

    #[test]
    fn two_labels_on_one_line_stack_their_messages() {
        let file = SourceFile::new("t.nihon", "束縛 合計 = 関数;\n".to_string());
        let d = Diagnostic::error("E0000", "demo")
            .primary(Span::new(file.text.find('関').unwrap(), file.text.find(';').unwrap()), "right")
            .secondary(Span::new(file.text.find('合').unwrap(), file.text.find('=').unwrap() - 1), "left");
        let out = render(&d, &file, false);
        let lines: Vec<&str> = out.lines().collect();
        // linha de codigo, marcadores com a mensagem da direita, conector, mensagem da esquerda
        assert!(lines.iter().any(|l| l.contains("^^^^ right")));
        assert!(lines.iter().any(|l| l.trim_end().ends_with("left")));
    }
}
