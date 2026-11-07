//! Tabela de palavras reservadas e macros: forma canonica em japones, o
//! apelido em romaji (alias em latim) e o significado em ingles, usado nas
//! mensagens do compilador.

use super::chars::{is_hiragana, latinish};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kw {
    Fn,
    Let,
    Mut,
    Const,
    If,
    Else,
    ElseIf,
    While,
    Loop,
    For,
    In,
    Break,
    Continue,
    Return,
    Match,
    True,
    False,
    Struct,
    Enum,
    Impl,
    Trait,
    Pub,
    Use,
    SelfValue,
    SelfType,
    Unsafe,
    Extern,
    As,
}

pub struct KwInfo {
    pub kw: Kw,
    pub jp: &'static str,
    pub romaji: &'static str,
    pub meaning: &'static str,
}

const fn kw(kw: Kw, jp: &'static str, romaji: &'static str, meaning: &'static str) -> KwInfo {
    KwInfo {
        kw,
        jp,
        romaji,
        meaning,
    }
}

pub const KEYWORDS: &[KwInfo] = &[
    kw(Kw::Fn, "関数", "kansuu", "function"),
    kw(Kw::Let, "束縛", "sokubaku", "let"),
    kw(Kw::Mut, "可変", "kahen", "mutable"),
    kw(Kw::Const, "定数", "teisuu", "constant"),
    kw(Kw::If, "もし", "moshi", "if"),
    kw(Kw::Else, "他", "hoka", "else"),
    kw(Kw::ElseIf, "他もし", "hokamoshi", "else if"),
    kw(Kw::While, "間", "aida", "while"),
    kw(Kw::Loop, "無限", "mugen", "loop"),
    kw(Kw::For, "各", "kaku", "for each"),
    kw(Kw::In, "中", "naka", "inside"),
    kw(Kw::Break, "中断", "chuudan", "break"),
    kw(Kw::Continue, "続行", "zokkou", "continue"),
    kw(Kw::Return, "返却", "henkyaku", "return"),
    kw(Kw::Match, "照合", "shougou", "match"),
    kw(Kw::True, "真", "shin", "true"),
    kw(Kw::False, "偽", "gi", "false"),
    kw(Kw::Struct, "構造体", "kouzoutai", "struct"),
    kw(Kw::Enum, "列挙型", "rekkyogata", "enum"),
    kw(Kw::Impl, "実装", "jissou", "implementation"),
    kw(Kw::Trait, "特性", "tokusei", "trait"),
    kw(Kw::Pub, "公開", "koukai", "public"),
    kw(Kw::Use, "使用", "shiyou", "use"),
    kw(Kw::SelfValue, "自分", "jibun", "self"),
    kw(Kw::SelfType, "自身", "jishin", "Self"),
    kw(Kw::Unsafe, "危険", "kiken", "unsafe"),
    kw(Kw::Extern, "外部", "gaibu", "external"),
    kw(Kw::As, "として", "toshite", "as"),
];

pub struct MacroInfo {
    pub jp: &'static str,
    pub romaji: &'static str,
    pub meaning: &'static str,
}

pub const MACROS: &[MacroInfo] = &[
    MacroInfo { jp: "表示", romaji: "hyouji", meaning: "println" },
    MacroInfo { jp: "出力", romaji: "shutsuryoku", meaning: "print" },
    MacroInfo { jp: "異常終了", romaji: "ijoushuuryou", meaning: "panic" },
    MacroInfo { jp: "表明", romaji: "hyoumei", meaning: "assert" },
    MacroInfo { jp: "整形", romaji: "seikei", meaning: "format" },
    MacroInfo { jp: "未実装", romaji: "mijissou", meaning: "todo" },
];

/// Palavra-chave exata. O `bool` diz se veio escrita em romaji.
pub fn lookup_keyword(text: &str) -> Option<(&'static KwInfo, bool)> {
    for info in KEYWORDS {
        if text == info.jp {
            return Some((info, false));
        }
        if text == info.romaji {
            return Some((info, true));
        }
    }
    None
}

pub fn lookup_macro(text: &str) -> Option<(&'static MacroInfo, bool)> {
    for m in MACROS {
        if text == m.jp {
            return Some((m, false));
        }
        if text == m.romaji {
            return Some((m, true));
        }
    }
    None
}

/// Falta um espaco entre a palavra-chave e o que vem colado depois?
///
/// - latim com latim e uma palavra so (`moshix` e um nome comum);
/// - latim com japones (ou o contrario) quase sempre e espaco esquecido:
///   `もしx`, `返却1`, `moshi値`;
/// - japones com japones so conta se a palavra-chave termina em hiragana e o
///   resto nao (`もし値`): palavras de kanji seguidas de kanji ou de okurigana
///   (`中心`, `他人`, `無限ループ`) sao nomes normais.
pub fn missing_space(kw_last: char, rest_first: char) -> bool {
    let kw_latin = latinish(kw_last);
    let rest_latin = latinish(rest_first);
    if kw_latin != rest_latin {
        return true;
    }
    if kw_latin {
        return false;
    }
    is_hiragana(kw_last) && !is_hiragana(rest_first)
}

/// Se `text` comeca por uma palavra-chave e tem algo colado depois que parece
/// um espaco esquecido, devolve (palavra, veio em romaji, bytes da palavra).
pub fn split_keyword_prefix(text: &str) -> Option<(&'static KwInfo, bool, usize)> {
    let mut best: Option<(&'static KwInfo, bool, usize)> = None;
    for info in KEYWORDS {
        for (form, latin) in [(info.jp, false), (info.romaji, true)] {
            if text.len() > form.len()
                && text.starts_with(form)
                && best.map_or(true, |b| form.len() > b.2)
            {
                best = Some((info, latin, form.len()));
            }
        }
    }
    let (info, latin, len) = best?;
    let last = text[..len].chars().last()?;
    let first = text[len..].chars().next()?;
    if missing_space(last, first) {
        Some((info, latin, len))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_has_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for k in KEYWORDS {
            assert!(seen.insert(k.jp), "duplicado: {}", k.jp);
            assert!(seen.insert(k.romaji), "duplicado: {}", k.romaji);
        }
        for m in MACROS {
            assert!(seen.insert(m.jp), "duplicado: {}", m.jp);
            assert!(seen.insert(m.romaji), "duplicado: {}", m.romaji);
        }
    }

    #[test]
    fn exact_lookup() {
        assert_eq!(lookup_keyword("関数").map(|(i, l)| (i.kw, l)), Some((Kw::Fn, false)));
        assert_eq!(lookup_keyword("kansuu").map(|(i, l)| (i.kw, l)), Some((Kw::Fn, true)));
        assert!(lookup_keyword("主").is_none());
        assert!(lookup_keyword("中心").is_none());
    }

    #[test]
    fn missing_space_rules() {
        // falta espaco
        assert!(split_keyword_prefix("もしx").is_some());
        assert!(split_keyword_prefix("返却1").is_some());
        assert!(split_keyword_prefix("もし値").is_some());
        assert!(split_keyword_prefix("他もしx").is_some());
        assert!(split_keyword_prefix("moshi値").is_some());
        // nomes normais
        assert!(split_keyword_prefix("中心").is_none());
        assert!(split_keyword_prefix("他人").is_none());
        assert!(split_keyword_prefix("無限ループ").is_none());
        assert!(split_keyword_prefix("もしも").is_none());
        assert!(split_keyword_prefix("moshix").is_none());
        assert!(split_keyword_prefix("moshi1").is_none());
    }

    #[test]
    fn longest_prefix_wins() {
        let (info, _, len) = split_keyword_prefix("他もしx").unwrap();
        assert_eq!(info.kw, Kw::ElseIf);
        assert_eq!(len, "他もし".len());
    }
}
