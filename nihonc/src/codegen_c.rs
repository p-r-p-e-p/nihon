//! Verificacao minima (nomes, tipos, mutabilidade) e geracao de codigo C.
//!
//! Tudo acontece numa passada so: ao percorrer a arvore o compilador confere
//! os tipos e ja escreve o C. Se houve algum erro, o C e descartado.
//!
//! Regras de geracao:
//! - chamadas de funcao e contas inteiras (que podem dar panic por estouro)
//!   viram temporarios `tN`, para manter a ordem de avaliacao da esquerda
//!   para a direita, como no Rust;
//! - nomes de variaveis locais ficam unicos (`v3_x`), entao sombreamento
//!   (`束縛 x = x + 1;`) nao precisa de escopos de C;
//! - nomes nao-ASCII sao codificados (`階乗` vira `nh_968e_4e57_`).

use crate::ast::*;
use crate::diag::codes::*;
use crate::diag::{Diagnostic, Handler};
use crate::lexer::keywords::{lookup_macro, MACROS};
use crate::source::{SourceFile, Span};
use std::collections::HashMap;

// ------------------------------------------------------------------- tipos

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ty {
    I32,
    U32,
    F64,
    Bool,
    Char,
    Str,
    /// Struct do programa (indice em `Cg::structs`).
    Struct(u16),
    /// `&T` (`false`) ou `&可変 T` (`true`), so para structs.
    Ref(u16, bool),
    Unit,
    /// Expressao que nunca termina (`返却`, `異常終了!`...). Serve para qualquer tipo.
    Never,
    /// Ja deu erro: nao repete o erro em cascata.
    Err,
}

struct TyInfo {
    ty: Ty,
    jp: &'static str,
    romaji: &'static str,
    meaning: &'static str,
}

const TYPES: &[TyInfo] = &[
    TyInfo { ty: Ty::I32, jp: "整32", romaji: "sei32", meaning: "32-bit signed integer" },
    TyInfo { ty: Ty::U32, jp: "非負32", romaji: "hifu32", meaning: "32-bit unsigned integer" },
    TyInfo { ty: Ty::F64, jp: "実64", romaji: "jitsu64", meaning: "64-bit floating point" },
    TyInfo { ty: Ty::Bool, jp: "真偽", romaji: "shingi", meaning: "boolean" },
    TyInfo { ty: Ty::Char, jp: "文字", romaji: "moji", meaning: "character" },
    TyInfo { ty: Ty::Str, jp: "文字列", romaji: "mojiretsu", meaning: "text (UTF-8, immutable)" },
];

impl Ty {
    fn c(self) -> String {
        match self {
            Ty::I32 => "int32_t".to_string(),
            Ty::U32 | Ty::Char => "uint32_t".to_string(),
            Ty::F64 => "double".to_string(),
            Ty::Bool => "int".to_string(),
            Ty::Str => "nh_str".to_string(),
            Ty::Struct(id) => format!("nhT{}", id),
            Ty::Ref(id, _) => format!("nhT{}*", id),
            Ty::Unit | Ty::Never | Ty::Err => "void".to_string(),
        }
    }

    /// Tipos simples: os que valem em contas, comparacoes e constantes.
    fn is_simple(self) -> bool {
        matches!(self, Ty::I32 | Ty::U32 | Ty::F64 | Ty::Bool | Ty::Char | Ty::Str)
    }

    fn jp(self) -> &'static str {
        match self {
            Ty::Unit => "()",
            Ty::Never => "!",
            Ty::Err | Ty::Struct(_) | Ty::Ref(..) => "?",
            t => TYPES.iter().find(|i| i.ty == t).unwrap().jp,
        }
    }

    /// `整32` (sei32, 32-bit signed integer), para as notas.
    fn long(self) -> String {
        match self {
            Ty::Unit => "`()` means \"no value\"".to_string(),
            Ty::Never | Ty::Err | Ty::Struct(_) | Ty::Ref(..) => String::new(),
            t => {
                let i = TYPES.iter().find(|i| i.ty == t).unwrap();
                format!("`{}` ({}) is a {}", i.jp, i.romaji, i.meaning)
            }
        }
    }

    fn is_int(self) -> bool {
        matches!(self, Ty::I32 | Ty::U32)
    }

    fn is_num(self) -> bool {
        matches!(self, Ty::I32 | Ty::U32 | Ty::F64)
    }

    fn has_value(self) -> bool {
        !matches!(self, Ty::Unit | Ty::Never | Ty::Err)
    }
}

fn compat(expected: Ty, found: Ty) -> bool {
    expected == found
        || found == Ty::Never
        || found == Ty::Err
        || expected == Ty::Err
        // `&可変 T` serve onde se espera `&T`
        || matches!((expected, found), (Ty::Ref(a, false), Ty::Ref(b, _)) if a == b)
}

// ----------------------------------------------------------------- estruturas

struct FieldDef {
    name: String,
    ty: Ty,
    span: Span,
    cname: String,
}

struct StructDef {
    name: String,
    span: Span,
    fields: Vec<FieldDef>,
}

/// Onde um valor mora, para saber se pode receber atribuicao ou emprestimo.
#[derive(Clone, Copy)]
struct PlaceInfo {
    is_place: bool,
    /// A variavel local no comeco do caminho (`p` em `p.a.b`).
    root: Option<usize>,
    /// `Some(mutavel)` quando o caminho passou por uma referencia.
    via_ref: Option<bool>,
}

struct FnSig {
    self_kind: Option<SelfKind>,
    params: Vec<(Ty, Span)>,
    ret: Ty,
    ret_span: Option<Span>,
    name_span: Span,
    cname: String,
}

struct ConstSig {
    ty: Ty,
    cname: String,
}

struct Local {
    name: String,
    cname: String,
    ty: Ty,
    mutable: bool,
    used: bool,
    assigned: bool,
    span: Span,
    mut_span: Option<Span>,
}

#[derive(Clone)]
struct Val {
    /// Expressao C pura (sem efeitos); vazia quando nao ha valor.
    c: String,
    ty: Ty,
    span: Span,
}

enum Piece {
    Lit(String),
    Arg,
}

#[derive(Clone, Copy, PartialEq)]
enum MacroKind {
    Println,
    Print,
    Panic,
    Assert,
    Format,
    Todo,
}

/// Valor de uma constante, calculado na compilacao.
#[derive(Clone, Debug)]
enum CV {
    S(String),
    I(i32),
    U(u32),
    F(f64),
    B(bool),
    C(char),
}

impl CV {
    fn ty(&self) -> Ty {
        match self {
            CV::S(_) => Ty::Str,
            CV::I(_) => Ty::I32,
            CV::U(_) => Ty::U32,
            CV::F(_) => Ty::F64,
            CV::B(_) => Ty::Bool,
            CV::C(_) => Ty::Char,
        }
    }

    fn to_c(&self) -> String {
        match self {
            CV::S(t) => format!("((nh_str){{\"{}\", {}}})", c_escape(t), t.len()),
            CV::I(v) if *v == i32::MIN => "(-2147483647-1)".to_string(),
            CV::I(v) if *v < 0 => format!("({})", v),
            CV::I(v) => v.to_string(),
            CV::U(v) => format!("{}u", v),
            CV::F(v) if v.is_nan() => "NAN".to_string(),
            CV::F(v) if *v == f64::INFINITY => "INFINITY".to_string(),
            CV::F(v) if *v == f64::NEG_INFINITY => "(-INFINITY)".to_string(),
            CV::F(v) => format!("({:?})", v),
            CV::B(v) => if *v { "1" } else { "0" }.to_string(),
            CV::C(c) => format!("((uint32_t){})", *c as u32),
        }
    }
}

struct Cg<'a, 'h> {
    file: &'a SourceFile,
    handler: &'h mut Handler,
    fns: HashMap<String, FnSig>,
    consts: HashMap<String, ConstSig>,
    names: HashMap<String, Span>,
    protos: String,
    defs: String,
    buf: String,
    locals: Vec<Local>,
    scopes: Vec<Vec<usize>>,
    counter: usize,
    loop_stack: Vec<bool>,
    cur_ret: Ty,
    cur_const: Option<String>,
    const_deps: HashMap<String, Vec<String>>,
    entry: Option<String>,
    /// Tipo escolhido para cada literal inteiro (chave: inicio do span).
    lit_tys: HashMap<usize, Ty>,
    structs: Vec<StructDef>,
    struct_ids: HashMap<String, u16>,
    methods: HashMap<(u16, String), FnSig>,
    /// Definicoes `typedef struct` em C.
    types: String,
    /// A struct do bloco `実装` em que estamos (`自身`).
    cur_self: Option<u16>,
}

pub fn generate(file: &SourceFile, prog: &Program, handler: &mut Handler) -> Option<String> {
    let before = handler.error_count();
    let diags_before = handler.diags.len();
    let mut cg = Cg {
        file,
        handler,
        fns: HashMap::new(),
        consts: HashMap::new(),
        names: HashMap::new(),
        protos: String::new(),
        defs: String::new(),
        buf: String::new(),
        locals: Vec::new(),
        scopes: Vec::new(),
        counter: 0,
        loop_stack: Vec::new(),
        cur_ret: Ty::Unit,
        cur_const: None,
        const_deps: HashMap::new(),
        entry: None,
        lit_tys: HashMap::new(),
        structs: Vec::new(),
        struct_ids: HashMap::new(),
        methods: HashMap::new(),
        types: String::new(),
        cur_self: None,
    };
    cg.run(prog);
    if cg.handler.error_count() > before {
        // com erros no programa, avisos de "variavel sem uso" so atrapalham:
        // o programa ainda esta mudando
        let mut i = 0;
        cg.handler.diags.retain(|d| {
            i += 1;
            i <= diags_before || !matches!(d.code, "W0002" | "W0003" | "W0004")
        });
        None
    } else {
        Some(cg.finish())
    }
}

// ------------------------------------------------------------------ utilitarios

fn mangle(name: &str) -> String {
    let mut s = String::from("nh_");
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c);
        } else {
            s.push_str(&format!("_{:x}_", c as u32));
        }
    }
    s
}

fn c_escape(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b' ' {
            out.push(b as char);
        } else {
            out.push_str(&format!("\\{:03o}", b));
        }
    }
    out
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i];
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur.push((prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// O nome parecido mais proximo (para "voce quis dizer...").
fn similar<'s>(name: &str, candidates: impl Iterator<Item = &'s str>) -> Option<String> {
    let max = (name.chars().count() / 3).max(1);
    let mut best: Option<(usize, &str)> = None;
    for c in candidates {
        if c == name {
            continue;
        }
        let d = if c.to_lowercase() == name.to_lowercase() {
            0
        } else {
            edit_distance(name, c)
        };
        if d <= max && best.map_or(true, |(bd, _)| d < bd) {
            best = Some((d, c));
        }
    }
    best.map(|(_, c)| c.to_string())
}

fn is_lit(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Int(_) | ExprKind::Float(_) => true,
        ExprKind::Paren(x) => is_lit(x),
        ExprKind::Unary { op: UnOp::Neg, expr } => is_lit(expr),
        _ => false,
    }
}

fn parse_format(s: &str) -> Result<Vec<Piece>, String> {
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '{' => {
                if it.peek() == Some(&'{') {
                    it.next();
                    lit.push('{');
                } else if it.peek() == Some(&'}') {
                    it.next();
                    if !lit.is_empty() {
                        out.push(Piece::Lit(std::mem::take(&mut lit)));
                    }
                    out.push(Piece::Arg);
                } else {
                    return Err(if it.peek().is_none() {
                        "expected `}` but the string was terminated".to_string()
                    } else {
                        "only `{}` is supported in format strings for now".to_string()
                    });
                }
            }
            '}' => {
                if it.peek() == Some(&'}') {
                    it.next();
                    lit.push('}');
                } else {
                    return Err("unmatched `}` found; use `}}` to write a literal `}`".to_string());
                }
            }
            c => lit.push(c),
        }
    }
    if !lit.is_empty() {
        out.push(Piece::Lit(lit));
    }
    Ok(out)
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{} {}", n, if n == 1 { one } else { many })
}

// ------------------------------------------------------------------- gerador

impl<'a, 'h> Cg<'a, 'h> {
    fn emit(&mut self, d: Diagnostic) {
        self.handler.emit(d);
    }

    fn line(&mut self, s: impl AsRef<str>) {
        self.buf.push_str(s.as_ref());
        self.buf.push('\n');
    }

    fn tmp(&mut self) -> String {
        self.counter += 1;
        format!("t{}", self.counter)
    }

    fn capture<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> (String, R) {
        let saved = std::mem::take(&mut self.buf);
        let r = f(self);
        let inner = std::mem::replace(&mut self.buf, saved);
        (inner, r)
    }

    fn loc(&self, span: Span) -> String {
        let (l, c) = self.file.line_col(span.start);
        c_escape(&format!("{}:{}:{}", self.file.name, l, c))
    }

    fn text(&self, span: Span) -> &'a str {
        &self.file.text[span.range()]
    }

    fn unit(&self, span: Span) -> Val {
        Val {
            c: String::new(),
            ty: Ty::Unit,
            span,
        }
    }

    fn err_val(&self, span: Span) -> Val {
        Val {
            c: "0".to_string(),
            ty: Ty::Err,
            span,
        }
    }

    // ------------------------------------------------------------------ tipos

    fn tjp(&self, t: Ty) -> String {
        match t {
            Ty::Struct(id) => self.structs[id as usize].name.clone(),
            Ty::Ref(id, m) => format!(
                "&{}{}",
                if m { "可変 " } else { "" },
                self.structs[id as usize].name
            ),
            t => t.jp().to_string(),
        }
    }

    fn bt(&self, t: Ty) -> String {
        format!("`{}`", self.tjp(t))
    }

    fn tlong(&self, t: Ty) -> String {
        match t {
            Ty::Struct(id) => format!("`{}` is a struct defined in this program", self.structs[id as usize].name),
            Ty::Ref(id, _) => format!(
                "`{}` is a reference to a `{}`: it points to the original value",
                self.tjp(t),
                self.structs[id as usize].name
            ),
            t => t.long(),
        }
    }

    fn resolve_type(&mut self, t: &Type) -> Ty {
        let base = self.resolve_base(t);
        match (t.reference, base) {
            (None, b) => b,
            (Some(m), Ty::Struct(id)) => Ty::Ref(id, m),
            (Some(_), Ty::Err) => Ty::Err,
            (Some(_), other) => {
                let d = Diagnostic::error(
                    E0622,
                    format!("cannot take a reference to {} for now", self.bt(other)),
                )
                .primary(t.span, "references only point to structs for now")
                .note("references to numbers, text and the other basic types arrive in a later stage");
                self.emit(d);
                Ty::Err
            }
        }
    }

    /// Procura uma struct pelo nome (`自身` e a struct do bloco `実装`).
    fn find_struct(&mut self, id: &Ident) -> Option<u16> {
        if id.name == "自身" {
            if let Some(s) = self.cur_self {
                return Some(s);
            }
            let d = Diagnostic::error(E0201, "cannot find type `自身` in this scope")
                .primary(id.span, "not inside an `実装` block")
                .note("`自身` (jishin, \"Self\") is the type of the current `実装` (jissou, \"implementation\") block, so it only exists inside one");
            self.emit(d);
            return None;
        }
        if let Some(&s) = self.struct_ids.get(&id.name) {
            return Some(s);
        }
        let mut d = Diagnostic::error(
            E0201,
            format!("cannot find struct `{}` in this scope", id.name),
        )
        .primary(id.span, "not found in this scope");
        let names: Vec<String> = self.structs.iter().map(|s| s.name.clone()).collect();
        if let Some(sg) = similar(&id.name, names.iter().map(|s| s.as_str())) {
            d = d.suggest("a struct with a similar name exists", id.span, sg, false);
        }
        self.emit(d);
        None
    }

    fn resolve_base(&mut self, t: &Type) -> Ty {
        if t.name == "自身" {
            let id = Ident { name: t.name.clone(), span: t.span };
            return match self.find_struct(&id) {
                Some(s) => Ty::Struct(s),
                None => Ty::Err,
            };
        }
        if let Some(&id) = self.struct_ids.get(&t.name) {
            if !TYPES.iter().any(|i| i.jp == t.name || i.romaji == t.name) {
                return Ty::Struct(id);
            }
        }
        for i in TYPES {
            if t.name == i.jp {
                return i.ty;
            }
            if t.name == i.romaji {
                if !self.handler.config.romaji {
                    let d = Diagnostic::warning(
                        W0001,
                        format!("type name `{}` is written in latin letters", t.name),
                    )
                    .primary(
                        t.span,
                        format!("latin spelling of `{}` (\"{}\")", i.jp, i.meaning),
                    )
                    .note("pass `--romaji` to allow latin keywords")
                    .suggest("use the canonical form", t.span, i.jp, true);
                    self.emit(d);
                }
                return i.ty;
            }
        }
        let mut d = Diagnostic::error(
            E0201,
            format!("cannot find type `{}` in this scope", t.name),
        )
        .primary(t.span, "not found in this scope");
        let mut cands: Vec<String> = TYPES
            .iter()
            .flat_map(|i| [i.jp.to_string(), i.romaji.to_string()])
            .collect();
        cands.extend(self.structs.iter().map(|s| s.name.clone()));
        if let Some(s) = similar(&t.name, cands.iter().map(|s| s.as_str())) {
            let s = TYPES
                .iter()
                .find(|i| i.romaji == s)
                .map(|i| i.jp.to_string())
                .unwrap_or(s);
            d = d.suggest("a type with a similar name exists", t.span, s, false);
        }
        let mut list: Vec<String> = TYPES
            .iter()
            .map(|i| format!("`{}` ({})", i.jp, i.romaji))
            .collect();
        list.extend(self.structs.iter().map(|s| format!("`{}`", s.name)));
        d = d.note(format!("available types: {}", list.join(", ")));
        self.emit(d);
        Ty::Err
    }

    fn mismatch(&mut self, e: &Val, expected: Ty, lit: Option<&Expr>, extra: Option<(Span, String)>) {
        let mut d = Diagnostic::error(E0301, "mismatched types");
        if let Some((s, m)) = extra {
            d = d.secondary(s, m);
        }
        d = d.primary(
            e.span,
            format!("expected {}, found {}", self.bt(expected), self.bt(e.ty)),
        );
        for t in [expected, e.ty] {
            let n = self.tlong(t);
            if !n.is_empty() {
                d = d.note(n);
            }
        }
        let int_lit_as_float = lit.map_or(false, |x| expected == Ty::F64 && matches!(x.kind, ExprKind::Int(_)));
        if int_lit_as_float {
            d = d.suggest(
                "a float literal needs a decimal point",
                Span::new(e.span.end, e.span.end),
                ".0",
                true,
            );
        } else if expected.is_num()
            && e.ty.is_num()
            && self.file.line_of(e.span.start) == self.file.line_of(e.span.end.max(1) - 1)
        {
            d = d.suggest(
                "convert it with `として` (toshite, \"as\")",
                Span::new(e.span.end, e.span.end),
                format!(" として {}", self.tjp(expected)),
                false,
            );
        }
        if let (Ty::Ref(a, m), Ty::Struct(b)) = (expected, e.ty) {
            if a == b {
                d = d.suggest(
                    "borrow it instead of passing it by value",
                    Span::new(e.span.start, e.span.start),
                    if m { "&可変 " } else { "&" },
                    false,
                );
            }
        }
        self.emit(d);
    }

    fn check(&mut self, e: &Val, expected: Ty, lit: Option<&Expr>) {
        if !compat(expected, e.ty) {
            self.mismatch(e, expected, lit, None);
        }
    }

    // --------------------------------------------------------------- escopos

    fn push_scope(&mut self) {
        self.scopes.push(Vec::new());
    }

    fn pop_scope(&mut self) {
        let Some(scope) = self.scopes.pop() else { return };
        for idx in scope {
            let (name, span, used, assigned, mutable, mut_span) = {
                let l = &self.locals[idx];
                (l.name.clone(), l.span, l.used, l.assigned, l.mutable, l.mut_span)
            };
            if !used && !name.starts_with('_') {
                let msg = if assigned {
                    format!("variable `{}` is assigned to, but never used", name)
                } else {
                    format!("unused variable: `{}`", name)
                };
                let d = Diagnostic::warning(W0002, msg)
                    .primary(span, "never read")
                    .suggest(
                        "if this is intentional, prefix it with an underscore",
                        Span::new(span.start, span.start),
                        "_",
                        true,
                    );
                self.emit(d);
            }
            if mutable && !assigned {
                if let Some(ms) = mut_span {
                    let d = Diagnostic::warning(W0004, "variable does not need to be mutable")
                        .primary(ms, "never reassigned")
                        .suggest("remove this `可変` (kahen, \"mutable\")", ms, "", true);
                    self.emit(d);
                }
            }
        }
    }

    fn declare(&mut self, name: &Ident, ty: Ty, mutable: bool, mut_span: Option<Span>) -> String {
        self.counter += 1;
        let cname = format!("v{}_{}", self.counter, &mangle(&name.name)[3..]);
        self.locals.push(Local {
            name: name.name.clone(),
            cname: cname.clone(),
            ty,
            mutable,
            used: false,
            assigned: false,
            span: name.span,
            mut_span,
        });
        let idx = self.locals.len() - 1;
        self.scopes.last_mut().unwrap().push(idx);
        cname
    }

    fn lookup_local(&self, name: &str) -> Option<usize> {
        for scope in self.scopes.iter().rev() {
            for &i in scope.iter().rev() {
                if self.locals[i].name == name {
                    return Some(i);
                }
            }
        }
        None
    }

    fn visible_names(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for scope in &self.scopes {
            for &i in scope {
                v.push(self.locals[i].name.clone());
            }
        }
        v.extend(self.consts.keys().cloned());
        v
    }

    // ------------------------------------------------------------------ itens

    /// Tipos dos parametros e do retorno de uma funcao ou metodo.
    fn sig_parts(&mut self, f: &FnItem, owner: Option<u16>) -> (Vec<(Ty, Span)>, Ty) {
        let mut params = Vec::new();
        if let (Some(sp), Some(id)) = (&f.self_param, owner) {
            let ty = match sp.kind {
                SelfKind::Value => Ty::Struct(id),
                SelfKind::Ref => Ty::Ref(id, false),
                SelfKind::RefMut => Ty::Ref(id, true),
            };
            params.push((ty, sp.span));
        } else if let Some(sp) = &f.self_param {
            let d = Diagnostic::error(E0201, "`自分` can only be used in methods")
                .primary(sp.span, "not inside an `実装` block");
            self.emit(d);
        }
        let mut seen: HashMap<&str, Span> = HashMap::new();
        for p in &f.params {
            let ty = self.resolve_type(&p.ty);
            if let Some(first) = seen.get(p.name.name.as_str()) {
                let d = Diagnostic::error(
                    E0203,
                    format!("parameter `{}` is declared more than once", p.name.name),
                )
                .secondary(*first, "first declared here")
                .primary(p.name.span, "declared again here");
                self.emit(d);
            } else {
                seen.insert(&p.name.name, p.name.span);
            }
            params.push((ty, p.ty.span));
        }
        let mut ret = match &f.ret {
            Some(t) => self.resolve_type(t),
            None => Ty::Unit,
        };
        if let (Ty::Ref(..), Some(t)) = (ret, &f.ret) {
            let d = Diagnostic::error(E0622, "functions cannot return references yet")
                .primary(t.span, "this return type is a reference")
                .note("without a borrow checker, a returned reference could point to a value that no longer exists")
                .help("return the struct by value instead");
            self.emit(d);
            ret = Ty::Err;
        }
        (params, ret)
    }

    fn run(&mut self, prog: &Program) {
        // 0. structs: nomes, campos e as definicoes em C
        let mut struct_items: Vec<(&StructItem, u16)> = Vec::new();
        for item in &prog.items {
            let Item::Struct(st) = item else { continue };
            if let Some(first) = self.names.get(&st.name.name) {
                let d = Diagnostic::error(
                    E0203,
                    format!("the name `{}` is defined multiple times", st.name.name),
                )
                .secondary(*first, "first definition")
                .primary(st.name.span, "defined again here");
                self.emit(d);
                continue;
            }
            self.names.insert(st.name.name.clone(), st.name.span);
            let id = self.structs.len() as u16;
            self.structs.push(StructDef {
                name: st.name.name.clone(),
                span: st.name.span,
                fields: Vec::new(),
            });
            self.struct_ids.insert(st.name.name.clone(), id);
            struct_items.push((st, id));
        }
        for (st, id) in &struct_items {
            let mut fields: Vec<FieldDef> = Vec::new();
            for f in &st.fields {
                let ty = self.resolve_type(&f.ty);
                if matches!(ty, Ty::Ref(..)) {
                    let d = Diagnostic::error(E0622, "struct fields cannot be references yet")
                        .primary(f.ty.span, "this field type is a reference")
                        .note("without a borrow checker, a stored reference could outlive the value it points to")
                        .help("store the struct by value instead");
                    self.emit(d);
                    continue;
                }
                if let Some(prev) = fields.iter().find(|x| x.name == f.name.name) {
                    let d = Diagnostic::error(
                        E0203,
                        format!("field `{}` is declared more than once", f.name.name),
                    )
                    .secondary(prev.span, "first declared here")
                    .primary(f.name.span, "declared again here");
                    self.emit(d);
                    continue;
                }
                fields.push(FieldDef {
                    name: f.name.name.clone(),
                    ty,
                    span: f.name.span,
                    cname: format!("m_{}", &mangle(&f.name.name)[3..]),
                });
            }
            self.structs[*id as usize].fields = fields;
        }
        self.emit_struct_types();

        // 1. nomes e assinaturas
        for item in &prog.items {
            match item {
                Item::Struct(_) => {}
                Item::Impl(imp) => {
                    let Some(id) = self.find_struct(&imp.ty) else { continue };
                    let saved = self.cur_self.replace(id);
                    for f in &imp.fns {
                        let (params, ret) = self.sig_parts(f, Some(id));
                        let key = (id, f.name.name.clone());
                        if let Some(first) = self.methods.get(&key) {
                            let d = Diagnostic::error(
                                E0203,
                                format!(
                                    "the name `{}` is defined multiple times for `{}`",
                                    f.name.name, imp.ty.name
                                ),
                            )
                            .secondary(first.name_span, "first definition")
                            .primary(f.name.span, "defined again here");
                            self.emit(d);
                            continue;
                        }
                        self.methods.insert(
                            key,
                            FnSig {
                                self_kind: f.self_param.map(|sp| sp.kind),
                                params,
                                ret,
                                ret_span: f.ret.as_ref().map(|t| t.span),
                                name_span: f.name.span,
                                cname: format!("nhM{}_{}", id, &mangle(&f.name.name)[3..]),
                            },
                        );
                    }
                    self.cur_self = saved;
                }
                Item::Fn(f) => {
                    let (params, ret) = self.sig_parts(f, None);
                    if let Some(first) = self.names.get(&f.name.name) {
                        let d = Diagnostic::error(
                            E0203,
                            format!("the name `{}` is defined multiple times", f.name.name),
                        )
                        .secondary(*first, "first definition")
                        .primary(f.name.span, "defined again here");
                        self.emit(d);
                        continue;
                    }
                    self.names.insert(f.name.name.clone(), f.name.span);
                    self.fns.insert(
                        f.name.name.clone(),
                        FnSig {
                            self_kind: None,
                            params,
                            ret,
                            ret_span: f.ret.as_ref().map(|t| t.span),
                            name_span: f.name.span,
                            cname: mangle(&f.name.name),
                        },
                    );
                }
                Item::Const(c) => {
                    let mut ty = self.resolve_type(&c.ty);
                    if !ty.is_simple() && ty != Ty::Err {
                        let d = Diagnostic::error(E0999, "constants of this type are not supported yet")
                            .primary(c.ty.span, format!("{} is not a basic type", self.bt(ty)))
                            .note("constants can be numbers, `真偽`, `文字` and `文字列` for now");
                        self.emit(d);
                        ty = Ty::Err;
                    }
                    if let Some(first) = self.names.get(&c.name.name) {
                        let d = Diagnostic::error(
                            E0203,
                            format!("the name `{}` is defined multiple times", c.name.name),
                        )
                        .secondary(*first, "first definition")
                        .primary(c.name.span, "defined again here");
                        self.emit(d);
                        continue;
                    }
                    self.names.insert(c.name.name.clone(), c.name.span);
                    self.consts.insert(
                        c.name.name.clone(),
                        ConstSig {
                            ty,
                            cname: mangle(&c.name.name),
                        },
                    );
                }
            }
        }

        // 2. o ponto de entrada
        self.check_entry(prog);

        // 3. corpos, na ordem do arquivo
        for item in &prog.items {
            match item {
                Item::Fn(f) => {
                    if self.fns.get(&f.name.name).map(|s| s.name_span) == Some(f.name.span) {
                        self.gen_fn(f, None);
                    }
                }
                Item::Struct(_) => {}
                Item::Impl(imp) => {
                    let Some(&id) = self.struct_ids.get(&imp.ty.name) else {
                        continue;
                    };
                    for f in &imp.fns {
                        let key = (id, f.name.name.clone());
                        if self.methods.get(&key).map(|s| s.name_span) == Some(f.name.span) {
                            self.gen_fn(f, Some(id));
                        }
                    }
                }
                Item::Const(c) => {
                    if self.names.get(&c.name.name) == Some(&c.name.span) {
                        self.gen_const(c);
                    }
                }
            }
        }

        self.check_const_cycles(prog);

        // 4. valores das constantes, calculados agora (erro de conta = erro de compilacao)
        if self.handler.error_count() == 0 {
            self.emit_consts(prog);
        }
    }

    /// Escreve os `typedef struct` em C, cada struct depois das que ela contem.
    fn emit_struct_types(&mut self) {
        fn visit(id: usize, structs: &[StructDef], state: &mut [u8], order: &mut Vec<usize>) -> bool {
            match state[id] {
                1 => return true,
                2 => return false,
                _ => {}
            }
            state[id] = 1;
            for f in &structs[id].fields {
                if let Ty::Struct(c) = f.ty {
                    if visit(c as usize, structs, state, order) {
                        return true;
                    }
                }
            }
            state[id] = 2;
            order.push(id);
            false
        }
        let n = self.structs.len();
        let mut state = vec![0u8; n];
        let mut order = Vec::new();
        let mut cyclic = false;
        for id in 0..n {
            if visit(id, &self.structs, &mut state, &mut order) {
                let st = &self.structs[id];
                let d = Diagnostic::error(
                    E0620,
                    format!("struct `{}` contains itself, so it would have infinite size", st.name),
                )
                .primary(st.span, "recursive struct")
                .note("a struct cannot hold a copy of itself, directly or through other structs")
                .help("references cannot be stored in fields yet, so a recursive struct is not possible for now");
                self.emit(d);
                // um ciclo so e relatado uma vez
                for st in state.iter_mut() {
                    if *st == 1 {
                        *st = 2;
                    }
                }
                cyclic = true;
            }
        }
        if cyclic {
            return;
        }
        for id in order {
            let mut text = format!("typedef struct nhT{} {{\n", id);
            if self.structs[id].fields.is_empty() {
                text.push_str("char nh_unused;\n");
            }
            for f in &self.structs[id].fields {
                text.push_str(&format!("{} {};\n", f.ty.c(), f.cname));
            }
            text.push_str(&format!("}} nhT{};\n\n", id));
            self.types.push_str(&text);
        }
    }

    fn emit_consts(&mut self, prog: &Program) {
        let mut asts: HashMap<&str, &ConstItem> = HashMap::new();
        for item in &prog.items {
            if let Item::Const(c) = item {
                if self.names.get(&c.name.name) == Some(&c.name.span) {
                    asts.insert(&c.name.name, c);
                }
            }
        }
        let mut memo: HashMap<String, CV> = HashMap::new();
        for item in &prog.items {
            let Item::Const(c) = item else { continue };
            if self.names.get(&c.name.name) != Some(&c.name.span) {
                continue;
            }
            let Some(v) = self.eval_const(&c.name.name, &asts, &mut memo) else {
                continue;
            };
            let (ty, cname) = {
                let s = &self.consts[&c.name.name];
                (s.ty, s.cname.clone())
            };
            if !ty.has_value() {
                continue;
            }
            self.protos
                .push_str(&format!("static {} {}(void);\n", ty.c(), cname));
            self.defs.push_str(&format!(
                "static {} {}(void) {{\nreturn {};\n}}\n\n",
                ty.c(),
                cname,
                v.to_c()
            ));
        }
    }

    fn eval_const(
        &mut self,
        name: &str,
        asts: &HashMap<&str, &ConstItem>,
        memo: &mut HashMap<String, CV>,
    ) -> Option<CV> {
        if let Some(v) = memo.get(name) {
            return Some(v.clone());
        }
        let c = *asts.get(name)?;
        let v = self.eval(&c.value, asts, memo)?;
        memo.insert(name.to_string(), v.clone());
        Some(v)
    }

    fn eval_fail(&mut self, span: Span, msg: &str) -> Option<CV> {
        let d = Diagnostic::error(E0208, "evaluation of constant value failed")
            .primary(span, msg.to_string())
            .note("constants are calculated while compiling, so a bad calculation is an error here, not a panic when the program runs");
        self.emit(d);
        None
    }

    fn eval(
        &mut self,
        e: &Expr,
        asts: &HashMap<&str, &ConstItem>,
        memo: &mut HashMap<String, CV>,
    ) -> Option<CV> {
        match &e.kind {
            ExprKind::Int(n) => {
                let ty = self.lit_tys.get(&e.span.start).copied().unwrap_or(Ty::I32);
                Some(if ty == Ty::U32 { CV::U(*n as u32) } else { CV::I(*n as i32) })
            }
            ExprKind::Float(s) => {
                let t: String = s.chars().filter(|c| *c != '_').collect();
                Some(CV::F(t.parse::<f64>().ok()?))
            }
            ExprKind::Bool(b) => Some(CV::B(*b)),
            ExprKind::Char(c) => Some(CV::C(*c)),
            ExprKind::Str(t) => Some(CV::S(t.clone())),
            ExprKind::Paren(x) => self.eval(x, asts, memo),
            ExprKind::Path(id) => self.eval_const(&id.name, asts, memo),
            ExprKind::Unary { op, expr } => {
                if let (UnOp::Neg, ExprKind::Int(n)) = (op, &expr.kind) {
                    let ty = self.lit_tys.get(&e.span.start).copied().unwrap_or(Ty::I32);
                    let _ = ty;
                    return Some(CV::I((-(*n as i64)) as i32));
                }
                let v = self.eval(expr, asts, memo)?;
                match (op, v) {
                    (UnOp::Neg, CV::I(a)) => match a.checked_neg() {
                        Some(r) => Some(CV::I(r)),
                        None => self.eval_fail(e.span, "attempt to negate with overflow"),
                    },
                    (UnOp::Neg, CV::F(a)) => Some(CV::F(-a)),
                    (UnOp::Not, CV::B(a)) => Some(CV::B(!a)),
                    _ => None,
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.eval(lhs, asts, memo)?;
                if *op == BinOp::And && matches!(l, CV::B(false)) {
                    return Some(CV::B(false));
                }
                if *op == BinOp::Or && matches!(l, CV::B(true)) {
                    return Some(CV::B(true));
                }
                let r = self.eval(rhs, asts, memo)?;
                self.eval_binary(*op, l, r, e.span)
            }
            ExprKind::Cast { expr, ty } => {
                let v = self.eval(expr, asts, memo)?;
                let target = TYPES.iter().find(|i| i.jp == ty.name || i.romaji == ty.name)?.ty;
                let as_i64 = |v: &CV| -> Option<i64> {
                    Some(match v {
                        CV::I(a) => *a as i64,
                        CV::U(a) => *a as i64,
                        CV::B(a) => *a as i64,
                        CV::C(a) => *a as i64,
                        CV::F(_) | CV::S(_) => return None,
                    })
                };
                Some(match (v, target) {
                    (CV::F(a), Ty::I32) => CV::I(a as i32),
                    (CV::F(a), Ty::U32) => CV::U(a as u32),
                    (CV::F(a), Ty::F64) => CV::F(a),
                    (v, Ty::F64) => CV::F(as_i64(&v)? as f64),
                    (v, Ty::I32) => CV::I(as_i64(&v)? as i32),
                    (v, Ty::U32) => CV::U(as_i64(&v)? as u32),
                    (v, t) if v.ty() == t => v,
                    _ => return None,
                })
            }
            ExprKind::If { cond, then, else_ } => {
                let c = self.eval(cond, asts, memo)?;
                let CV::B(c) = c else { return None };
                if c {
                    self.eval_block(then, asts, memo)
                } else {
                    match else_ {
                        Some(el) => self.eval(el, asts, memo),
                        None => None,
                    }
                }
            }
            ExprKind::Block(b) => self.eval_block(b, asts, memo),
            _ => {
                let d = Diagnostic::error(E0999, "this expression is not supported in constants yet")
                    .primary(e.span, "not supported here")
                    .note("constants accept numbers, `真偽`, `文字`, other constants, operators, `として` and `もし` / `他` (moshi / hoka)");
                self.emit(d);
                None
            }
        }
    }

    fn eval_block(
        &mut self,
        b: &Block,
        asts: &HashMap<&str, &ConstItem>,
        memo: &mut HashMap<String, CV>,
    ) -> Option<CV> {
        match (&b.stmts.is_empty(), &b.tail) {
            (true, Some(t)) => self.eval(t, asts, memo),
            _ => {
                let d = Diagnostic::error(E0999, "blocks with statements are not supported in constants yet")
                    .primary(b.span, "only a single final expression is allowed here");
                self.emit(d);
                None
            }
        }
    }

    fn eval_binary(&mut self, op: BinOp, l: CV, r: CV, span: Span) -> Option<CV> {
        use BinOp::*;
        let overflow = |this: &mut Self, what: &str| this.eval_fail(span, &format!("attempt to {} with overflow", what));
        match (l, r) {
            (CV::I(a), CV::I(b)) => match op {
                Add => a.checked_add(b).map(CV::I).or_else(|| overflow(self, "add").map(|_| CV::I(0))),
                Sub => a.checked_sub(b).map(CV::I).or_else(|| overflow(self, "subtract").map(|_| CV::I(0))),
                Mul => a.checked_mul(b).map(CV::I).or_else(|| overflow(self, "multiply").map(|_| CV::I(0))),
                Div | Rem => {
                    if b == 0 {
                        return self.eval_fail(span, if op == Div { "attempt to divide by zero" } else { "attempt to calculate the remainder with a divisor of zero" });
                    }
                    let r = if op == Div { a.checked_div(b) } else { a.checked_rem(b) };
                    match r {
                        Some(v) => Some(CV::I(v)),
                        None => overflow(self, "divide"),
                    }
                }
                Eq => Some(CV::B(a == b)),
                Ne => Some(CV::B(a != b)),
                Lt => Some(CV::B(a < b)),
                Gt => Some(CV::B(a > b)),
                Le => Some(CV::B(a <= b)),
                Ge => Some(CV::B(a >= b)),
                _ => None,
            },
            (CV::U(a), CV::U(b)) => match op {
                Add => a.checked_add(b).map(CV::U).or_else(|| overflow(self, "add").map(|_| CV::U(0))),
                Sub => a.checked_sub(b).map(CV::U).or_else(|| overflow(self, "subtract").map(|_| CV::U(0))),
                Mul => a.checked_mul(b).map(CV::U).or_else(|| overflow(self, "multiply").map(|_| CV::U(0))),
                Div | Rem => {
                    if b == 0 {
                        return self.eval_fail(span, if op == Div { "attempt to divide by zero" } else { "attempt to calculate the remainder with a divisor of zero" });
                    }
                    Some(CV::U(if op == Div { a / b } else { a % b }))
                }
                Eq => Some(CV::B(a == b)),
                Ne => Some(CV::B(a != b)),
                Lt => Some(CV::B(a < b)),
                Gt => Some(CV::B(a > b)),
                Le => Some(CV::B(a <= b)),
                Ge => Some(CV::B(a >= b)),
                _ => None,
            },
            (CV::F(a), CV::F(b)) => match op {
                Add => Some(CV::F(a + b)),
                Sub => Some(CV::F(a - b)),
                Mul => Some(CV::F(a * b)),
                Div => Some(CV::F(a / b)),
                Rem => Some(CV::F(a % b)),
                Eq => Some(CV::B(a == b)),
                Ne => Some(CV::B(a != b)),
                Lt => Some(CV::B(a < b)),
                Gt => Some(CV::B(a > b)),
                Le => Some(CV::B(a <= b)),
                Ge => Some(CV::B(a >= b)),
                _ => None,
            },
            (CV::B(a), CV::B(b)) => match op {
                And => Some(CV::B(a && b)),
                Or => Some(CV::B(a || b)),
                Eq => Some(CV::B(a == b)),
                Ne => Some(CV::B(a != b)),
                _ => None,
            },
            (CV::S(a), CV::S(b)) => match op {
                Add => Some(CV::S(format!("{}{}", a, b))),
                Eq => Some(CV::B(a == b)),
                Ne => Some(CV::B(a != b)),
                Lt => Some(CV::B(a < b)),
                Gt => Some(CV::B(a > b)),
                Le => Some(CV::B(a <= b)),
                Ge => Some(CV::B(a >= b)),
                _ => None,
            },
            (CV::C(a), CV::C(b)) => match op {
                Eq => Some(CV::B(a == b)),
                Ne => Some(CV::B(a != b)),
                Lt => Some(CV::B(a < b)),
                Gt => Some(CV::B(a > b)),
                Le => Some(CV::B(a <= b)),
                Ge => Some(CV::B(a >= b)),
                _ => None,
            },
            _ => None,
        }
    }

    fn check_entry(&mut self, prog: &Program) {
        let name = if self.fns.contains_key("主") {
            "主"
        } else if self.fns.contains_key("shu") {
            "shu"
        } else {
            let d = Diagnostic::error(E0204, "`主` function not found in the program")
                .note("every program starts at a function named `主` (shu, \"main\")")
                .help("add one, for example: `関数 主() { 表示!(\"こんにちは世界\"); }` (kansuu shu, hyouji!: \"function main, println!\")");
            self.emit(d);
            return;
        };
        let f = prog
            .items
            .iter()
            .find_map(|i| match i {
                Item::Fn(f) if f.name.name == name => Some(f),
                _ => None,
            })
            .unwrap();
        if let Some(p) = f.params.first() {
            let d = Diagnostic::error(E0205, "`主` must not take parameters")
                .primary(p.name.span, "remove the parameters")
                .note("`主` (shu, \"main\") is called by the system, which passes no arguments");
            self.emit(d);
        }
        if let Some(r) = &f.ret {
            let d = Diagnostic::error(E0205, "`主` must not return a value")
                .primary(r.span, "remove the return type")
                .note("`主` (shu, \"main\") returns nothing; exit codes arrive in a later stage");
            self.emit(d);
        }
        self.entry = Some(self.fns[name].cname.clone());
    }

    fn check_const_cycles(&mut self, prog: &Program) {
        // busca em profundidade; uma constante que depende de si mesma
        // viraria recursao infinita no C
        fn visit(
            n: &str,
            deps: &HashMap<String, Vec<String>>,
            state: &mut HashMap<String, u8>,
        ) -> bool {
            match state.get(n) {
                Some(1) => return true,
                Some(2) => return false,
                _ => {}
            }
            state.insert(n.to_string(), 1);
            if let Some(ds) = deps.get(n) {
                for d in ds {
                    if visit(d, deps, state) {
                        return true;
                    }
                }
            }
            state.insert(n.to_string(), 2);
            false
        }
        let mut state = HashMap::new();
        for item in &prog.items {
            if let Item::Const(c) = item {
                if self.names.get(&c.name.name) != Some(&c.name.span) {
                    continue;
                }
                if visit(&c.name.name, &self.const_deps, &mut state) {
                    let d = Diagnostic::error(
                        E0206,
                        format!("constant `{}` depends on itself", c.name.name),
                    )
                    .primary(c.name.span, "cycle detected when evaluating this constant");
                    self.emit(d);
                    return;
                }
            }
        }
    }

    fn begin_body(&mut self, ret: Ty) {
        self.buf.clear();
        self.scopes.clear();
        self.scopes.push(Vec::new());
        self.loop_stack.clear();
        self.cur_ret = ret;
    }

    fn gen_const(&mut self, c: &ConstItem) {
        let ty = self.consts[&c.name.name].ty;
        self.begin_body(Ty::Err);
        self.cur_const = Some(c.name.name.clone());
        self.const_deps.entry(c.name.name.clone()).or_default();
        let v = self.gen_expr(&c.value, Some(ty));
        self.check(&v, ty, Some(&c.value));
        self.pop_scope();
        self.cur_const = None;
        self.buf.clear();
    }

    fn gen_fn(&mut self, f: &FnItem, owner: Option<u16>) {
        let (params, ret, ret_span, cname) = {
            let s = match owner {
                Some(id) => &self.methods[&(id, f.name.name.clone())],
                None => &self.fns[&f.name.name],
            };
            (
                s.params.iter().map(|p| p.0).collect::<Vec<_>>(),
                s.ret,
                s.ret_span,
                s.cname.clone(),
            )
        };
        self.begin_body(ret);
        self.cur_self = owner;
        let mut c_params = Vec::new();
        let mut ptys = params.iter();
        if let Some(sp) = &f.self_param {
            if let Some(ty) = ptys.next() {
                let id = Ident { name: "自分".to_string(), span: sp.span };
                let cn = self.declare(&id, *ty, sp.mutable, None);
                let li = self.locals.len() - 1;
                self.locals[li].used = true;
                c_params.push(format!("{} {}", ty.c(), cn));
            }
        }
        for (p, ty) in f.params.iter().zip(ptys) {
            let cn = self.declare(&p.name, *ty, false, None);
            c_params.push(format!("{} {}", ty.c(), cn));
        }
        let r = self.gen_block(&f.body, Some(ret));

        if ret == Ty::Unit {
            if r.ty.has_value() {
                self.mismatch(&r, Ty::Unit, None, None);
            }
        } else if r.ty == Ty::Unit {
            self.report_missing_tail(f, ret, ret_span);
        } else {
            self.check(&r, ret, None);
            if r.ty != Ty::Never && !r.c.is_empty() {
                self.line(format!("return {};", r.c));
            } else {
                self.line("abort();");
            }
        }
        self.pop_scope();

        let body = std::mem::take(&mut self.buf);
        let plist = if c_params.is_empty() {
            "void".to_string()
        } else {
            c_params.join(", ")
        };
        let head = format!("static {} {}({})", ret.c(), cname, plist);
        self.protos.push_str(&format!("{};\n", head));
        self.defs.push_str(&format!("{} {{\n{}}}\n\n", head, body));
        self.cur_self = None;
    }

    /// Corpo sem valor final numa funcao que devia devolver algo.
    fn report_missing_tail(&mut self, f: &FnItem, ret: Ty, ret_span: Option<Span>) {
        let last = f.body.stmts.last();
        let mut d;
        if let Some(Stmt {
            kind: StmtKind::Expr(_),
            span,
        }) = last
        {
            if self.text(*span).ends_with(';') {
                // pode ser so o `;` a mais
                d = Diagnostic::error(E0307, "mismatched types")
                    .primary(
                        *span,
                        format!("expected {}, found `()` because of the `;`", self.bt(ret)),
                    )
                    .suggest(
                        "remove this semicolon to return the value",
                        Span::new(span.end - 1, span.end),
                        "",
                        true,
                    );
                if let Some(rs) = ret_span {
                    d = d.secondary(rs, format!("expected {} because of this return type", self.bt(ret)));
                }
                self.emit(d);
                return;
            }
        }
        d = Diagnostic::error(E0301, "mismatched types")
            .primary(
                f.body.span,
                format!("this function body has type `()`, but should return {}", self.bt(ret)),
            )
            .note("the last expression of a block, without `;`, is the value of the block")
            .help("end the body with a value, or use `返却` (henkyaku, \"return\") on every path");
        if let Some(rs) = ret_span {
            d = d.secondary(rs, format!("expected {} because of this return type", self.bt(ret)));
        }
        self.emit(d);
    }

    fn finish(&self) -> String {
        let mut out = String::new();
        out.push_str("/* generated by nihonc: do not edit */\n");
        out.push_str(RUNTIME);
        out.push('\n');
        out.push_str(&self.types);
        out.push_str(&self.protos);
        out.push('\n');
        out.push_str(&self.defs);
        out.push_str("int main(void) {\n  nh_setup();\n");
        if let Some(e) = &self.entry {
            out.push_str(&format!("  {}();\n", e));
        }
        out.push_str("  fflush(stdout);\n  return 0;\n}\n");
        indent(&out)
    }

    // ----------------------------------------------------------------- blocos

    fn gen_block(&mut self, b: &Block, expect: Option<Ty>) -> Val {
        self.push_scope();
        let mut diverged: Option<Span> = None;
        let mut warned = false;
        for s in &b.stmts {
            if let (Some(d), false) = (diverged, warned) {
                if !matches!(s.kind, StmtKind::Empty) {
                    self.warn_unreachable(s.span, d, "statement");
                    warned = true;
                }
            }
            if self.gen_stmt(s) && diverged.is_none() {
                diverged = Some(s.span);
            }
        }
        let res = match &b.tail {
            Some(t) => {
                if let (Some(d), false) = (diverged, warned) {
                    self.warn_unreachable(t.span, d, "expression");
                }
                let mut v = self.gen_expr(t, expect);
                if diverged.is_some() && v.ty != Ty::Err {
                    v.ty = Ty::Never;
                }
                v
            }
            None => Val {
                c: String::new(),
                ty: if diverged.is_some() { Ty::Never } else { Ty::Unit },
                span: Span::new(b.span.end.saturating_sub(1), b.span.end),
            },
        };
        self.pop_scope();
        res
    }

    fn warn_unreachable(&mut self, span: Span, cause: Span, what: &str) {
        let d = Diagnostic::warning(W0003, format!("unreachable {}", what))
            .secondary(cause, "any code following this is unreachable")
            .primary(span, format!("unreachable {}", what));
        self.emit(d);
    }

    /// Devolve `true` se o comando nunca termina normalmente.
    fn gen_stmt(&mut self, s: &Stmt) -> bool {
        match &s.kind {
            StmtKind::Empty => false,
            StmtKind::Let {
                mutable,
                name,
                ty,
                init,
            } => {
                let annot = ty.as_ref().map(|t| self.resolve_type(t));
                let v = self.gen_expr(init, annot);
                if let Some(a) = annot {
                    self.check(&v, a, Some(init));
                }
                let vty = match (annot, v.ty) {
                    (Some(a), _) => a,
                    (None, Ty::Unit) => {
                        let d = Diagnostic::error(E0301, "cannot bind a value of type `()`")
                            .primary(init.span, "this expression has no value");
                        self.emit(d);
                        Ty::Err
                    }
                    (None, Ty::Never) => Ty::Err,
                    (None, t) => t,
                };
                let mut_span = if *mutable { self.find_mut_span(name.span) } else { None };
                let cname = self.declare(name, vty, *mutable, mut_span);
                if vty.has_value() {
                    if v.ty == Ty::Never || v.c.is_empty() {
                        self.line(format!("{} {};", vty.c(), cname));
                    } else {
                        self.line(format!("{} {} = {};", vty.c(), cname, v.c));
                    }
                }
                v.ty == Ty::Never
            }
            StmtKind::Assign { target, op, value } => {
                self.gen_assign(target, *op, value);
                false
            }
            StmtKind::Return(value) => {
                let ret = self.cur_ret;
                match value {
                    Some(e) => {
                        let v = self.gen_expr(e, Some(ret));
                        self.check(&v, ret, Some(e));
                        if ret == Ty::Unit {
                            // `返却 x;` em funcao sem retorno: ja deu erro acima
                        } else if v.c.is_empty() {
                            self.line("return;");
                        } else {
                            self.line(format!("return {};", v.c));
                        }
                    }
                    None => {
                        if ret != Ty::Unit && ret != Ty::Err {
                            let v = self.unit(s.span);
                            self.mismatch(&v, ret, None, None);
                        }
                        self.line("return;");
                    }
                }
                true
            }
            StmtKind::Break | StmtKind::Continue => {
                let is_break = matches!(s.kind, StmtKind::Break);
                if self.loop_stack.is_empty() {
                    let (jp, ro, mean) = if is_break {
                        ("中断", "chuudan", "break")
                    } else {
                        ("続行", "zokkou", "continue")
                    };
                    let d = Diagnostic::error(E0402, format!("`{}` outside of a loop", jp))
                        .primary(s.span, format!("`{}` ({}, \"{}\") only works inside a loop", jp, ro, mean))
                        .note("loops are `間` (aida, \"while\"), `無限` (mugen, \"loop\") and `各` (kaku, \"for each\")");
                    self.emit(d);
                } else if is_break {
                    *self.loop_stack.last_mut().unwrap() = true;
                    self.line("break;");
                } else {
                    self.line("continue;");
                }
                true
            }
            StmtKind::Expr(e) => {
                let v = self.gen_expr(e, None);
                v.ty == Ty::Never
            }
        }
    }

    /// O span da palavra `可変` colada antes do nome (com o espaco depois).
    fn find_mut_span(&self, name: Span) -> Option<Span> {
        let before = &self.file.text[..name.start];
        let trimmed = before.trim_end();
        for kw in ["可変", "kahen"] {
            if trimmed.ends_with(kw) {
                return Some(Span::new(trimmed.len() - kw.len(), name.start));
            }
        }
        None
    }

    fn gen_assign(&mut self, target: &Expr, op: AssignOp, value: &Expr) {
        let mut t = target;
        while let ExprKind::Paren(x) = &t.kind {
            t = x;
        }
        match &t.kind {
            ExprKind::Path(id) => self.assign_local(id, op, value),
            ExprKind::Field { .. } => self.assign_field(t, op, value),
            _ => {
                let d = Diagnostic::error(E0628, "invalid left-hand side of assignment")
                    .primary(target.span, "cannot assign to this expression")
                    .note("only variables and fields (`p.x`) can be assigned to");
                self.emit(d);
                let _ = self.gen_expr(value, None);
            }
        }
    }

    fn assign_field(&mut self, target: &Expr, op: AssignOp, value: &Expr) {
        let (pv, pi) = self.gen_place(target);
        let v = self.gen_expr(value, Some(pv.ty));
        if matches!(pv.ty, Ty::Err | Ty::Never) {
            return;
        }
        if !pi.is_place {
            let d = Diagnostic::error(E0628, "cannot assign to a field of a temporary value")
                .primary(target.span, "this value is not stored in a variable")
                .help("store it in a variable first with `束縛 可変` (sokubaku kahen, \"let mut\")");
            self.emit(d);
            return;
        }
        if !self.require_mutable(&pi, target.span, "assign to") {
            return;
        }
        match op {
            AssignOp::Set => {
                self.check(&v, pv.ty, Some(value));
                if v.ty != Ty::Never && !v.c.is_empty() {
                    self.line(format!("{} = {};", pv.c, v.c));
                }
            }
            _ => {
                let bop = match op {
                    AssignOp::Add => BinOp::Add,
                    AssignOp::Sub => BinOp::Sub,
                    AssignOp::Mul => BinOp::Mul,
                    AssignOp::Div => BinOp::Div,
                    _ => BinOp::Rem,
                };
                let lhs = Val {
                    c: pv.c.clone(),
                    ty: pv.ty,
                    span: target.span,
                };
                let span = Span::new(target.span.start, value.span.end);
                if let Some(r) = self.arith(bop, lhs, v, span, None) {
                    self.line(format!("{} = {};", pv.c, r.c));
                }
            }
        }
    }

    /// Confere se o lugar pode ser mudado (ou emprestado como `&可変`).
    fn require_mutable(&mut self, info: &PlaceInfo, span: Span, action: &str) -> bool {
        let act = |text: &str| {
            if action == "borrow as mutable" {
                format!("borrow `{}` as mutable", text)
            } else {
                format!("{} `{}`", action, text)
            }
        };
        match (info.via_ref, info.root) {
            (Some(true), _) => {
                if let Some(r) = info.root {
                    self.locals[r].assigned = true;
                }
                true
            }
            (Some(false), _) => {
                let d = Diagnostic::error(
                    E0401,
                    format!("cannot {}, which is behind a `&` reference", act(self.text(span))),
                )
                .primary(span, "this reference does not allow changes")
                .help("take it as `&可変` (kahen, \"mutable\") instead of `&`");
                self.emit(d);
                false
            }
            (None, Some(r)) => {
                if self.locals[r].mutable {
                    self.locals[r].assigned = true;
                    return true;
                }
                let (name, decl_span, is_param) = {
                    let l = &self.locals[r];
                    (
                        l.name.clone(),
                        l.span,
                        l.mut_span.is_none() && self.scopes.first().map_or(false, |sc| sc.contains(&r)),
                    )
                };
                let mut d = Diagnostic::error(
                    E0401,
                    format!("cannot {}, as `{}` is not declared as mutable", act(self.text(span)), name),
                )
                .primary(span, "not mutable")
                .secondary(decl_span, format!("`{}` is declared here", name));
                if is_param {
                    d = d.note("function parameters are always immutable")
                        .help("copy it into a mutable variable first: `束縛 可変 x = x;` (sokubaku kahen, \"let mut\")");
                } else {
                    d = d.suggest(
                        "consider making this binding mutable with `可変` (kahen, \"mutable\")",
                        Span::new(decl_span.start, decl_span.start),
                        "可変 ",
                        false,
                    );
                }
                self.emit(d);
                false
            }
            (None, None) => {
                let d = Diagnostic::error(E0401, format!("cannot {} a temporary value", action))
                    .primary(span, "this value is not stored in a variable");
                self.emit(d);
                false
            }
        }
    }

    fn assign_local(&mut self, target: &Ident, op: AssignOp, value: &Expr) {
        let Some(idx) = self.lookup_local(&target.name) else {
            let mut d = Diagnostic::error(
                E0201,
                format!("cannot find value `{}` in this scope", target.name),
            )
            .primary(target.span, "not found in this scope");
            if let Some(s) = similar(&target.name, self.visible_names().iter().map(|s| s.as_str())) {
                d = d.suggest("a local variable with a similar name exists", target.span, s, false);
            }
            self.emit(d);
            let _ = self.gen_expr(value, None);
            return;
        };
        let (lty, cname, mutable, decl_span, is_let) = {
            let l = &self.locals[idx];
            (l.ty, l.cname.clone(), l.mutable, l.span, l.mut_span.is_some() || true)
        };
        let _ = is_let;
        let v = self.gen_expr(value, Some(lty));
        if !mutable {
            let first_use_is_param = self.locals[idx].mut_span.is_none()
                && self.scopes.first().map_or(false, |s| s.contains(&idx));
            let mut d = Diagnostic::error(
                E0401,
                format!("cannot assign twice to immutable variable `{}`", target.name),
            )
            .secondary(decl_span, format!("first assignment to `{}`", target.name))
            .primary(target.span, "cannot assign twice to immutable variable");
            if first_use_is_param {
                d = d.note("function parameters are always immutable")
                    .help("copy it into a mutable variable first: `束縛 可変 x = x;` (sokubaku kahen, \"let mut\")");
            } else {
                d = d.suggest(
                    "consider making this binding mutable with `可変` (kahen, \"mutable\")",
                    Span::new(decl_span.start, decl_span.start),
                    "可変 ",
                    false,
                );
            }
            self.emit(d);
            return;
        }
        self.locals[idx].assigned = true;
        if lty == Ty::Err {
            return;
        }
        match op {
            AssignOp::Set => {
                self.check(&v, lty, Some(value));
                if v.ty != Ty::Never && !v.c.is_empty() {
                    self.line(format!("{} = {};", cname, v.c));
                }
            }
            _ => {
                let bop = match op {
                    AssignOp::Add => BinOp::Add,
                    AssignOp::Sub => BinOp::Sub,
                    AssignOp::Mul => BinOp::Mul,
                    AssignOp::Div => BinOp::Div,
                    _ => BinOp::Rem,
                };
                let lhs = Val {
                    c: cname.clone(),
                    ty: lty,
                    span: target.span,
                };
                let span = Span::new(target.span.start, value.span.end);
                if let Some(r) = self.arith(bop, lhs, v, span, None) {
                    self.line(format!("{} = {};", cname, r.c));
                }
            }
        }
    }

    // ------------------------------------------------------------- expressoes

    fn gen_expr(&mut self, e: &Expr, expect: Option<Ty>) -> Val {
        let span = e.span;
        match &e.kind {
            ExprKind::Int(n) => {
                let ty = match expect {
                    Some(t @ (Ty::I32 | Ty::U32)) => t,
                    _ => Ty::I32,
                };
                self.int_lit(*n, false, ty, span)
            }
            ExprKind::Float(s) => {
                let mut t: String = s.chars().filter(|c| *c != '_').collect();
                if !t.contains(['.', 'e', 'E']) {
                    t.push_str(".0");
                }
                Val {
                    c: t,
                    ty: Ty::F64,
                    span,
                }
            }
            ExprKind::Bool(b) => Val {
                c: if *b { "1" } else { "0" }.to_string(),
                ty: Ty::Bool,
                span,
            },
            ExprKind::Char(c) => Val {
                c: format!("((uint32_t){})", *c as u32),
                ty: Ty::Char,
                span,
            },
            ExprKind::Str(t) => Val {
                c: format!("((nh_str){{\"{}\", {}}})", c_escape(t), t.len()),
                ty: Ty::Str,
                span,
            },
            ExprKind::Paren(inner) => {
                let mut v = self.gen_expr(inner, expect);
                v.span = span;
                v
            }
            ExprKind::Path(id) => {
                let v = self.gen_path(id);
                // struct lida de uma variavel: copia na hora (como no Rust, ela "move")
                if matches!(v.ty, Ty::Struct(_)) && self.lookup_local(&id.name).is_some() {
                    let t = self.tmp();
                    self.line(format!("{} {} = {};", v.ty.c(), t, v.c));
                    Val { c: t, ..v }
                } else {
                    v
                }
            }
            ExprKind::Field { .. } => {
                let (v, i) = self.gen_place(e);
                if i.is_place && v.ty.has_value() {
                    let t = self.tmp();
                    self.line(format!("{} {} = {};", v.ty.c(), t, v.c));
                    Val { c: t, ..v }
                } else {
                    v
                }
            }
            ExprKind::AssocPath { ty, name } => {
                let d = Diagnostic::error(
                    E0202,
                    format!("function `{}::{}` is used as a value", ty.name, name.name),
                )
                .primary(e.span, "functions are not values yet")
                .help(format!("call it: `{}::{}(...)`", ty.name, name.name));
                self.emit(d);
                self.err_val(span)
            }
            ExprKind::StructLit { name, fields } => self.gen_struct_lit(name, fields, span),
            ExprKind::Ref { mutable, expr } => self.gen_ref(*mutable, expr, span),
            ExprKind::Unary { op, expr } => self.gen_unary(*op, expr, expect, span),
            ExprKind::Binary { op, lhs, rhs } => self.gen_binary(*op, lhs, rhs, expect, span),
            ExprKind::Cast { expr, ty } => self.gen_cast(expr, ty, span),
            ExprKind::Range { .. } => {
                let d = Diagnostic::error(E0999, "ranges are only supported in `各` loops for now")
                    .primary(span, "a range value cannot be stored or passed around yet")
                    .note("write it directly in the loop: `各 i 中 0..10 { ... }` (kaku i naka)");
                self.emit(d);
                self.err_val(span)
            }
            ExprKind::Call { callee, args } => self.gen_call(callee, args, span),
            ExprKind::MethodCall { recv, name, args } => self.gen_method(recv, name, args, span),
            ExprKind::MacroCall {
                name,
                name_span,
                args,
            } => self.gen_macro(name, *name_span, args, span),
            ExprKind::If { cond, then, else_ } => self.gen_if(cond, then, else_.as_deref(), expect, span),
            ExprKind::While { cond, body } => {
                let (cs, cv) = self.capture(|s| s.gen_cond(cond));
                self.loop_stack.push(false);
                let (bs, _) = self.capture(|s| s.gen_block(body, None));
                self.loop_stack.pop();
                if cs.is_empty() {
                    self.line(format!("while ({}) {{", cv));
                    self.buf.push_str(&bs);
                } else {
                    self.line("while (1) {");
                    self.buf.push_str(&cs);
                    self.line(format!("if (!({})) break;", cv));
                    self.buf.push_str(&bs);
                }
                self.line("}");
                self.unit(span)
            }
            ExprKind::Loop { body } => {
                self.loop_stack.push(false);
                let (bs, _) = self.capture(|s| s.gen_block(body, None));
                let has_break = self.loop_stack.pop().unwrap_or(false);
                self.line("for (;;) {");
                self.buf.push_str(&bs);
                self.line("}");
                Val {
                    c: String::new(),
                    ty: if has_break { Ty::Unit } else { Ty::Never },
                    span,
                }
            }
            ExprKind::For { var, iter, body } => self.gen_for(var, iter, body, span),
            ExprKind::Block(b) => self.gen_block(b, expect),
        }
    }

    fn int_lit(&mut self, n: u128, neg: bool, ty: Ty, span: Span) -> Val {
        self.lit_tys.insert(span.start, ty);
        let (ok, c) = match ty {
            Ty::U32 => (n <= u32::MAX as u128, format!("{}u", n)),
            _ => {
                if neg && n == 2147483648 {
                    (true, "(-2147483647-1)".to_string())
                } else if neg {
                    (n <= 2147483647, format!("(-{})", n))
                } else {
                    (n <= 2147483647, format!("{}", n))
                }
            }
        };
        if !ok {
            let (lo, hi) = if ty == Ty::U32 { ("0", "4294967295") } else { ("-2147483648", "2147483647") };
            let d = Diagnostic::error(E0306, "integer literal is out of range")
                .primary(span, format!("does not fit in {}", self.bt(ty)))
                .note(format!("the range of {} is {} to {}", self.bt(ty), lo, hi));
            self.emit(d);
            return self.err_val(span);
        }
        Val { c, ty, span }
    }

    fn gen_path(&mut self, id: &Ident) -> Val {
        if let Some(i) = self.lookup_local(&id.name) {
            self.locals[i].used = true;
            let l = &self.locals[i];
            return Val {
                c: if l.ty.has_value() { l.cname.clone() } else { "0".to_string() },
                ty: l.ty,
                span: id.span,
            };
        }
        if let Some(c) = self.consts.get(&id.name) {
            let (ty, cname) = (c.ty, c.cname.clone());
            if let Some(cur) = &self.cur_const {
                let cur = cur.clone();
                self.const_deps.entry(cur).or_default().push(id.name.clone());
            }
            return Val {
                c: format!("{}()", cname),
                ty,
                span: id.span,
            };
        }
        let mut d = Diagnostic::error(
            E0201,
            format!("cannot find value `{}` in this scope", id.name),
        )
        .primary(id.span, "not found in this scope");
        if id.name == "自分" {
            d = d.note("`自分` (jibun, \"self\") only exists inside a method that takes it as its first parameter");
        } else if self.fns.contains_key(&id.name) {
            d = d.note(format!(
                "`{}` is a function; call it with `{}(...)`",
                id.name, id.name
            ));
        } else if let Some(s) = similar(&id.name, self.visible_names().iter().map(|s| s.as_str())) {
            d = d.suggest("a value with a similar name exists", id.span, s, false);
        }
        self.emit(d);
        self.err_val(id.span)
    }

    fn gen_unary(&mut self, op: UnOp, inner: &Expr, expect: Option<Ty>, span: Span) -> Val {
        match op {
            UnOp::Neg => {
                // `-2147483648` e um literal valido
                if let ExprKind::Int(n) = &inner.kind {
                    let ty = match expect {
                        Some(t @ (Ty::I32 | Ty::U32)) => t,
                        _ => Ty::I32,
                    };
                    if ty == Ty::U32 {
                        return self.bad_unary("-", Ty::U32, span, inner.span);
                    }
                    let mut v = self.int_lit(*n, true, ty, span);
                    v.span = span;
                    return v;
                }
                let v = self.gen_expr(inner, expect);
                match v.ty {
                    Ty::I32 => {
                        let t = self.tmp();
                        let loc = self.loc(span);
                        self.line(format!("int32_t {} = nh_neg_i32({}, \"{}\");", t, v.c, loc));
                        Val { c: t, ty: Ty::I32, span }
                    }
                    Ty::F64 => Val {
                        c: format!("(-{})", v.c),
                        ty: Ty::F64,
                        span,
                    },
                    Ty::Err | Ty::Never => Val { span, ..v },
                    t => self.bad_unary("-", t, span, inner.span),
                }
            }
            UnOp::Not => {
                let v = self.gen_expr(inner, Some(Ty::Bool));
                match v.ty {
                    Ty::Bool => Val {
                        c: format!("(!{})", v.c),
                        ty: Ty::Bool,
                        span,
                    },
                    Ty::Err | Ty::Never => Val { span, ..v },
                    t => self.bad_unary("!", t, span, inner.span),
                }
            }
        }
    }

    fn bad_unary(&mut self, op: &str, t: Ty, span: Span, operand: Span) -> Val {
        let mut d = Diagnostic::error(
            E0304,
            format!("cannot apply unary operator `{}` to type {}", op, self.bt(t)),
        )
        .primary(span, format!("cannot apply `{}` to {}", op, self.bt(t)));
        if t == Ty::U32 && op == "-" {
            d = d.note("unsigned numbers cannot be negative");
        }
        if op == "!" {
            d = d.note("`!` only works on `真偽` (shingi, \"boolean\") for now");
        }
        let _ = operand;
        self.emit(d);
        self.err_val(span)
    }

    fn gen_binary(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr, expect: Option<Ty>, span: Span) -> Val {
        if matches!(op, BinOp::And | BinOp::Or) {
            return self.gen_logic(op, lhs, rhs, span);
        }
        let arith = matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem);
        let exp = if arith { expect } else { None };
        // um literal na esquerda assume o tipo do lado direito (`1 + x`)
        let (l, r) = if is_lit(lhs) && !is_lit(rhs) {
            let r = self.gen_expr(rhs, exp);
            let l = self.gen_expr(lhs, Some(r.ty));
            (l, r)
        } else {
            let l = self.gen_expr(lhs, exp);
            let r = self.gen_expr(rhs, Some(l.ty));
            (l, r)
        };
        if arith {
            match self.arith(op, l, r, span, Some(rhs)) {
                Some(v) => v,
                None => self.err_val(span),
            }
        } else {
            self.compare(op, l, r, span)
        }
    }

    fn op_str(op: BinOp) -> &'static str {
        match op {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Gt => ">",
            BinOp::Le => "<=",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }

    fn bad_binary(&mut self, op: BinOp, l: &Val, r: &Val, span: Span) {
        let o = Self::op_str(op);
        let _ = span;
        if op == BinOp::Add && (l.ty == Ty::Str) != (r.ty == Ty::Str) && (l.ty.is_num() || r.ty.is_num()) {
            let (text, num) = if l.ty == Ty::Str { (&l, &r) } else { (&r, &l) };
            let d = Diagnostic::error(E0617, "cannot add a number to text")
                .secondary(text.span, "text (`文字列`, mojiretsu)")
                .secondary(num.span, format!("number ({})", self.bt(num.ty)))
                .primary(span, "`+` joins two texts, not a text and a number")
                .help("build the text with `整形!` (seikei!, \"format!\"), for example `整形!(\"{}{}\", a, b)`");
            self.emit(d);
            return;
        }
        let (gs, ge) = (l.span.end.min(r.span.start), r.span.start.max(l.span.end));
        let between = &self.file.text[gs..ge];
        let lead = between.len() - between.trim_start().len();
        let op_span = Span::new(gs + lead, gs + lead + between.trim().len());
        let mut d = Diagnostic::error(
            E0304,
            format!("cannot apply `{}` to {} and {}", o, self.bt(l.ty), self.bt(r.ty)),
        )
        .secondary(l.span, self.bt(l.ty))
        .secondary(r.span, self.bt(r.ty))
        .primary(
            op_span,
            format!("no implementation for `{} {} {}`", self.tjp(l.ty), o, self.tjp(r.ty)),
        );
        for (i, t) in [l.ty, r.ty].into_iter().enumerate() {
            if i == 1 && l.ty == r.ty {
                break;
            }
            let n = self.tlong(t);
            if !n.is_empty() {
                d = d.note(n);
            }
        }
        if let (Ty::Struct(_), true) = (l.ty, l.ty == r.ty) {
            d = d.help("compare the fields one by one, for example `a.x == b.x`");
        }
        if l.ty.is_num() && r.ty.is_num() {
            d = d.help(format!(
                "both sides must have the same type; convert one with `として` (toshite, \"as\"), for example `x として {}`",
                self.tjp(l.ty)
            ));
        }
        self.emit(d);
    }

    fn arith(&mut self, op: BinOp, l: Val, r: Val, span: Span, rhs_expr: Option<&Expr>) -> Option<Val> {
        let _ = rhs_expr;
        if l.ty == Ty::Err || r.ty == Ty::Err {
            return Some(self.err_val(span));
        }
        if l.ty == Ty::Never || r.ty == Ty::Never {
            return Some(Val {
                c: String::new(),
                ty: Ty::Never,
                span,
            });
        }
        if op == BinOp::Add && l.ty == Ty::Str && r.ty == Ty::Str {
            let t = self.tmp();
            self.line(format!("nh_str {} = nh_str_concat({}, {});", t, l.c, r.c));
            return Some(Val { c: t, ty: Ty::Str, span });
        }
        if l.ty != r.ty || !l.ty.is_num() {
            self.bad_binary(op, &l, &r, span);
            return None;
        }
        let ty = l.ty;
        if ty == Ty::F64 {
            let c = match op {
                BinOp::Rem => format!("fmod({}, {})", l.c, r.c),
                _ => format!("({} {} {})", l.c, Self::op_str(op), r.c),
            };
            return Some(Val { c, ty, span });
        }
        let name = match op {
            BinOp::Add => "add",
            BinOp::Sub => "sub",
            BinOp::Mul => "mul",
            BinOp::Div => "div",
            _ => "rem",
        };
        let suffix = if ty == Ty::I32 { "i32" } else { "u32" };
        let t = self.tmp();
        let loc = self.loc(span);
        self.line(format!(
            "{} {} = nh_{}_{}({}, {}, \"{}\");",
            ty.c(),
            t,
            name,
            suffix,
            l.c,
            r.c,
            loc
        ));
        Some(Val { c: t, ty, span })
    }

    fn compare(&mut self, op: BinOp, l: Val, r: Val, span: Span) -> Val {
        if l.ty == Ty::Err || r.ty == Ty::Err {
            return self.err_val(span);
        }
        if l.ty == Ty::Never || r.ty == Ty::Never {
            return Val {
                c: String::new(),
                ty: Ty::Never,
                span,
            };
        }
        let eq = matches!(op, BinOp::Eq | BinOp::Ne);
        let ok = l.ty == r.ty && l.ty.is_simple() && (eq || l.ty != Ty::Bool);
        if !ok {
            self.bad_binary(op, &l, &r, span);
            return self.err_val(span);
        }
        if l.ty == Ty::Str {
            return Val {
                c: format!("(nh_str_cmp({}, {}) {} 0)", l.c, r.c, Self::op_str(op)),
                ty: Ty::Bool,
                span,
            };
        }
        Val {
            c: format!("({} {} {})", l.c, Self::op_str(op), r.c),
            ty: Ty::Bool,
            span,
        }
    }

    fn gen_logic(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr, span: Span) -> Val {
        let l = self.gen_expr(lhs, Some(Ty::Bool));
        self.check(&l, Ty::Bool, Some(lhs));
        let (rs, r) = self.capture(|s| s.gen_expr(rhs, Some(Ty::Bool)));
        self.check(&r, Ty::Bool, Some(rhs));
        let o = Self::op_str(op);
        if rs.is_empty() {
            return Val {
                c: format!("({} {} {})", l.c, o, r.c),
                ty: Ty::Bool,
                span,
            };
        }
        // o lado direito tem comandos: so pode rodar se for preciso
        let t = self.tmp();
        self.line(format!("int {} = {};", t, l.c));
        let cond = if op == BinOp::And { t.clone() } else { format!("!{}", t) };
        self.line(format!("if ({}) {{", cond));
        self.buf.push_str(&rs);
        self.line(format!("{} = {};", t, r.c));
        self.line("}");
        Val {
            c: t,
            ty: Ty::Bool,
            span,
        }
    }

    fn gen_cast(&mut self, expr: &Expr, ty: &Type, span: Span) -> Val {
        let target = self.resolve_type(ty);
        let v = self.gen_expr(expr, None);
        if target == Ty::Err || v.ty == Ty::Err {
            return self.err_val(span);
        }
        if v.ty == Ty::Never {
            return Val {
                c: String::new(),
                ty: Ty::Never,
                span,
            };
        }
        let from = v.ty;
        let ok = match (from, target) {
            (a, b) if a.is_int() && b.is_num() => true,
            (Ty::F64, b) if b.is_num() => true,
            (Ty::Bool | Ty::Char, b) if b.is_int() => true,
            (a, b) if a == b => true,
            _ => false,
        };
        if !ok {
            let mut d = Diagnostic::error(
                E0305,
                format!("cannot cast {} as {}", self.bt(from), self.bt(target)),
            )
            .primary(span, "invalid cast");
            d = d.note("`として` (toshite, \"as\") converts between numbers; `真偽` and `文字` can only become integers");
            self.emit(d);
            return self.err_val(span);
        }
        let c = match (from, target) {
            (Ty::F64, Ty::I32) => format!("nh_f2i32({})", v.c),
            (Ty::F64, Ty::U32) => format!("nh_f2u32({})", v.c),
            (_, Ty::F64) => format!("((double){})", v.c),
            (a, b) if a == b => v.c,
            _ => format!("(({}){})", target.c(), v.c),
        };
        Val {
            c,
            ty: target,
            span,
        }
    }

    fn gen_call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> Val {
        if let ExprKind::AssocPath { ty, name } = &callee.kind {
            return self.gen_assoc_call(ty, name, args, span);
        }
        let ExprKind::Path(id) = &callee.kind else {
            let d = Diagnostic::error(E0301, "expected a function name")
                .primary(callee.span, "only named functions can be called");
            self.emit(d);
            return self.err_val(span);
        };
        if self.cur_const.is_some() {
            let d = Diagnostic::error(E0207, "function calls are not allowed in constants")
                .primary(span, "call in a constant")
                .note("constants are evaluated without running user code (for now)");
            self.emit(d);
            return self.err_val(span);
        }
        if !self.fns.contains_key(&id.name) {
            if let Some(i) = self.lookup_local(&id.name) {
                self.locals[i].used = true;
                let t = self.locals[i].ty;
                let d = Diagnostic::error(
                    E0301,
                    format!("expected function, found {}", self.bt(t)),
                )
                .primary(id.span, format!("`{}` is a variable, not a function", id.name))
                .secondary(self.locals[i].span, "variable defined here");
                self.emit(d);
            } else {
                let mut d = Diagnostic::error(
                    E0202,
                    format!("cannot find function `{}` in this scope", id.name),
                )
                .primary(id.span, "not found in this scope");
                if let Some(s) = similar(&id.name, self.fns.keys().map(|s| s.as_str())) {
                    d = d.suggest("a function with a similar name exists", id.span, s, false);
                }
                self.emit(d);
            }
            for a in args {
                let _ = self.gen_expr(a, None);
            }
            return self.err_val(span);
        }
        let (params, ret, name_span, cname) = {
            let s = &self.fns[&id.name];
            (s.params.clone(), s.ret, s.name_span, s.cname.clone())
        };
        self.call_common(&cname, &params, ret, name_span, Vec::new(), args, span, id.span, "function")
    }

    /// Confere os argumentos e escreve a chamada. `pre` sao argumentos ja
    /// prontos (o `自分` de um metodo).
    #[allow(clippy::too_many_arguments)]
    fn call_common(
        &mut self,
        cname: &str,
        params: &[(Ty, Span)],
        ret: Ty,
        name_span: Span,
        pre: Vec<Val>,
        args: &[Expr],
        span: Span,
        at: Span,
        what: &str,
    ) -> Val {
        let skip = pre.len();
        let mut vals = pre;
        for (i, a) in args.iter().enumerate() {
            let p = params.get(skip + i);
            let v = self.gen_expr(a, p.map(|p| p.0));
            if let Some((pty, pspan)) = p {
                if !compat(*pty, v.ty) {
                    self.mismatch(
                        &v,
                        *pty,
                        Some(a),
                        Some((*pspan, "expected due to the type of this parameter".to_string())),
                    );
                }
            }
            vals.push(v);
        }
        let want = params.len().saturating_sub(skip);
        if args.len() != want {
            let d = Diagnostic::error(
                E0302,
                format!(
                    "this {} takes {} but {} {} supplied",
                    what,
                    plural(want, "argument", "arguments"),
                    plural(args.len(), "argument", "arguments"),
                    if args.len() == 1 { "was" } else { "were" }
                ),
            )
            .secondary(name_span, format!("{} defined here", what))
            .primary(at, format!("expected {}", plural(want, "argument", "arguments")));
            self.emit(d);
            return self.err_val(span);
        }
        let list: Vec<String> = vals.iter().map(|v| v.c.clone()).collect();
        let call = format!("{}({})", cname, list.join(", "));
        if ret.has_value() {
            let t = self.tmp();
            self.line(format!("{} {} = {};", ret.c(), t, call));
            Val { c: t, ty: ret, span }
        } else {
            self.line(format!("{};", call));
            Val {
                c: String::new(),
                ty: ret,
                span,
            }
        }
    }

    fn gen_args_only(&mut self, args: &[Expr]) {
        for a in args {
            let _ = self.gen_expr(a, None);
        }
    }

    /// `点::原点(...)`: funcao associada de uma struct.
    fn gen_assoc_call(&mut self, ty: &Ident, name: &Ident, args: &[Expr], span: Span) -> Val {
        if self.cur_const.is_some() {
            let d = Diagnostic::error(E0207, "function calls are not allowed in constants")
                .primary(span, "call in a constant")
                .note("constants are evaluated without running user code (for now)");
            self.emit(d);
            return self.err_val(span);
        }
        let Some(id) = self.find_struct(ty) else {
            self.gen_args_only(args);
            return self.err_val(span);
        };
        let key = (id, name.name.clone());
        let Some(sig) = self.methods.get(&key) else {
            let sname = self.structs[id as usize].name.clone();
            let mut d = Diagnostic::error(
                E0202,
                format!("no function named `{}` found for struct `{}`", name.name, sname),
            )
            .primary(name.span, "not found");
            let cands: Vec<String> = self
                .methods
                .keys()
                .filter(|k| k.0 == id)
                .map(|k| k.1.clone())
                .collect();
            if let Some(sg) = similar(&name.name, cands.iter().map(|s| s.as_str())) {
                d = d.suggest("a function with a similar name exists", name.span, sg, false);
            }
            self.emit(d);
            self.gen_args_only(args);
            return self.err_val(span);
        };
        let (params, ret, name_span, cname) = (sig.params.clone(), sig.ret, sig.name_span, sig.cname.clone());
        self.call_common(&cname, &params, ret, name_span, Vec::new(), args, span, name.span, "function")
    }

    /// `&x` / `&可変 x`.
    fn gen_ref(&mut self, mutable: bool, inner: &Expr, span: Span) -> Val {
        let (v, info) = self.gen_place(inner);
        let id = match v.ty {
            Ty::Struct(id) => id,
            Ty::Err | Ty::Never => return self.err_val(span),
            Ty::Ref(..) => {
                let d = Diagnostic::error(E0622, "cannot take a reference to a reference")
                    .primary(span, "this value already is a reference")
                    .help("pass it as it is");
                self.emit(d);
                return self.err_val(span);
            }
            t => {
                let d = Diagnostic::error(
                    E0622,
                    format!("cannot take a reference to {} for now", self.bt(t)),
                )
                .primary(span, "references only point to structs for now");
                self.emit(d);
                return self.err_val(span);
            }
        };
        if mutable && !self.require_mutable(&info, inner.span, "borrow as mutable") {
            return self.err_val(span);
        }
        let base = if info.is_place {
            v.c
        } else {
            let t = self.tmp();
            self.line(format!("{} {} = {};", v.ty.c(), t, v.c));
            t
        };
        Val {
            c: format!("(&{})", base),
            ty: Ty::Ref(id, mutable),
            span,
        }
    }

    /// Expressao que pode ser um "lugar" (variavel ou campo de um lugar).
    fn gen_place(&mut self, e: &Expr) -> (Val, PlaceInfo) {
        match &e.kind {
            ExprKind::Paren(inner) => {
                let (mut v, i) = self.gen_place(inner);
                v.span = e.span;
                (v, i)
            }
            ExprKind::Path(id) if self.lookup_local(&id.name).is_some() => {
                let idx = self.lookup_local(&id.name).unwrap();
                let v = self.gen_path(id);
                (
                    v,
                    PlaceInfo {
                        is_place: true,
                        root: Some(idx),
                        via_ref: None,
                    },
                )
            }
            ExprKind::Field { base, name } => {
                let (bv, bi) = self.gen_place(base);
                self.field_of(bv, bi, name, e.span)
            }
            _ => {
                let v = self.gen_expr(e, None);
                (
                    v,
                    PlaceInfo {
                        is_place: false,
                        root: None,
                        via_ref: None,
                    },
                )
            }
        }
    }

    fn field_of(&mut self, bv: Val, bi: PlaceInfo, name: &Ident, span: Span) -> (Val, PlaceInfo) {
        let no = PlaceInfo {
            is_place: false,
            root: None,
            via_ref: None,
        };
        let (id, through) = match bv.ty {
            Ty::Struct(id) => (id, None),
            Ty::Ref(id, m) => (id, Some(m)),
            Ty::Err | Ty::Never => return (self.err_val(span), no),
            t => {
                let d = Diagnostic::error(
                    E0626,
                    format!("no field `{}` on type {}", name.name, self.bt(t)),
                )
                .primary(name.span, "unknown field")
                .note("only structs have fields");
                self.emit(d);
                return (self.err_val(span), no);
            }
        };
        let found = self.structs[id as usize]
            .fields
            .iter()
            .find(|f| f.name == name.name)
            .map(|f| (f.ty, f.cname.clone()));
        let Some((fty, fc)) = found else {
            let sname = self.structs[id as usize].name.clone();
            let names: Vec<String> = self.structs[id as usize].fields.iter().map(|f| f.name.clone()).collect();
            let mut d = Diagnostic::error(
                E0626,
                format!("no field `{}` on struct `{}`", name.name, sname),
            )
            .primary(name.span, "unknown field");
            if let Some(sg) = similar(&name.name, names.iter().map(|s| s.as_str())) {
                d = d.suggest("a field with a similar name exists", name.span, sg, false);
            }
            if names.is_empty() {
                d = d.note(format!("`{}` has no fields", sname));
            } else {
                d = d.note(format!(
                    "available fields: {}",
                    names.iter().map(|n| format!("`{}`", n)).collect::<Vec<_>>().join(", ")
                ));
            }
            self.emit(d);
            return (self.err_val(span), no);
        };
        let c = match through {
            Some(_) => format!("{}->{}", bv.c, fc),
            None => format!("{}.{}", bv.c, fc),
        };
        let info = match through {
            Some(m) => PlaceInfo {
                is_place: true,
                root: bi.root,
                via_ref: Some(m),
            },
            None => bi,
        };
        (Val { c, ty: fty, span }, info)
    }

    /// `点 { x: 1, y: 2 }`.
    fn gen_struct_lit(&mut self, name: &Ident, fields: &[(Ident, Expr)], span: Span) -> Val {
        let Some(id) = self.find_struct(name) else {
            for (_, v) in fields {
                let _ = self.gen_expr(v, None);
            }
            return self.err_val(span);
        };
        let sname = self.structs[id as usize].name.clone();
        let defs: Vec<(String, Ty, String)> = self.structs[id as usize]
            .fields
            .iter()
            .map(|f| (f.name.clone(), f.ty, f.cname.clone()))
            .collect();
        let mut given: Vec<Option<String>> = vec![None; defs.len()];
        let mut failed = false;
        for (fname, expr) in fields {
            match defs.iter().position(|d| d.0 == fname.name) {
                None => {
                    let mut d = Diagnostic::error(
                        E0624,
                        format!("struct `{}` has no field named `{}`", sname, fname.name),
                    )
                    .primary(fname.span, "unknown field");
                    if let Some(sg) = similar(&fname.name, defs.iter().map(|d| d.0.as_str())) {
                        d = d.suggest("a field with a similar name exists", fname.span, sg, false);
                    }
                    self.emit(d);
                    let _ = self.gen_expr(expr, None);
                    failed = true;
                }
                Some(i) => {
                    let v = self.gen_expr(expr, Some(defs[i].1));
                    if given[i].is_some() {
                        let d = Diagnostic::error(
                            E0625,
                            format!("field `{}` is given more than once", fname.name),
                        )
                        .primary(fname.span, "given again here");
                        self.emit(d);
                        failed = true;
                        continue;
                    }
                    self.check(&v, defs[i].1, Some(expr));
                    given[i] = Some(if v.c.is_empty() { "0".to_string() } else { v.c });
                }
            }
        }
        let missing: Vec<String> = defs
            .iter()
            .zip(&given)
            .filter(|(_, g)| g.is_none())
            .map(|(d, _)| format!("`{}`", d.0))
            .collect();
        if !missing.is_empty() && !failed {
            let d = Diagnostic::error(
                E0623,
                format!(
                    "missing {} in the initializer of `{}`: {}",
                    if missing.len() == 1 { "field" } else { "fields" },
                    sname,
                    missing.join(", ")
                ),
            )
            .primary(span, format!("missing {}", missing.join(", ")));
            self.emit(d);
            failed = true;
        }
        if failed {
            return self.err_val(span);
        }
        let c = if defs.is_empty() {
            format!("((nhT{}){{0}})", id)
        } else {
            let parts: Vec<String> = defs
                .iter()
                .zip(given)
                .map(|(d, g)| format!(".{} = {}", d.2, g.unwrap()))
                .collect();
            format!("((nhT{}){{{}}})", id, parts.join(", "))
        };
        Val {
            c,
            ty: Ty::Struct(id),
            span,
        }
    }

    /// `p.mover(...)`: metodo de uma struct.
    #[allow(clippy::too_many_arguments)]
    fn gen_user_method(
        &mut self,
        id: u16,
        r: Val,
        ri: PlaceInfo,
        recv: &Expr,
        name: &Ident,
        args: &[Expr],
        span: Span,
    ) -> Val {
        let sname = self.structs[id as usize].name.clone();
        let key = (id, name.name.clone());
        let Some(sig) = self.methods.get(&key) else {
            let mut d = Diagnostic::error(
                E0613,
                format!("no method named `{}` found for struct `{}`", name.name, sname),
            )
            .primary(name.span, "method not found");
            let cands: Vec<String> = self
                .methods
                .iter()
                .filter(|(k, s)| k.0 == id && s.self_kind.is_some())
                .map(|(k, _)| k.1.clone())
                .collect();
            if let Some(sg) = similar(&name.name, cands.iter().map(|s| s.as_str())) {
                d = d.suggest("a method with a similar name exists", name.span, sg, false);
            }
            self.emit(d);
            self.gen_args_only(args);
            return self.err_val(span);
        };
        let (params, ret, name_span, cname, kind) =
            (sig.params.clone(), sig.ret, sig.name_span, sig.cname.clone(), sig.self_kind);
        let Some(kind) = kind else {
            let d = Diagnostic::error(
                E0631,
                format!("`{}` is an associated function, not a method", name.name),
            )
            .primary(name.span, "it has no `自分` parameter")
            .secondary(name_span, "defined here")
            .help(format!("call it as `{}::{}(...)`", sname, name.name));
            self.emit(d);
            self.gen_args_only(args);
            return self.err_val(span);
        };
        let recv_val = match kind {
            SelfKind::Value => match r.ty {
                Ty::Ref(..) => Val {
                    c: format!("(*{})", r.c),
                    ty: Ty::Struct(id),
                    span: recv.span,
                },
                _ => r,
            },
            SelfKind::Ref | SelfKind::RefMut => {
                let m = kind == SelfKind::RefMut;
                match r.ty {
                    Ty::Ref(_, rm) => {
                        if m && !rm {
                            let d = Diagnostic::error(
                                E0401,
                                format!("cannot borrow `{}` as mutable, as it is behind a `&` reference", self.text(recv.span)),
                            )
                            .primary(recv.span, "this reference does not allow changes")
                            .secondary(name_span, "this method needs `&可変 自分`")
                            .help("take it as `&可変` (kahen, \"mutable\") instead of `&`");
                            self.emit(d);
                            self.gen_args_only(args);
                            return self.err_val(span);
                        }
                        r
                    }
                    _ => {
                        if m && !self.require_mutable(&ri, recv.span, "borrow as mutable") {
                            self.gen_args_only(args);
                            return self.err_val(span);
                        }
                        let base = if ri.is_place {
                            r.c.clone()
                        } else {
                            let t = self.tmp();
                            self.line(format!("{} {} = {};", r.ty.c(), t, r.c));
                            t
                        };
                        Val {
                            c: format!("(&{})", base),
                            ty: Ty::Ref(id, m),
                            span: recv.span,
                        }
                    }
                }
            }
        };
        self.call_common(&cname, &params, ret, name_span, vec![recv_val], args, span, name.span, "method")
    }

    // ---------------------------------------------------------------- metodos

    fn gen_method(&mut self, recv: &Expr, name: &Ident, args: &[Expr], span: Span) -> Val {
        const METHODS: &[(&str, &str, &str, &str)] = &[
            ("長さ", "nagasa", "length in characters", "nh_str_chars"),
            ("バイト数", "baitosuu", "length in bytes", "nh_str_bytes"),
        ];
        let (r, ri) = self.gen_place(recv);
        if let Ty::Struct(id) | Ty::Ref(id, _) = r.ty {
            return self.gen_user_method(id, r, ri, recv, name, args, span);
        }
        for a in args {
            let _ = self.gen_expr(a, None);
        }
        if r.ty == Ty::Err || r.ty == Ty::Never {
            return self.err_val(span);
        }
        let list: Vec<String> = METHODS
            .iter()
            .map(|m| format!("`{}` ({}, \"{}\")", m.0, m.1, m.2))
            .collect();
        if r.ty != Ty::Str {
            let d = Diagnostic::error(
                E0613,
                format!("no method named `{}` found for type {}", name.name, self.bt(r.ty)),
            )
            .primary(name.span, "method not found")
            .note(format!("only `文字列` (mojiretsu, \"text\") has methods for now: {}", list.join(", ")));
            self.emit(d);
            return self.err_val(span);
        }
        let Some(m) = METHODS.iter().find(|m| m.0 == name.name || m.1 == name.name) else {
            let mut d = Diagnostic::error(
                E0613,
                format!("no method named `{}` found for type {}", name.name, self.bt(Ty::Str)),
            )
            .primary(name.span, "method not found");
            let cands = METHODS.iter().flat_map(|m| [m.0, m.1]);
            if let Some(sg) = similar(&name.name, cands) {
                let sg = METHODS.iter().find(|m| m.1 == sg).map(|m| m.0.to_string()).unwrap_or(sg);
                d = d.suggest("a method with a similar name exists", name.span, sg, false);
            }
            d = d.note(format!("available methods: {}", list.join(", ")));
            self.emit(d);
            return self.err_val(span);
        };
        if name.name == m.1 && !self.handler.config.romaji {
            let d = Diagnostic::warning(
                W0001,
                format!("method name `{}` is written in latin letters", name.name),
            )
            .primary(name.span, format!("latin spelling of `{}` (\"{}\")", m.0, m.2))
            .note("pass `--romaji` to allow latin keywords")
            .suggest("use the canonical form", name.span, m.0, true);
            self.emit(d);
        }
        if !args.is_empty() {
            let d = Diagnostic::error(
                E0302,
                format!(
                    "this method takes 0 arguments but {} {} supplied",
                    plural(args.len(), "argument", "arguments"),
                    if args.len() == 1 { "was" } else { "were" }
                ),
            )
            .primary(name.span, "expected 0 arguments");
            self.emit(d);
            return self.err_val(span);
        }
        let t = self.tmp();
        let loc = self.loc(span);
        self.line(format!("int32_t {} = nh_len_i32({}({}), \"{}\");", t, m.3, r.c, loc));
        Val {
            c: t,
            ty: Ty::I32,
            span,
        }
    }

    // ----------------------------------------------------------------- macros

    fn gen_macro(&mut self, name: &str, name_span: Span, args: &[Expr], span: Span) -> Val {
        let Some((info, _)) = lookup_macro(name) else {
            let mut d = Diagnostic::error(E0502, format!("cannot find macro `{}!` in this scope", name))
                .primary(name_span, "not found");
            let cands = MACROS.iter().flat_map(|m| [m.jp, m.romaji]);
            if let Some(s) = similar(name, cands) {
                let s = MACROS.iter().find(|m| m.romaji == s).map(|m| m.jp.to_string()).unwrap_or(s);
                d = d.suggest("a macro with a similar name exists", name_span, s, false);
            }
            let list: Vec<String> = MACROS
                .iter()
                .map(|m| format!("`{}!` ({}, \"{}\")", m.jp, m.romaji, m.meaning))
                .collect();
            d = d.note(format!("available macros: {}", list.join(", ")));
            self.emit(d);
            return self.err_val(span);
        };
        if self.cur_const.is_some() {
            let d = Diagnostic::error(E0207, "macros are not allowed in constants")
                .primary(span, "macro call in a constant");
            self.emit(d);
            return self.err_val(span);
        }
        let kind = match info.jp {
            "表示" => MacroKind::Println,
            "出力" => MacroKind::Print,
            "異常終了" => MacroKind::Panic,
            "表明" => MacroKind::Assert,
            "整形" => MacroKind::Format,
            _ => MacroKind::Todo,
        };
        // 表明! confere a condicao primeiro
        let mut rest = args;
        let mut cond: Option<Val> = None;
        if kind == MacroKind::Assert {
            match args.first() {
                Some(c) => {
                    let v = self.gen_expr(c, Some(Ty::Bool));
                    if !compat(Ty::Bool, v.ty) {
                        self.cond_error(&v);
                    }
                    cond = Some(v);
                    rest = &args[1..];
                }
                None => {
                    let d = Diagnostic::error(E0501, "`表明!` needs a condition")
                        .primary(span, "missing condition")
                        .note("`表明!` (hyoumei!, \"assert!\") takes a condition, then an optional message");
                    self.emit(d);
                    return self.err_val(span);
                }
            }
        }

        // texto de formato
        let mut pieces: Vec<Piece> = Vec::new();
        let mut fmt_span = span;
        let mut fmt_args: &[Expr] = &[];
        if let Some(first) = rest.first() {
            match &first.kind {
                ExprKind::Str(s) => {
                    fmt_span = first.span;
                    fmt_args = &rest[1..];
                    match parse_format(s) {
                        Ok(p) => pieces = p,
                        Err(msg) => {
                            let d = Diagnostic::error(E0504, format!("invalid format string: {}", msg))
                                .primary(first.span, "invalid format string")
                                .note("use `{}` where a value goes, and `{{` / `}}` for literal braces");
                            self.emit(d);
                            return self.err_val(span);
                        }
                    }
                }
                _ => {
                    let d = Diagnostic::error(E0503, "format argument must be a string literal")
                        .primary(first.span, "expected a string literal")
                        .note("the first argument of `表示!` (hyouji!, \"println!\") is the text, with `{}` for each value")
                        .help("write it like this: `表示!(\"{}\", valor)` (hyouji!)");
                    self.emit(d);
                    return self.err_val(span);
                }
            }
        }
        let holes = pieces.iter().filter(|p| matches!(p, Piece::Arg)).count();
        if holes != fmt_args.len() {
            let mut d = Diagnostic::error(
                E0501,
                format!(
                    "{} in the format string, but {}",
                    plural(holes, "positional argument", "positional arguments"),
                    if fmt_args.is_empty() {
                        "no arguments were given".to_string()
                    } else {
                        format!("{} {} given", fmt_args.len(), if fmt_args.len() == 1 { "argument was" } else { "arguments were" })
                    }
                ),
            );
            if fmt_args.len() > holes {
                d = d.secondary(fmt_span, format!("{} here", plural(holes, "`{}`", "`{}`")));
                for a in &fmt_args[holes..] {
                    d = d.primary(a.span, "argument never used");
                }
            } else {
                d = d.primary(fmt_span, format!("{} here", plural(holes, "`{}`", "`{}`")));
                for a in fmt_args {
                    d = d.secondary(a.span, "given here");
                }
            }
            self.emit(d);
            return self.err_val(span);
        }

        // valores, da esquerda para a direita
        let mut vals: Vec<Val> = Vec::new();
        for a in fmt_args {
            let v = self.gen_expr(a, None);
            if matches!(v.ty, Ty::Struct(_) | Ty::Ref(..)) {
                let d = Diagnostic::error(
                    E0629,
                    format!("{} cannot be shown with `{{}}`", self.bt(v.ty)),
                )
                .primary(a.span, "structs have no text form yet")
                .help("show the fields one by one, for example `表示!(\"{}\", p.x)` (hyouji!)");
                self.emit(d);
            } else if !v.ty.has_value() && v.ty != Ty::Err && v.ty != Ty::Never {
                let d = Diagnostic::error(E0301, "this value cannot be printed")
                    .primary(a.span, format!("has type {}, which has no value to show", self.bt(v.ty)));
                self.emit(d);
            }
            vals.push(v);
        }

        // escrita
        let fmt_buf = if kind == MacroKind::Format { Some(self.tmp()) } else { None };
        let (sink, newline) = match kind {
            MacroKind::Println => ("&nh_o_out".to_string(), true),
            MacroKind::Print => ("&nh_o_out".to_string(), false),
            MacroKind::Format => (format!("&{}", fmt_buf.as_ref().unwrap()), false),
            _ => ("&nh_o_err".to_string(), false),
        };
        let sink = sink.as_str();
        let mut code = String::new();
        let mut pending = String::new();
        let mut vi = 0;
        let mut ops: Vec<String> = Vec::new();
        let flush_lit = |pending: &mut String, ops: &mut Vec<String>, sink: &str| {
            if !pending.is_empty() {
                ops.push(format!(
                    "nh_w_lit({}, \"{}\", {});",
                    sink,
                    c_escape(pending),
                    pending.len()
                ));
                pending.clear();
            }
        };
        match kind {
            MacroKind::Panic if pieces.is_empty() => pending.push_str("explicit panic"),
            MacroKind::Assert if pieces.is_empty() => {
                pending.push_str("assertion failed");
            }
            MacroKind::Todo => pending.push_str(if pieces.is_empty() { "not yet implemented" } else { "not yet implemented: " }),
            _ => {}
        }
        for p in &pieces {
            match p {
                Piece::Lit(s) => pending.push_str(s),
                Piece::Arg => {
                    flush_lit(&mut pending, &mut ops, sink);
                    let v = &vals[vi];
                    vi += 1;
                    let f = match v.ty {
                        Ty::I32 => "i32",
                        Ty::U32 => "u32",
                        Ty::F64 => "f64",
                        Ty::Bool => "bool",
                        Ty::Char => "char",
                        Ty::Str => "str",
                        _ => continue,
                    };
                    ops.push(format!("nh_w_{}({}, {});", f, sink, v.c));
                }
            }
        }
        if newline {
            pending.push('\n');
        }
        flush_lit(&mut pending, &mut ops, sink);
        for o in &ops {
            code.push_str(o);
            code.push('\n');
        }

        let loc = self.loc(span);
        match kind {
            MacroKind::Format => {
                let b = fmt_buf.unwrap();
                let t = self.tmp();
                self.line(format!("nh_out {} = nh_out_buf();", b));
                self.buf.push_str(&code);
                self.line(format!("nh_str {} = nh_out_take(&{});", t, b));
                Val {
                    c: t,
                    ty: Ty::Str,
                    span,
                }
            }
            MacroKind::Println | MacroKind::Print => {
                self.buf.push_str(&code);
                self.unit(span)
            }
            MacroKind::Assert => {
                let c = cond.map(|v| v.c).unwrap_or_else(|| "1".to_string());
                self.line(format!("if (!({})) {{", c));
                self.line(format!("nh_panic_begin(\"{}\");", loc));
                self.buf.push_str(&code);
                self.line("nh_panic_end();");
                self.line("}");
                self.unit(span)
            }
            _ => {
                self.line(format!("nh_panic_begin(\"{}\");", loc));
                self.buf.push_str(&code);
                self.line("nh_panic_end();");
                Val {
                    c: String::new(),
                    ty: Ty::Never,
                    span,
                }
            }
        }
    }

    // --------------------------------------------------------- if, for, cond

    fn cond_error(&mut self, v: &Val) {
        let d = Diagnostic::error(E0303, "mismatched types")
            .primary(v.span, format!("expected {}, found {}", self.bt(Ty::Bool), self.bt(v.ty)))
            .note("a condition must be `真偽` (shingi, \"boolean\"): a comparison like `x > 0`, or `真` / `偽` (shin / gi, \"true\" / \"false\")");
        self.emit(d);
    }

    fn gen_cond(&mut self, e: &Expr) -> String {
        let v = self.gen_expr(e, Some(Ty::Bool));
        if !compat(Ty::Bool, v.ty) {
            self.cond_error(&v);
        }
        if v.c.is_empty() {
            "0".to_string()
        } else {
            v.c
        }
    }

    fn gen_if(&mut self, cond: &Expr, then: &Block, else_: Option<&Expr>, expect: Option<Ty>, span: Span) -> Val {
        let c = self.gen_cond(cond);
        let Some(el) = else_ else {
            let (ts, tr) = self.capture(|s| s.gen_block(then, None));
            if tr.ty.has_value() {
                let d = Diagnostic::error(E0301, "`もし` may be missing an `他` clause")
                    .primary(tr.span, format!("expected `()`, found {}", self.bt(tr.ty)))
                    .secondary(cond.span, "this `もし` has no `他` (hoka, \"else\")")
                    .note("without an `他` block, the `もし` cannot produce a value");
                self.emit(d);
            }
            self.line(format!("if ({}) {{", c));
            self.buf.push_str(&ts);
            self.line("}");
            return self.unit(span);
        };
        let (ts, tr) = self.capture(|s| s.gen_block(then, expect));
        let exp2 = expect.or(if tr.ty.has_value() { Some(tr.ty) } else { None });
        let (es, er) = self.capture(|s| match &el.kind {
            ExprKind::Block(b) => s.gen_block(b, exp2),
            _ => s.gen_expr(el, exp2),
        });
        let ty = if tr.ty == Ty::Never {
            er.ty
        } else if er.ty == Ty::Never {
            tr.ty
        } else if tr.ty == Ty::Err {
            er.ty
        } else if er.ty == Ty::Err {
            tr.ty
        } else if tr.ty == er.ty {
            tr.ty
        } else {
            let d = Diagnostic::error(E0301, "`もし` and `他` have incompatible types")
                .secondary(tr.span, format!("expected because of this: {}", self.bt(tr.ty)))
                .primary(er.span, format!("expected {}, found {}", self.bt(tr.ty), self.bt(er.ty)))
                .note("both branches of a `もし` / `他` must produce the same type");
            self.emit(d);
            Ty::Err
        };
        let dest = if ty.has_value() {
            let t = self.tmp();
            self.line(format!("{} {};", ty.c(), t));
            Some(t)
        } else {
            None
        };
        self.line(format!("if ({}) {{", c));
        self.buf.push_str(&ts);
        if let (Some(t), true) = (&dest, tr.ty.has_value()) {
            self.line(format!("{} = {};", t, tr.c));
        }
        self.line("} else {");
        self.buf.push_str(&es);
        if let (Some(t), true) = (&dest, er.ty.has_value()) {
            self.line(format!("{} = {};", t, er.c));
        }
        self.line("}");
        match dest {
            Some(t) => Val { c: t, ty, span },
            None => Val {
                c: if ty == Ty::Err { "0".to_string() } else { String::new() },
                ty,
                span,
            },
        }
    }

    fn gen_for(&mut self, var: &Ident, iter: &Expr, body: &Block, span: Span) -> Val {
        let ExprKind::Range {
            start,
            end,
            inclusive,
        } = &iter.kind
        else {
            let d = Diagnostic::error(E0999, "`各` only loops over ranges for now")
                .primary(iter.span, "expected a range like `0..10` or `1..=10`")
                .note("arrays and other collections arrive in stage 1");
            self.emit(d);
            return self.err_val(span);
        };
        let (s, e) = if is_lit(start) && !is_lit(end) {
            let e = self.gen_expr(end, None);
            let s = self.gen_expr(start, Some(e.ty));
            (s, e)
        } else {
            let s = self.gen_expr(start, None);
            let e = self.gen_expr(end, Some(s.ty));
            (s, e)
        };
        let mut ty = s.ty;
        if s.ty != Ty::Err && !s.ty.is_int() {
            let d = Diagnostic::error(E0304, format!("a range needs integers, found {}", self.bt(s.ty)))
                .primary(s.span, format!("has type {}", self.bt(s.ty)));
            self.emit(d);
            ty = Ty::Err;
        } else if e.ty != Ty::Err && s.ty != Ty::Err && e.ty != s.ty {
            self.mismatch(&e, s.ty, Some(end), Some((s.span, format!("this has type {}", self.bt(s.ty)))));
            ty = Ty::Err;
        }
        self.push_scope();
        let vcname = self.declare(var, if ty == Ty::Err { Ty::I32 } else { ty }, false, None);
        self.loop_stack.push(false);
        let (bs, _) = self.capture(|g| g.gen_block(body, None));
        self.loop_stack.pop();
        self.pop_scope();
        if ty == Ty::Err {
            return self.unit(span);
        }
        let (cur, end_t) = (self.tmp(), self.tmp());
        self.line(format!("{} {} = {};", ty.c(), cur, s.c));
        self.line(format!("{} {} = {};", ty.c(), end_t, e.c));
        if *inclusive {
            let go = self.tmp();
            self.line(format!(
                "for (int {go} = {cur} <= {end_t}; {go}; {go} = ({cur} != {end_t}), {cur} += {go}) {{"
            ));
        } else {
            self.line(format!("for (; {cur} < {end_t}; {cur}++) {{"));
        }
        self.line(format!("{} {} = {};", ty.c(), vcname, cur));
        self.buf.push_str(&bs);
        self.line("}");
        self.unit(span)
    }
}

/// Indenta o C pelas chaves (as strings nao tem `{` nem `}` literais, porque
/// `c_escape` troca tudo que nao e letra, digito ou espaco por `\ooo`).
fn indent(src: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for line in src.lines() {
        let t = line.trim_start();
        if t.is_empty() {
            out.push('\n');
            continue;
        }
        let in_runtime = false;
        let _ = in_runtime;
        if t.starts_with('}') {
            depth = depth.saturating_sub(1);
        }
        out.push_str(&"  ".repeat(depth));
        out.push_str(t);
        out.push('\n');
        let opens = t.matches('{').count();
        let closes = t.matches('}').count();
        let net = opens as isize - closes as isize + if t.starts_with('}') { 1 } else { 0 };
        if net > 0 {
            depth += net as usize;
        } else if net < 0 {
            depth = depth.saturating_sub((-net) as usize);
        }
    }
    out
}

const RUNTIME: &str = r#"#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <inttypes.h>
#ifdef _WIN32
#include <io.h>
#include <fcntl.h>
#include <windows.h>
#endif
#if defined(__GNUC__)
#define NH_NORETURN __attribute__((noreturn))
#else
#define NH_NORETURN
#endif

typedef struct { const char *p; size_t n; } nh_str;
typedef struct { FILE *f; char *buf; size_t len, cap; } nh_out;
static nh_out nh_o_out, nh_o_err;

static void nh_setup(void) {
#ifdef _WIN32
SetConsoleOutputCP(65001);
_setmode(_fileno(stdout), _O_BINARY);
_setmode(_fileno(stderr), _O_BINARY);
#endif
nh_o_out.f = stdout;
nh_o_err.f = stderr;
}

static void nh_panic_begin(const char *loc) {
fflush(stdout);
fprintf(stderr, "thread 'main' panicked at %s:\n", loc);
}
NH_NORETURN static void nh_panic_end_(void) {
fputc('\n', stderr);
fflush(stderr);
exit(101);
}
#define nh_panic_end() nh_panic_end_()
NH_NORETURN static void nh_panic_msg(const char *loc, const char *msg) {
nh_panic_begin(loc);
fputs(msg, stderr);
nh_panic_end_();
}

#define NH_ARITH(T, N, MINV) \
static T nh_add_##N(T a, T b, const char *l) { T r; if (__builtin_add_overflow(a, b, &r)) nh_panic_msg(l, "attempt to add with overflow"); return r; } \
static T nh_sub_##N(T a, T b, const char *l) { T r; if (__builtin_sub_overflow(a, b, &r)) nh_panic_msg(l, "attempt to subtract with overflow"); return r; } \
static T nh_mul_##N(T a, T b, const char *l) { T r; if (__builtin_mul_overflow(a, b, &r)) nh_panic_msg(l, "attempt to multiply with overflow"); return r; } \
static T nh_div_##N(T a, T b, const char *l) { \
if (b == 0) nh_panic_msg(l, "attempt to divide by zero"); \
if (MINV != 0 && a == MINV && b == (T)-1) nh_panic_msg(l, "attempt to divide with overflow"); \
return a / b; } \
static T nh_rem_##N(T a, T b, const char *l) { \
if (b == 0) nh_panic_msg(l, "attempt to calculate the remainder with a divisor of zero"); \
if (MINV != 0 && a == MINV && b == (T)-1) nh_panic_msg(l, "attempt to calculate the remainder with overflow"); \
return a % b; }
NH_ARITH(int32_t, i32, INT32_MIN)
NH_ARITH(uint32_t, u32, 0)

static int32_t nh_neg_i32(int32_t a, const char *l) {
if (a == INT32_MIN) nh_panic_msg(l, "attempt to negate with overflow");
return -a;
}

static int32_t nh_f2i32(double x) {
if (x != x) return 0;
if (x <= -2147483648.0) return INT32_MIN;
if (x >= 2147483647.0) return INT32_MAX;
return (int32_t)x;
}
static uint32_t nh_f2u32(double x) {
if (x != x || x <= 0.0) return 0;
if (x >= 4294967295.0) return UINT32_MAX;
return (uint32_t)x;
}

/* ---- output: a file (stdout, stderr) or a growing buffer (for the text of format) ---- */
static void nh_oom(void) {
fputs("error: out of memory\n", stderr);
exit(101);
}
static void nh_put(nh_out *o, const char *s, size_t n) {
if (o->f) { fwrite(s, 1, n, o->f); return; }
if (o->len + n > o->cap) {
size_t cap = o->cap ? o->cap : 32;
while (cap < o->len + n) cap *= 2;
o->buf = (char *)realloc(o->buf, cap);
if (!o->buf) nh_oom();
o->cap = cap;
}
if (n) memcpy(o->buf + o->len, s, n);
o->len += n;
}
static nh_out nh_out_buf(void) { nh_out o; o.f = NULL; o.buf = NULL; o.len = 0; o.cap = 0; return o; }
/* the buffer is handed over to the text and never freed (no ownership until stage 4) */
static nh_str nh_out_take(nh_out *o) {
nh_str s;
s.p = o->buf ? o->buf : "";
s.n = o->len;
return s;
}

static void nh_w_lit(nh_out *o, const char *s, size_t n) { nh_put(o, s, n); }
static void nh_w_str(nh_out *o, nh_str s) { nh_put(o, s.p, s.n); }
static void nh_w_i32(nh_out *o, int32_t v) { char b[16]; int n = snprintf(b, sizeof b, "%" PRId32, v); nh_put(o, b, (size_t)n); }
static void nh_w_u32(nh_out *o, uint32_t v) { char b[16]; int n = snprintf(b, sizeof b, "%" PRIu32, v); nh_put(o, b, (size_t)n); }
static void nh_w_bool(nh_out *o, int v) {
if (v) nh_put(o, "\xe7\x9c\x9f (shin)", 10); else nh_put(o, "\xe5\x81\xbd (gi)", 8);
}
static void nh_w_char(nh_out *o, uint32_t c) {
char b[4];
size_t n;
if (c < 0x80) { b[0] = (char)c; n = 1; }
else if (c < 0x800) { b[0] = (char)(0xC0 | (c >> 6)); b[1] = (char)(0x80 | (c & 0x3F)); n = 2; }
else if (c < 0x10000) { b[0] = (char)(0xE0 | (c >> 12)); b[1] = (char)(0x80 | ((c >> 6) & 0x3F)); b[2] = (char)(0x80 | (c & 0x3F)); n = 3; }
else { b[0] = (char)(0xF0 | (c >> 18)); b[1] = (char)(0x80 | ((c >> 12) & 0x3F)); b[2] = (char)(0x80 | ((c >> 6) & 0x3F)); b[3] = (char)(0x80 | (c & 0x3F)); n = 4; }
nh_put(o, b, n);
}
/* shortest text that reads back as the same number, never in scientific notation (like Rust) */
static void nh_w_f64(nh_out *o, double v) {
char buf[48], digits[32], out[400];
int p, n = 0, i, neg = 0, e10, pos;
size_t k = 0;
char *s;
if (v != v) { nh_put(o, "NaN", 3); return; }
if (v == INFINITY) { nh_put(o, "inf", 3); return; }
if (v == -INFINITY) { nh_put(o, "-inf", 4); return; }
for (p = 0; p < 17; p++) {
snprintf(buf, sizeof buf, "%.*e", p, v);
if (strtod(buf, NULL) == v) break;
}
s = buf;
if (*s == '-') { neg = 1; s++; }
while (*s && *s != 'e') { if (*s != '.') digits[n++] = *s; s++; }
e10 = atoi(s + 1);
while (n > 1 && digits[n - 1] == '0') n--;
if (neg) out[k++] = '-';
if (digits[0] == '0') { out[k++] = '0'; nh_put(o, out, k); return; }
pos = e10 + 1;
if (pos <= 0) {
out[k++] = '0'; out[k++] = '.';
for (i = 0; i < -pos; i++) out[k++] = '0';
memcpy(out + k, digits, (size_t)n); k += (size_t)n;
} else if (pos >= n) {
memcpy(out + k, digits, (size_t)n); k += (size_t)n;
for (i = 0; i < pos - n; i++) out[k++] = '0';
} else {
memcpy(out + k, digits, (size_t)pos); k += (size_t)pos;
out[k++] = '.';
memcpy(out + k, digits + pos, (size_t)(n - pos)); k += (size_t)(n - pos);
}
nh_put(o, out, k);
}

/* ---- text ---- */
static int nh_str_cmp(nh_str a, nh_str b) {
size_t m = a.n < b.n ? a.n : b.n;
int c = m ? memcmp(a.p, b.p, m) : 0;
if (c) return c < 0 ? -1 : 1;
return a.n < b.n ? -1 : (a.n > b.n ? 1 : 0);
}
static nh_str nh_str_concat(nh_str a, nh_str b) {
nh_str r;
char *m = (char *)malloc(a.n + b.n + 1);
if (!m) nh_oom();
if (a.n) memcpy(m, a.p, a.n);
if (b.n) memcpy(m + a.n, b.p, b.n);
m[a.n + b.n] = 0;
r.p = m;
r.n = a.n + b.n;
return r;
}
static size_t nh_str_bytes(nh_str s) { return s.n; }
static size_t nh_str_chars(nh_str s) {
size_t i, n = 0;
for (i = 0; i < s.n; i++) if ((((unsigned char)s.p[i]) & 0xC0) != 0x80) n++;
return n;
}
static int32_t nh_len_i32(size_t n, const char *l) {
if (n > (size_t)INT32_MAX) nh_panic_msg(l, "the length does not fit in a 32-bit integer");
return (int32_t)n;
}
"#;
