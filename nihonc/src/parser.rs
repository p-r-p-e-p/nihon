//! Parser do stage 0: descida recursiva para itens, blocos e comandos, e
//! precedencia de operadores (Pratt) para expressoes.
//!
//! Estrategia de erros: primeiro uma passada so de delimitadores (como o
//! rustc faz), depois a analise de verdade com recuperacao: um erro dentro de
//! um comando pula ate o proximo `;` ou `}` e continua, para mostrar varios
//! erros de uma vez.

use crate::ast::*;
use crate::diag::codes::*;
use crate::diag::{Diagnostic, Handler};
use crate::lexer::keywords::{info_of, is_future_keyword, Kw};
use crate::lexer::{Token, TokenKind};
use crate::source::{SourceFile, Span};
use TokenKind as T;

type PResult<T> = Result<T, ()>;

/// Devolve o programa so se nao houve nenhum erro.
pub fn parse(file: &SourceFile, tokens: &[Token], handler: &mut Handler) -> Option<Program> {
    if !check_delimiters(file, tokens, handler) {
        return None;
    }
    let before = handler.error_count();
    let mut p = Parser {
        file,
        tokens,
        pos: 0,
        no_struct: false,
        handler,
    };
    let program = p.parse_program();
    if p.handler.error_count() > before {
        None
    } else {
        Some(program)
    }
}

// ----------------------------------------------------------- delimitadores

fn delim_text(k: &TokenKind) -> &'static str {
    match k {
        T::LParen => "(",
        T::RParen => ")",
        T::LBrace => "{",
        T::RBrace => "}",
        T::LBracket => "[",
        T::RBracket => "]",
        _ => "?",
    }
}

fn closer_of(open: &TokenKind) -> TokenKind {
    match open {
        T::LParen => T::RParen,
        T::LBrace => T::RBrace,
        _ => T::RBracket,
    }
}

/// Confere `(` `)` `{` `}` `[` `]` antes de qualquer outra coisa.
fn check_delimiters(file: &SourceFile, tokens: &[Token], handler: &mut Handler) -> bool {
    let _ = file;
    let mut stack: Vec<(&TokenKind, Span)> = Vec::new();
    for t in tokens {
        match &t.kind {
            T::LParen | T::LBrace | T::LBracket => stack.push((&t.kind, t.span)),
            T::RParen | T::RBrace | T::RBracket => match stack.pop() {
                None => {
                    let d = Diagnostic::error(
                        E0103,
                        format!("unexpected closing delimiter: `{}`", delim_text(&t.kind)),
                    )
                    .primary(t.span, "unexpected closing delimiter");
                    handler.emit(d);
                    return false;
                }
                Some((open, open_span)) => {
                    if closer_of(open) != t.kind {
                        let d = Diagnostic::error(
                            E0103,
                            format!("mismatched closing delimiter: `{}`", delim_text(&t.kind)),
                        )
                        .secondary(open_span, "unclosed delimiter")
                        .primary(t.span, "mismatched closing delimiter");
                        handler.emit(d);
                        return false;
                    }
                }
            },
            _ => {}
        }
    }
    if stack.is_empty() {
        return true;
    }
    let mut d = Diagnostic::error(E0103, "this file contains an unclosed delimiter");
    let last = stack.len() - 1;
    for (i, (_, span)) in stack.iter().enumerate() {
        d = if i == last {
            d.primary(*span, "unclosed delimiter")
        } else {
            d.secondary(*span, "unclosed delimiter")
        };
    }
    handler.emit(d);
    false
}

// ------------------------------------------------------------------ parser

struct Parser<'a, 'h> {
    file: &'a SourceFile,
    tokens: &'a [Token],
    pos: usize,
    /// Dentro do cabecalho de `もし`/`間`/`各`, `nome { ... }` nao e um literal de struct.
    no_struct: bool,
    handler: &'h mut Handler,
}

enum Parsed {
    Stmt(Stmt),
    Tail(Expr),
}

fn infix_bp(k: &TokenKind) -> Option<(u8, u8, BinOp)> {
    Some(match k {
        T::OrOr => (4, 5, BinOp::Or),
        T::AndAnd => (6, 7, BinOp::And),
        T::EqEq => (8, 9, BinOp::Eq),
        T::NotEq => (8, 9, BinOp::Ne),
        T::Lt => (8, 9, BinOp::Lt),
        T::Gt => (8, 9, BinOp::Gt),
        T::LtEq => (8, 9, BinOp::Le),
        T::GtEq => (8, 9, BinOp::Ge),
        T::Plus => (10, 11, BinOp::Add),
        T::Minus => (10, 11, BinOp::Sub),
        T::Star => (12, 13, BinOp::Mul),
        T::Slash => (12, 13, BinOp::Div),
        T::Percent => (12, 13, BinOp::Rem),
        _ => return None,
    })
}

const BP_RANGE: u8 = 2;
const BP_CAST: u8 = 14;
const BP_PREFIX: u8 = 16;
const BP_CALL: u8 = 18;

fn assign_op(k: &TokenKind) -> Option<AssignOp> {
    Some(match k {
        T::Eq => AssignOp::Set,
        T::PlusEq => AssignOp::Add,
        T::MinusEq => AssignOp::Sub,
        T::StarEq => AssignOp::Mul,
        T::SlashEq => AssignOp::Div,
        T::PercentEq => AssignOp::Rem,
        _ => return None,
    })
}

impl<'a, 'h> Parser<'a, 'h> {
    // ------------------------------------------------------------- basico

    fn tok(&self) -> &'a Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn kind(&self) -> &'a TokenKind {
        &self.tok().kind
    }

    fn nth_kind(&self, n: usize) -> &'a TokenKind {
        &self.tokens[(self.pos + n).min(self.tokens.len() - 1)].kind
    }

    fn at_eof(&self) -> bool {
        matches!(self.kind(), T::Eof)
    }

    fn bump(&mut self) -> &'a Token {
        let t = self.tok();
        if !self.at_eof() {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, k: TokenKind) -> bool {
        if *self.kind() == k {
            self.bump();
            true
        } else {
            false
        }
    }

    fn prev_end(&self) -> usize {
        if self.pos == 0 {
            0
        } else {
            self.tokens[self.pos - 1].span.end
        }
    }

    fn text(&self, span: Span) -> &'a str {
        &self.file.text[span.range()]
    }

    fn describe(&self, t: &Token) -> String {
        if matches!(t.kind, T::Eof) {
            "end of file".to_string()
        } else {
            format!("`{}`", self.text(t.span))
        }
    }

    fn emit(&mut self, d: Diagnostic) {
        self.handler.emit(d);
    }

    /// Notas extras quando o token e uma palavra reservada.
    fn with_kw_notes(&self, mut d: Diagnostic, t: &Token) -> Diagnostic {
        if let T::Keyword(kw) = &t.kind {
            let info = info_of(*kw);
            let written = self.text(t.span);
            if written == info.romaji {
                d = d.note(format!(
                    "`{}` is the romaji spelling of the reserved keyword `{}` (\"{}\")",
                    written, info.jp, info.meaning
                ));
            } else {
                d = d.note(format!(
                    "`{}` is a reserved keyword ({}, \"{}\")",
                    written, info.romaji, info.meaning
                ));
            }
            if is_future_keyword(*kw) {
                d = d.note("this keyword is reserved, but the compiler does not support it yet");
            }
        }
        d
    }

    // -------------------------------------------------------------- erros

    /// "expected X, found Y" generico (E0110).
    fn err_expected(&mut self, what: &str) {
        let t = self.tok();
        let d = Diagnostic::error(
            E0110,
            format!("expected {}, found {}", what, self.describe(t)),
        )
        .primary(t.span, format!("expected {}", what));
        let d = self.with_kw_notes(d, t);
        self.emit(d);
    }

    fn expect(&mut self, want: TokenKind, what: &str) -> PResult<&'a Token> {
        if *self.kind() == want {
            Ok(self.bump())
        } else {
            self.err_expected(what);
            Err(())
        }
    }

    /// Falta `;`: avisa e segue como se ele existisse.
    fn expect_semi(&mut self) {
        if self.eat(T::Semi) {
            return;
        }
        let t = self.tok();
        let at = self.prev_end();
        let same_line = self.file.line_of(at) == self.file.line_of(t.span.start);
        let d = Diagnostic::error(
            E0102,
            format!("expected `;`, found {}", self.describe(t)),
        )
        .primary(Span::new(at, at), "expected `;`")
        .suggest("add `;` here", Span::new(at, at), ";", !same_line);
        let d = self.with_kw_notes(d, t);
        self.emit(d);
    }

    /// Nome. Palavra reservada gera E0004 mas segue como se fosse um nome.
    fn expect_ident(&mut self, what: &str) -> PResult<Ident> {
        let t = self.tok();
        match &t.kind {
            T::Ident(name) => {
                self.bump();
                Ok(Ident {
                    name: name.clone(),
                    span: t.span,
                })
            }
            T::Keyword(kw) => {
                let written = self.text(t.span);
                let info = info_of(*kw);
                let d = Diagnostic::error(
                    E0004,
                    format!("`{}` is a reserved keyword and cannot be used as a name", written),
                )
                .primary(t.span, "reserved keyword")
                .suggest(
                    "rename it, for example by adding an underscore",
                    Span::new(t.span.end, t.span.end),
                    "_",
                    false,
                );
                let d = self.with_kw_notes(d, t);
                let _ = (info, what);
                self.emit(d);
                self.bump();
                Ok(Ident {
                    name: written.to_string(),
                    span: t.span,
                })
            }
            _ => {
                let d = Diagnostic::error(
                    E0109,
                    format!("expected {}, found {}", what, self.describe(t)),
                )
                .primary(t.span, format!("expected {}", what));
                self.emit(d);
                Err(())
            }
        }
    }

    fn parse_type(&mut self) -> PResult<Type> {
        let start_tok = self.tok();
        let mut reference = None;
        if self.eat(T::Amp) {
            reference = Some(self.eat(T::Keyword(Kw::Mut)));
        }
        let t = self.tok();
        match &t.kind {
            T::Ident(_) | T::Keyword(Kw::SelfType) => {
                let name = match &t.kind {
                    T::Ident(n) => n.clone(),
                    _ => "自身".to_string(),
                };
                self.bump();
                Ok(Type {
                    name,
                    span: Span::new(start_tok.span.start, t.span.end),
                    reference,
                })
            }
            _ => {
                let d = Diagnostic::error(
                    E0104,
                    format!("expected type, found {}", self.describe(t)),
                )
                .primary(t.span, "expected type");
                let d = self.with_kw_notes(d, t);
                self.emit(d);
                Err(())
            }
        }
    }

    // ---------------------------------------------------------- recuperacao

    /// Pula um grupo `(...)`, `{...}` ou `[...]` ja balanceado.
    fn skip_group(&mut self) {
        let mut depth = 0usize;
        loop {
            match self.kind() {
                T::LParen | T::LBrace | T::LBracket => depth += 1,
                T::RParen | T::RBrace | T::RBracket => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        self.bump();
                        return;
                    }
                }
                T::Eof => return,
                _ => {}
            }
            self.bump();
        }
    }

    fn recover_stmt(&mut self, start: usize) {
        if self.pos == start {
            self.bump();
        }
        loop {
            match self.kind() {
                T::Semi => {
                    self.bump();
                    return;
                }
                T::RBrace | T::Eof => return,
                T::LBrace | T::LParen | T::LBracket => self.skip_group(),
                T::Keyword(
                    Kw::Let | Kw::Return | Kw::Break | Kw::Continue | Kw::If | Kw::While | Kw::Loop | Kw::For,
                ) => return,
                _ => {
                    self.bump();
                }
            }
        }
    }

    fn recover_item(&mut self, start: usize) {
        if self.pos == start {
            self.bump();
        }
        loop {
            match self.kind() {
                T::Eof | T::Keyword(Kw::Fn | Kw::Const | Kw::Struct | Kw::Impl | Kw::Pub) => return,
                T::LBrace | T::LParen | T::LBracket => self.skip_group(),
                _ => {
                    self.bump();
                }
            }
        }
    }

    // --------------------------------------------------------------- itens

    fn parse_program(&mut self) -> Program {
        let mut items = Vec::new();
        while !self.at_eof() {
            let start = self.pos;
            match self.parse_item() {
                Ok(item) => items.push(item),
                Err(()) => self.recover_item(start),
            }
        }
        Program { items }
    }

    fn parse_item(&mut self) -> PResult<Item> {
        // `公開` (koukai, "public") e aceito e ignorado por enquanto.
        self.eat(T::Keyword(Kw::Pub));
        match self.kind() {
            T::Keyword(Kw::Fn) => Ok(Item::Fn(self.parse_fn()?)),
            T::Keyword(Kw::Const) => self.parse_const(),
            T::Keyword(Kw::Struct) => self.parse_struct(),
            T::Keyword(Kw::Impl) => self.parse_impl(),
            _ => {
                let t = self.tok();
                let d = Diagnostic::error(
                    E0108,
                    format!("expected item, found {}", self.describe(t)),
                )
                .primary(t.span, "expected `関数`, `定数`, `構造体` or `実装`")
                .note("only functions `関数` (kansuu, \"function\"), constants `定数` (teisuu, \"constant\"), structs `構造体` (kouzoutai, \"struct\") and `実装` (jissou, \"implementation\") blocks may appear at the top level");
                let d = self.with_kw_notes(d, t);
                self.emit(d);
                Err(())
            }
        }
    }

    /// Tenta ler `自分`, `可変 自分`, `&自分` ou `&可変 自分` como primeiro parametro.
    fn parse_self_param(&mut self) -> Option<SelfParam> {
        let start = self.tok().span.start;
        let (kind, mutable, skip) = match (self.kind(), self.nth_kind(1), self.nth_kind(2)) {
            (T::Keyword(Kw::SelfValue), _, _) => (SelfKind::Value, false, 1),
            (T::Keyword(Kw::Mut), T::Keyword(Kw::SelfValue), _) => (SelfKind::Value, true, 2),
            (T::Amp, T::Keyword(Kw::SelfValue), _) => (SelfKind::Ref, false, 2),
            (T::Amp, T::Keyword(Kw::Mut), T::Keyword(Kw::SelfValue)) => (SelfKind::RefMut, false, 3),
            _ => return None,
        };
        for _ in 0..skip {
            self.bump();
        }
        Some(SelfParam {
            kind,
            mutable,
            span: Span::new(start, self.prev_end()),
        })
    }

    fn parse_fn(&mut self) -> PResult<FnItem> {
        let fn_tok = self.bump();
        let name = self.expect_ident("a function name")?;
        self.expect(T::LParen, "`(`")?;
        let mut params = Vec::new();
        let self_param = self.parse_self_param();
        if self_param.is_some() && !self.eat(T::Comma) && *self.kind() != T::RParen {
            self.err_expected("`,` or `)`");
            return Err(());
        }
        while *self.kind() != T::RParen {
            let pname = self.expect_ident("a parameter name")?;
            self.expect(T::Colon, "`:`")?;
            let ty = self.parse_type()?;
            params.push(Param { name: pname, ty });
            if !self.eat(T::Comma) {
                break;
            }
        }
        self.expect(T::RParen, "`,` or `)`")?;
        let ret = if self.eat(T::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = self.parse_block()?;
        let span = Span::new(fn_tok.span.start, body.span.end);
        Ok(FnItem {
            name,
            self_param,
            params,
            ret,
            body,
            span,
        })
    }

    fn parse_struct(&mut self) -> PResult<Item> {
        let kw = self.bump();
        let name = self.expect_ident("a struct name")?;
        self.expect(T::LBrace, "`{`")?;
        let mut fields = Vec::new();
        while *self.kind() != T::RBrace {
            self.eat(T::Keyword(Kw::Pub));
            let fname = self.expect_ident("a field name")?;
            self.expect(T::Colon, "`:`")?;
            let ty = self.parse_type()?;
            fields.push(Field { name: fname, ty });
            if !self.eat(T::Comma) {
                break;
            }
        }
        self.expect(T::RBrace, "`,` or `}`")?;
        Ok(Item::Struct(StructItem {
            name,
            fields,
            span: Span::new(kw.span.start, self.prev_end()),
        }))
    }

    fn parse_impl(&mut self) -> PResult<Item> {
        let kw = self.bump();
        let ty = self.expect_ident("a struct name")?;
        self.expect(T::LBrace, "`{`")?;
        let mut fns = Vec::new();
        while *self.kind() != T::RBrace {
            self.eat(T::Keyword(Kw::Pub));
            if *self.kind() != T::Keyword(Kw::Fn) {
                self.err_expected("`関数` (kansuu, \"function\")");
                return Err(());
            }
            fns.push(self.parse_fn()?);
        }
        self.expect(T::RBrace, "`}`")?;
        Ok(Item::Impl(ImplItem {
            ty,
            fns,
            span: Span::new(kw.span.start, self.prev_end()),
        }))
    }

    fn parse_const(&mut self) -> PResult<Item> {
        let kw = self.bump();
        let name = self.expect_ident("a constant name")?;
        self.expect(T::Colon, "`:`")?;
        let ty = self.parse_type()?;
        self.expect(T::Eq, "`=`")?;
        let value = self.parse_expr(0)?;
        self.expect_semi();
        let span = Span::new(kw.span.start, self.prev_end());
        Ok(Item::Const(ConstItem {
            name,
            ty,
            value,
            span,
        }))
    }

    // --------------------------------------------------------------- blocos

    fn parse_block(&mut self) -> PResult<Block> {
        let open = self.tok();
        if open.kind != T::LBrace {
            let d = Diagnostic::error(
                E0101,
                format!("expected `{{`, found {}", self.describe(open)),
            )
            .primary(open.span, "expected `{`");
            let d = self.with_kw_notes(d, open);
            self.emit(d);
            return Err(());
        }
        self.bump();
        let saved_ns = std::mem::replace(&mut self.no_struct, false);
        let mut stmts = Vec::new();
        let mut tail = None;
        while !matches!(self.kind(), T::RBrace | T::Eof) {
            let start = self.pos;
            match self.parse_stmt() {
                Ok(Parsed::Stmt(s)) => stmts.push(s),
                Ok(Parsed::Tail(e)) => {
                    tail = Some(Box::new(e));
                    break;
                }
                Err(()) => self.recover_stmt(start),
            }
        }
        self.no_struct = saved_ns;
        let close = self.expect(T::RBrace, "`}`")?;
        Ok(Block {
            stmts,
            tail,
            span: Span::new(open.span.start, close.span.end),
        })
    }

    fn parse_stmt(&mut self) -> PResult<Parsed> {
        let first = self.tok();
        let start = first.span.start;
        let stmt = |kind: StmtKind, end: usize| Parsed::Stmt(Stmt {
            kind,
            span: Span::new(start, end),
        });
        match self.kind() {
            T::Semi => {
                self.bump();
                Ok(stmt(StmtKind::Empty, self.prev_end()))
            }
            T::Keyword(Kw::Let) => {
                self.bump();
                let mutable = self.eat(T::Keyword(Kw::Mut));
                let name = self.expect_ident("a variable name")?;
                let ty = if self.eat(T::Colon) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                self.expect(T::Eq, "`=`")?;
                let init = self.parse_expr(0)?;
                self.expect_semi();
                Ok(stmt(
                    StmtKind::Let {
                        mutable,
                        name,
                        ty,
                        init,
                    },
                    self.prev_end(),
                ))
            }
            T::Keyword(Kw::Return) => {
                self.bump();
                let value = if matches!(self.kind(), T::Semi | T::RBrace) {
                    None
                } else {
                    Some(self.parse_expr(0)?)
                };
                self.expect_semi();
                Ok(stmt(StmtKind::Return(value), self.prev_end()))
            }
            T::Keyword(Kw::Break) => {
                self.bump();
                self.expect_semi();
                Ok(stmt(StmtKind::Break, self.prev_end()))
            }
            T::Keyword(Kw::Continue) => {
                self.bump();
                self.expect_semi();
                Ok(stmt(StmtKind::Continue, self.prev_end()))
            }
            T::Keyword(Kw::Else | Kw::ElseIf) => {
                let t = self.tok();
                let written = self.text(t.span);
                let (jp, ro, mean) = if matches!(t.kind, T::Keyword(Kw::Else)) {
                    ("他", "hoka", "else")
                } else {
                    ("他もし", "hokamoshi", "else if")
                };
                let d = Diagnostic::error(
                    E0107,
                    format!("`{}` without a preceding `もし`", written),
                )
                .primary(t.span, "no `もし` (moshi, \"if\") before this")
                .note(format!("`{}` ({}, \"{}\") must directly follow the `}}` of a `もし` block", jp, ro, mean));
                self.emit(d);
                Err(())
            }
            T::Keyword(Kw::If | Kw::While | Kw::Loop | Kw::For) | T::LBrace => {
                let e = self.parse_block_like()?;
                if self.eat(T::Semi) {
                    return Ok(stmt(StmtKind::Expr(e), self.prev_end()));
                }
                if matches!(self.kind(), T::RBrace) {
                    return Ok(Parsed::Tail(e));
                }
                Ok(stmt(StmtKind::Expr(e), self.prev_end()))
            }
            _ => {
                let e = self.parse_expr(0)?;
                if let Some(op) = assign_op(self.kind()) {
                    self.bump();
                    let value = self.parse_expr(0)?;
                    self.expect_semi();
                    return Ok(stmt(
                        StmtKind::Assign { target: e, op, value },
                        self.prev_end(),
                    ));
                }
                if self.eat(T::Semi) {
                    return Ok(stmt(StmtKind::Expr(e), self.prev_end()));
                }
                if matches!(self.kind(), T::RBrace) {
                    return Ok(Parsed::Tail(e));
                }
                self.expect_semi();
                Ok(stmt(StmtKind::Expr(e), self.prev_end()))
            }
        }
    }

    // ------------------------------------------------- expressoes com bloco

    fn parse_block_like(&mut self) -> PResult<Expr> {
        match self.kind() {
            T::Keyword(Kw::If | Kw::ElseIf) => self.parse_if(),
            T::Keyword(Kw::While) => {
                let kw = self.bump();
                let cond = self.parse_cond()?;
                let body = self.parse_block()?;
                let span = Span::new(kw.span.start, body.span.end);
                Ok(Expr {
                    kind: ExprKind::While {
                        cond: Box::new(cond),
                        body,
                    },
                    span,
                })
            }
            T::Keyword(Kw::Loop) => {
                let kw = self.bump();
                let body = self.parse_block()?;
                let span = Span::new(kw.span.start, body.span.end);
                Ok(Expr {
                    kind: ExprKind::Loop { body },
                    span,
                })
            }
            T::Keyword(Kw::For) => self.parse_for(),
            _ => {
                let body = self.parse_block()?;
                let span = body.span;
                Ok(Expr {
                    kind: ExprKind::Block(body),
                    span,
                })
            }
        }
    }

    fn parse_cond(&mut self) -> PResult<Expr> {
        let saved = std::mem::replace(&mut self.no_struct, true);
        let r = self.parse_expr(0);
        self.no_struct = saved;
        r
    }

    fn parse_if(&mut self) -> PResult<Expr> {
        let kw = self.bump(); // `もし` ou `他もし`
        let cond = self.parse_cond()?;
        let then = self.parse_block()?;
        let mut end = then.span.end;
        let else_ = match self.kind() {
            T::Keyword(Kw::ElseIf) => {
                let e = self.parse_if()?;
                end = e.span.end;
                Some(Box::new(e))
            }
            T::Keyword(Kw::Else) => {
                let else_tok = self.bump();
                if let T::Keyword(Kw::If) = self.kind() {
                    // `他 もし` com espaco: e uma palavra so (`他もし`)
                    let if_tok = self.tok();
                    let d = Diagnostic::error(
                        E0101,
                        format!("expected `{{`, found {}", self.describe(if_tok)),
                    )
                    .primary(if_tok.span, "expected `{`")
                    .suggest(
                        "`他もし` (hokamoshi, \"else if\") is one word, write it without a space",
                        Span::new(else_tok.span.start, if_tok.span.end),
                        "他もし",
                        true,
                    );
                    self.emit(d);
                    let e = self.parse_if()?;
                    end = e.span.end;
                    Some(Box::new(e))
                } else {
                    let block = self.parse_block()?;
                    end = block.span.end;
                    let span = block.span;
                    Some(Box::new(Expr {
                        kind: ExprKind::Block(block),
                        span,
                    }))
                }
            }
            _ => None,
        };
        Ok(Expr {
            kind: ExprKind::If {
                cond: Box::new(cond),
                then,
                else_,
            },
            span: Span::new(kw.span.start, end),
        })
    }

    fn parse_for(&mut self) -> PResult<Expr> {
        let kw = self.bump();
        let var = self.expect_ident("a loop variable")?;
        if !self.eat(T::Keyword(Kw::In)) {
            let t = self.tok();
            let at = var.span.end;
            let d = Diagnostic::error(
                E0106,
                format!("expected `中` in `各` loop, found {}", self.describe(t)),
            )
            .primary(t.span, "expected `中`")
            .note("`中` is read naka (\"inside\"): write `各 i 中 1..10 { ... }`")
            .suggest("add `中` after the loop variable", Span::new(at, at), " 中", true);
            let d = self.with_kw_notes(d, t);
            self.emit(d);
        }
        let iter = self.parse_cond()?;
        let body = self.parse_block()?;
        let span = Span::new(kw.span.start, body.span.end);
        Ok(Expr {
            kind: ExprKind::For {
                var,
                iter: Box::new(iter),
                body,
            },
            span,
        })
    }

    // ------------------------------------------------------------ expressoes

    fn parse_args(&mut self) -> PResult<Vec<Expr>> {
        let saved = std::mem::replace(&mut self.no_struct, false);
        let mut args = Vec::new();
        while *self.kind() != T::RParen {
            match self.parse_expr(0) {
                Ok(e) => args.push(e),
                Err(()) => {
                    self.no_struct = saved;
                    return Err(());
                }
            }
            if !self.eat(T::Comma) {
                break;
            }
        }
        self.no_struct = saved;
        self.expect(T::RParen, "`,` or `)`")?;
        Ok(args)
    }

    fn parse_expr(&mut self, min_bp: u8) -> PResult<Expr> {
        let mut lhs = self.parse_prefix()?;
        loop {
            match self.kind() {
                T::LParen if BP_CALL >= min_bp => {
                    self.bump();
                    let args = self.parse_args()?;
                    let span = Span::new(lhs.span.start, self.prev_end());
                    lhs = Expr {
                        kind: ExprKind::Call {
                            callee: Box::new(lhs),
                            args,
                        },
                        span,
                    };
                }
                T::Dot if BP_CALL >= min_bp => {
                    self.bump();
                    let name = self.expect_ident("a field or method name")?;
                    if *self.kind() != T::LParen {
                        let span = Span::new(lhs.span.start, name.span.end);
                        lhs = Expr {
                            kind: ExprKind::Field {
                                base: Box::new(lhs),
                                name,
                            },
                            span,
                        };
                        continue;
                    }
                    self.bump();
                    let args = self.parse_args()?;
                    let span = Span::new(lhs.span.start, self.prev_end());
                    lhs = Expr {
                        kind: ExprKind::MethodCall {
                            recv: Box::new(lhs),
                            name,
                            args,
                        },
                        span,
                    };
                }
                T::Keyword(Kw::As) if BP_CAST >= min_bp => {
                    self.bump();
                    let ty = self.parse_type()?;
                    let span = Span::new(lhs.span.start, ty.span.end);
                    lhs = Expr {
                        kind: ExprKind::Cast {
                            expr: Box::new(lhs),
                            ty,
                        },
                        span,
                    };
                }
                T::DotDot | T::DotDotEq if BP_RANGE >= min_bp => {
                    let inclusive = matches!(self.kind(), T::DotDotEq);
                    self.bump();
                    let rhs = self.parse_expr(BP_RANGE + 1)?;
                    let span = Span::new(lhs.span.start, rhs.span.end);
                    lhs = Expr {
                        kind: ExprKind::Range {
                            start: Box::new(lhs),
                            end: Box::new(rhs),
                            inclusive,
                        },
                        span,
                    };
                    // intervalos nao encadeiam
                    break;
                }
                k => {
                    let Some((lbp, rbp, op)) = infix_bp(k) else {
                        break;
                    };
                    if lbp < min_bp {
                        break;
                    }
                    self.bump();
                    let rhs = self.parse_expr(rbp)?;
                    let span = Span::new(lhs.span.start, rhs.span.end);
                    lhs = Expr {
                        kind: ExprKind::Binary {
                            op,
                            lhs: Box::new(lhs),
                            rhs: Box::new(rhs),
                        },
                        span,
                    };
                }
            }
        }
        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> PResult<Expr> {
        let t = self.tok();
        let op = match &t.kind {
            T::Minus => Some(UnOp::Neg),
            T::Bang => Some(UnOp::Not),
            _ => None,
        };
        if matches!(t.kind, T::Amp) {
            self.bump();
            let mutable = self.eat(T::Keyword(Kw::Mut));
            let e = self.parse_expr(BP_PREFIX)?;
            let span = Span::new(t.span.start, e.span.end);
            return Ok(Expr {
                kind: ExprKind::Ref {
                    mutable,
                    expr: Box::new(e),
                },
                span,
            });
        }
        if let Some(op) = op {
            self.bump();
            let e = self.parse_expr(BP_PREFIX)?;
            let span = Span::new(t.span.start, e.span.end);
            return Ok(Expr {
                kind: ExprKind::Unary {
                    op,
                    expr: Box::new(e),
                },
                span,
            });
        }
        self.parse_primary()
    }

    fn parse_struct_lit(&mut self, name: Ident) -> PResult<Expr> {
        self.bump(); // `{`
        let saved = std::mem::replace(&mut self.no_struct, false);
        let mut fields = Vec::new();
        while *self.kind() != T::RBrace {
            let f = self.expect_ident("a field name")?;
            self.expect(T::Colon, "`:`")?;
            let v = self.parse_expr(0)?;
            fields.push((f, v));
            if !self.eat(T::Comma) {
                break;
            }
        }
        self.no_struct = saved;
        self.expect(T::RBrace, "`,` or `}`")?;
        let span = Span::new(name.span.start, self.prev_end());
        Ok(Expr {
            kind: ExprKind::StructLit { name, fields },
            span,
        })
    }

    fn parse_primary(&mut self) -> PResult<Expr> {
        let t = self.tok();
        let lit = |kind: ExprKind| Expr {
            kind,
            span: t.span,
        };
        match &t.kind {
            T::Int(v) => {
                self.bump();
                Ok(lit(ExprKind::Int(*v)))
            }
            T::Float(s) => {
                self.bump();
                Ok(lit(ExprKind::Float(s.clone())))
            }
            T::Str(s) => {
                self.bump();
                Ok(lit(ExprKind::Str(s.clone())))
            }
            T::Char(c) => {
                self.bump();
                Ok(lit(ExprKind::Char(*c)))
            }
            T::Keyword(Kw::True) => {
                self.bump();
                Ok(lit(ExprKind::Bool(true)))
            }
            T::Keyword(Kw::False) => {
                self.bump();
                Ok(lit(ExprKind::Bool(false)))
            }
            T::Ident(_) | T::Keyword(Kw::SelfValue | Kw::SelfType) => {
                let name = match &t.kind {
                    T::Ident(n) => n.clone(),
                    T::Keyword(Kw::SelfValue) => "自分".to_string(),
                    _ => "自身".to_string(),
                };
                self.bump();
                let id = Ident {
                    name,
                    span: t.span,
                };
                if self.eat(T::ColonColon) {
                    let member = self.expect_ident("a function name")?;
                    let span = Span::new(t.span.start, member.span.end);
                    return Ok(Expr {
                        kind: ExprKind::AssocPath { ty: id, name: member },
                        span,
                    });
                }
                if !self.no_struct && *self.kind() == T::LBrace {
                    let looks_like_lit = match (self.nth_kind(1), self.nth_kind(2)) {
                        (T::RBrace, _) => true,
                        (T::Ident(_), T::Colon) => true,
                        _ => false,
                    };
                    if looks_like_lit {
                        return self.parse_struct_lit(id);
                    }
                }
                Ok(lit(ExprKind::Path(id)))
            }
            T::Macro(name) => {
                self.bump();
                let name_span = Span::new(t.span.start, t.span.end - 1);
                self.expect(T::LParen, "`(`")?;
                let args = self.parse_args()?;
                Ok(Expr {
                    kind: ExprKind::MacroCall {
                        name: name.clone(),
                        name_span,
                        args,
                    },
                    span: Span::new(t.span.start, self.prev_end()),
                })
            }
            T::LParen => {
                self.bump();
                let saved = std::mem::replace(&mut self.no_struct, false);
                let inner = self.parse_expr(0);
                self.no_struct = saved;
                let inner = inner?;
                self.expect(T::RParen, "`)`")?;
                Ok(Expr {
                    kind: ExprKind::Paren(Box::new(inner)),
                    span: Span::new(t.span.start, self.prev_end()),
                })
            }
            T::Keyword(Kw::If | Kw::While | Kw::Loop | Kw::For) | T::LBrace => self.parse_block_like(),
            _ => {
                let d = Diagnostic::error(
                    E0105,
                    format!("expected expression, found {}", self.describe(t)),
                )
                .primary(t.span, "expected expression");
                let d = self.with_kw_notes(d, t);
                self.emit(d);
                Err(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Config;
    use crate::lexer::lex;

    fn parse_src(src: &str) -> (Option<Program>, Vec<Diagnostic>) {
        let file = SourceFile::new("t.nihon", src.to_string());
        let mut h = Handler::new(Config::default());
        let toks = lex(&file, &mut h);
        assert!(h.diags.is_empty(), "erro lexico no teste: {:?}", h.diags);
        let prog = parse(&file, &toks, &mut h);
        (prog, h.diags)
    }

    fn s(e: &Expr) -> String {
        match &e.kind {
            ExprKind::Int(v) => v.to_string(),
            ExprKind::Float(f) => f.clone(),
            ExprKind::Str(x) => format!("{:?}", x),
            ExprKind::Char(c) => format!("{:?}", c),
            ExprKind::Bool(b) => b.to_string(),
            ExprKind::Path(i) => i.name.clone(),
            ExprKind::Paren(x) => format!("(group {})", s(x)),
            ExprKind::Unary { op, expr } => format!("({} {})", if *op == UnOp::Neg { "neg" } else { "not" }, s(expr)),
            ExprKind::Binary { op, lhs, rhs } => format!("({:?} {} {})", op, s(lhs), s(rhs)),
            ExprKind::Cast { expr, ty } => format!("(as {} {})", s(expr), ty.name),
            ExprKind::Range { start, end, inclusive } => {
                format!("({} {} {})", if *inclusive { "..=" } else { ".." }, s(start), s(end))
            }
            ExprKind::Call { callee, args } => {
                let a: Vec<String> = args.iter().map(s).collect();
                format!("(call {} {})", s(callee), a.join(" "))
            }
            ExprKind::MethodCall { recv, name, args } => {
                let a: Vec<String> = args.iter().map(s).collect();
                format!("(method {} {} {})", s(recv), name.name, a.join(" "))
            }
            ExprKind::Field { base, name } => format!("(field {} {})", s(base), name.name),
            ExprKind::AssocPath { ty, name } => format!("(path {}::{})", ty.name, name.name),
            ExprKind::StructLit { name, fields } => {
                let f: Vec<String> = fields.iter().map(|(n, v)| format!("{}={}", n.name, s(v))).collect();
                format!("(lit {} {})", name.name, f.join(" "))
            }
            ExprKind::Ref { mutable, expr } => format!("({} {})", if *mutable { "&mut" } else { "&" }, s(expr)),
            ExprKind::MacroCall { name, args, .. } => {
                let a: Vec<String> = args.iter().map(s).collect();
                format!("(macro {} {})", name, a.join(" "))
            }
            ExprKind::If { cond, else_, .. } => match else_ {
                Some(e) => format!("(if {} .. else {})", s(cond), s(e)),
                None => format!("(if {} ..)", s(cond)),
            },
            ExprKind::While { cond, .. } => format!("(while {})", s(cond)),
            ExprKind::Loop { .. } => "(loop)".to_string(),
            ExprKind::For { var, iter, .. } => format!("(for {} {})", var.name, s(iter)),
            ExprKind::Block(_) => "(block)".to_string(),
        }
    }

    /// Expressao do `束縛 x = <expr>;` dentro de uma funcao.
    fn expr_of(src_expr: &str) -> String {
        let src = format!("関数 f() {{ 束縛 x = {}; }}", src_expr);
        let (prog, diags) = parse_src(&src);
        assert!(diags.is_empty(), "{:?}", diags);
        let Item::Fn(f) = &prog.unwrap().items[0] else {
            panic!()
        };
        let StmtKind::Let { init, .. } = &f.body.stmts[0].kind else {
            panic!()
        };
        s(init)
    }

    fn codes(d: &[Diagnostic]) -> Vec<&'static str> {
        d.iter().map(|x| x.code).collect()
    }

    #[test]
    fn precedence() {
        assert_eq!(expr_of("1 + 2 * 3"), "(Add 1 (Mul 2 3))");
        assert_eq!(expr_of("1 * 2 + 3"), "(Add (Mul 1 2) 3)");
        assert_eq!(expr_of("1 - 2 - 3"), "(Sub (Sub 1 2) 3)");
        assert_eq!(expr_of("a || b && c"), "(Or a (And b c))");
        assert_eq!(expr_of("a < b == c"), "(Eq (Lt a b) c)");
        assert_eq!(expr_of("(1 + 2) * 3"), "(Mul (group (Add 1 2)) 3)");
    }

    #[test]
    fn unary_cast_call_and_range() {
        assert_eq!(expr_of("-a * b"), "(Mul (neg a) b)");
        assert_eq!(expr_of("!a && b"), "(And (not a) b)");
        assert_eq!(expr_of("a として 実64 * b"), "(Mul (as a 実64) b)");
        assert_eq!(expr_of("-a として 実64"), "(as (neg a) 実64)");
        assert_eq!(expr_of("f(1, g(2),)"), "(call f 1 (call g 2))");
        assert_eq!(expr_of("1..=n + 1"), "(..= 1 (Add n 1))");
        assert_eq!(expr_of("表示!(\"a\", 1)"), "(macro 表示 \"a\" 1)");
        assert_eq!(expr_of("a.長さ() + 1"), "(Add (method a 長さ ) 1)");
        assert_eq!(expr_of("\"x\".長さ().f(2)"), "(method (method \"x\" 長さ ) f 2)");
    }

    #[test]
    fn method_calls() {
        assert_eq!(expr_of("a.長さ()"), "(method a 長さ )");
        assert_eq!(expr_of("a.f(1).g(2, 3)"), "(method (method a f 1) g 2 3)");
        assert_eq!(expr_of("-a.f() + 1"), "(Add (neg (method a f )) 1)");
    }

    #[test]
    fn fields_refs_and_paths() {
        assert_eq!(expr_of("a.x"), "(field a x)");
        assert_eq!(expr_of("a.b.c + 1"), "(Add (field (field a b) c) 1)");
        assert_eq!(expr_of("a.f(1).x"), "(field (method a f 1) x)");
        assert_eq!(expr_of("点::新(1)"), "(call (path 点::新) 1)");
        assert_eq!(expr_of("点 { x: 1, y: 2 }"), "(lit 点 x=1 y=2)");
        assert_eq!(expr_of("点 { }"), "(lit 点 )");
        assert_eq!(expr_of("&p.x"), "(& (field p x))");
        assert_eq!(expr_of("&可変 p"), "(&mut p)");
        assert_eq!(expr_of("自身::新()"), "(call (path 自身::新) )");
    }

    #[test]
    fn struct_literal_is_not_an_if_body() {
        // `もし x { ... }`: o `{` abre o bloco, nao um literal
        let e = expr_of("もし x { 1 } 他 { 2 }");
        assert!(e.starts_with("(if x .."), "{}", e);
        let e = expr_of("もし x {} 他 {}");
        assert!(e.starts_with("(if x .."), "{}", e);
        // entre parenteses o literal volta a valer
        let e = expr_of("もし x == (点 { x: 1, y: 2 }).x { 1 }");
        assert!(e.starts_with("(if (Eq x (field"), "{}", e);
    }

    #[test]
    fn items_struct_and_impl() {
        let src = "公開 構造体 点 { 公開 x: 整32, y: &点 }\n実装 点 { 公開 関数 f(&可変 自分, a: 整32) { } 関数 g(自分) { } 関数 h() { } }";
        let file = SourceFile::new("t.nihon", src.to_string());
        let mut h = Handler::new(Config::default());
        let toks = lex(&file, &mut h);
        let prog = parse(&file, &toks, &mut h).expect("parses");
        assert!(h.diags.is_empty(), "{:?}", codes(&h.diags));
        let Item::Struct(st) = &prog.items[0] else { panic!() };
        assert_eq!(st.fields.len(), 2);
        assert_eq!(st.fields[1].ty.reference, Some(false));
        let Item::Impl(im) = &prog.items[1] else { panic!() };
        let kinds: Vec<_> = im.fns.iter().map(|f| f.self_param.map(|s| s.kind)).collect();
        assert_eq!(kinds, vec![Some(SelfKind::RefMut), Some(SelfKind::Value), None]);
        assert_eq!(im.fns[0].params.len(), 1);
    }

    #[test]
    fn field_assignment_is_a_statement() {
        let file = SourceFile::new("t.nihon", "関数 f() { p.x = 1; p.a.b += 2; x = 3; }".to_string());
        let mut h = Handler::new(Config::default());
        let toks = lex(&file, &mut h);
        let prog = parse(&file, &toks, &mut h).expect("parses");
        let Item::Fn(f) = &prog.items[0] else { panic!() };
        assert_eq!(f.body.stmts.len(), 3);
        assert!(f.body.stmts.iter().all(|s| matches!(s.kind, StmtKind::Assign { .. })));
    }

    #[test]
    fn if_chain_is_an_expression() {
        let e = expr_of("もし a { 1 } 他もし b { 2 } 他 { 3 }");
        assert_eq!(e, "(if a .. else (if b .. else (block)))");
    }

    #[test]
    fn tail_expression_versus_statement() {
        let (prog, d) = parse_src("関数 f() -> 整32 { 束縛 a = 1; a + 1 }");
        assert!(d.is_empty());
        let Item::Fn(f) = &prog.unwrap().items[0] else { panic!() };
        assert_eq!(f.body.stmts.len(), 1);
        assert_eq!(s(f.body.tail.as_ref().unwrap()), "(Add a 1)");

        // com `;` no fim nao ha valor de bloco
        let (prog, _) = parse_src("関数 f() { 束縛 a = 1; a + 1; }");
        let Item::Fn(f) = &prog.unwrap().items[0] else { panic!() };
        assert!(f.body.tail.is_none());
        assert_eq!(f.body.stmts.len(), 2);
    }

    #[test]
    fn block_like_statements_need_no_semicolon() {
        let src = "関数 f() { もし a { } 間 b { } 無限 { 中断; } 各 i 中 0..3 { } 束縛 x = 1; }";
        let (prog, d) = parse_src(src);
        assert!(d.is_empty(), "{:?}", d);
        let Item::Fn(f) = &prog.unwrap().items[0] else { panic!() };
        assert_eq!(f.body.stmts.len(), 5);
    }

    #[test]
    fn assignment_statements() {
        let (prog, d) = parse_src("関数 f() { 束縛 可変 x = 0; x += 1; x = x * 2; x == 1; }");
        assert!(d.is_empty());
        let Item::Fn(f) = &prog.unwrap().items[0] else { panic!() };
        assert!(matches!(f.body.stmts[1].kind, StmtKind::Assign { op: AssignOp::Add, .. }));
        assert!(matches!(f.body.stmts[2].kind, StmtKind::Assign { op: AssignOp::Set, .. }));
        assert!(matches!(f.body.stmts[3].kind, StmtKind::Expr(_)));
    }

    #[test]
    fn items_and_params() {
        let (prog, d) = parse_src("定数 N: 整32 = 10;\n関数 soma(a: 整32, b: 整32,) -> 整32 { a + b }");
        assert!(d.is_empty());
        let items = prog.unwrap().items;
        assert!(matches!(items[0], Item::Const(_)));
        let Item::Fn(f) = &items[1] else { panic!() };
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.ret.as_ref().unwrap().name, "整32");
    }

    #[test]
    fn syntax_errors_have_the_right_codes() {
        let cases: &[(&str, &str)] = &[
            ("関数 f() { 束縛 x = ; }", "E0105"),
            ("関数 f() { 束縛 x = 1 }", "E0102"),
            ("関数 f() { もし a 返却; }", "E0101"),
            ("関数 f() { 他 { } }", "E0107"),
            ("関数 f() { 各 i 0..3 { } }", "E0106"),
            ("関数 f(a: ) { }", "E0104"),
            ("関数 f(a 整32) { }", "E0110"),
            ("関数 5() { }", "E0109"),
            ("束縛 x = 1;", "E0108"),
            ("関数 f() { 束縛 中 = 1; }", "E0004"),
            ("関数 f() { 束縛 naka = 1; }", "E0004"),
            ("関数 f() { もし a { } 他 もし b { } }", "E0101"),
            ("関数 f() { f(1 }", "E0103"),
            ("関数 f() { ", "E0103"),
            ("関数 f() { } }", "E0103"),
        ];
        for (src, code) in cases {
            let file = SourceFile::new("t.nihon", src.to_string());
            let mut h = Handler::new(Config::default());
            let toks = lex(&file, &mut h);
            let prog = parse(&file, &toks, &mut h);
            assert!(prog.is_none(), "{:?} deveria falhar", src);
            // avisos (W0001 do romaji) podem vir antes: so o primeiro ERRO conta
            let first_err = h.diags.iter().find(|x| x.level == crate::diag::Level::Error).map(|x| x.code);
            assert_eq!(first_err, Some(*code), "para {:?}: {:?}", src, h.diags);
        }
    }

    #[test]
    fn several_errors_in_one_run() {
        let src = "関数 f() {\n 束縛 a = ;\n 束縛 b = 2\n 束縛 c = 3;\n}\n関数 g() { 他 { } }";
        let file = SourceFile::new("t.nihon", src.to_string());
        let mut h = Handler::new(Config::default());
        let toks = lex(&file, &mut h);
        let _ = parse(&file, &toks, &mut h);
        assert_eq!(codes(&h.diags), vec!["E0105", "E0102", "E0107"]);
    }

    #[test]
    fn recovery_always_makes_progress() {
        // lixo variado nao pode travar o parser
        for src in ["関数", "関数 f", "関数 f(", "} {", "関数 f() { 束縛 }", "関数 f() { ) }", "もし もし もし", ";;;"] {
            let file = SourceFile::new("t.nihon", src.to_string());
            let mut h = Handler::new(Config::default());
            let toks = lex(&file, &mut h);
            let _ = parse(&file, &toks, &mut h);
        }
    }
}
