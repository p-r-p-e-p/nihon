//! Classes de caracteres: o que pode aparecer num nome e o que e "zenkaku".

pub fn is_hiragana(c: char) -> bool {
    matches!(c as u32, 0x3041..=0x3096 | 0x309D | 0x309E)
}

pub fn is_katakana(c: char) -> bool {
    // inclui o traco de vogal longa `ー` (U+30FC) e as marcas de repeticao
    matches!(c as u32, 0x30A1..=0x30FA | 0x30FC..=0x30FE | 0x31F0..=0x31FF)
}

/// Kanji (ideogramas CJK), incluindo `々` `〆` `〇`.
pub fn is_han(c: char) -> bool {
    matches!(
        c as u32,
        0x3005..=0x3007 | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x3134F
    )
}

/// Katakana de meia largura (`ﾊﾝｶｸ`). Vale em nomes, mas gera aviso.
pub fn is_half_kana(c: char) -> bool {
    matches!(c as u32, 0xFF66..=0xFF9F)
}

/// Letras latinas: ASCII mais as acentuadas (Latin-1 e Latin Extended A/B).
pub fn is_latin_letter(c: char) -> bool {
    if c.is_ascii_alphabetic() {
        return true;
    }
    let cp = c as u32;
    (0xC0..=0x24F).contains(&cp) && cp != 0xD7 && cp != 0xF7
}

/// "Parece latim" para decidir se falta um espaco entre palavra-chave e nome:
/// letras latinas, digitos ASCII e `_`.
pub fn latinish(c: char) -> bool {
    is_latin_letter(c) || c.is_ascii_digit() || c == '_'
}

pub fn is_ident_start(c: char) -> bool {
    is_hiragana(c) || is_katakana(c) || is_han(c) || is_latin_letter(c) || c == '_' || is_half_kana(c)
}

pub fn is_ident_continue(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}

/// Caractere em largura total (ou pontuacao japonesa) e seu equivalente ASCII.
#[derive(Debug, Clone, Copy)]
pub struct Zenkaku {
    pub ascii: Option<char>,
    /// O equivalente e certo o bastante para um `fix` automatico aplicar.
    pub safe: bool,
}

pub fn zenkaku(c: char) -> Option<Zenkaku> {
    let cp = c as u32;
    match cp {
        0xFF01..=0xFF5E => Some(Zenkaku {
            ascii: char::from_u32(cp - 0xFEE0),
            safe: true,
        }),
        0x3000 => Some(Zenkaku {
            ascii: Some(' '),
            safe: true,
        }),
        0x3001 => Some(Zenkaku {
            ascii: Some(','),
            safe: false,
        }),
        0x3002 | 0x3008..=0x3011 | 0x3014..=0x301B | 0x30FB => Some(Zenkaku {
            ascii: None,
            safe: false,
        }),
        _ => None,
    }
}

pub fn ascii_name(c: char) -> &'static str {
    match c {
        ';' => "semicolon",
        ':' => "colon",
        ',' => "comma",
        '.' => "period",
        '(' => "left parenthesis",
        ')' => "right parenthesis",
        '{' => "left brace",
        '}' => "right brace",
        '[' => "left bracket",
        ']' => "right bracket",
        '=' => "equals sign",
        '+' => "plus sign",
        '-' => "minus sign",
        '*' => "asterisk",
        '/' => "slash",
        '\\' => "backslash",
        '<' => "less-than sign",
        '>' => "greater-than sign",
        '!' => "exclamation mark",
        '?' => "question mark",
        '&' => "ampersand",
        '|' => "vertical bar",
        '"' => "quotation mark",
        '\'' => "apostrophe",
        '%' => "percent sign",
        '_' => "underscore",
        '~' => "tilde",
        '^' => "caret",
        '#' => "number sign",
        '$' => "dollar sign",
        '@' => "at sign",
        '`' => "grave accent",
        _ => "character",
    }
}

fn voiced(c: char) -> Option<char> {
    if "カキクケコサシスセソタチツテトハヒフヘホ".contains(c) {
        char::from_u32(c as u32 + 1)
    } else if c == 'ウ' {
        Some('ヴ')
    } else {
        None
    }
}

fn semi_voiced(c: char) -> Option<char> {
    if "ハヒフヘホ".contains(c) {
        char::from_u32(c as u32 + 2)
    } else {
        None
    }
}

/// Converte katakana de meia largura para largura normal (`ﾊﾝｶｸ` -> `ハンカク`),
/// juntando as marcas de dakuten. `None` se alguma marca nao tiver como juntar.
pub fn half_to_full(text: &str) -> Option<String> {
    const FULL: &str = "ヲァィゥェォャュョッーアイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワン";
    let map: Vec<char> = FULL.chars().collect();
    let mut out = String::new();
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        let cp = c as u32;
        if (0xFF66..=0xFF9D).contains(&cp) {
            let mut full = map[(cp - 0xFF66) as usize];
            match it.peek() {
                Some('\u{FF9E}') => {
                    full = voiced(full)?;
                    it.next();
                }
                Some('\u{FF9F}') => {
                    full = semi_voiced(full)?;
                    it.next();
                }
                _ => {}
            }
            out.push(full);
        } else if cp == 0xFF9E || cp == 0xFF9F {
            return None;
        } else {
            out.push(c);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes() {
        assert!(is_han('関') && is_han('々'));
        assert!(is_hiragana('も') && is_katakana('ー') && is_katakana('コ'));
        assert!(is_latin_letter('ç') && is_latin_letter('A') && !is_latin_letter('×'));
        assert!(is_ident_start('_') && !is_ident_start('1') && is_ident_continue('1'));
        assert!(!is_ident_start('；') && !is_ident_start('@'));
    }

    #[test]
    fn zenkaku_maps_to_ascii() {
        assert_eq!(zenkaku('；').unwrap().ascii, Some(';'));
        assert_eq!(zenkaku('１').unwrap().ascii, Some('1'));
        assert_eq!(zenkaku('Ａ').unwrap().ascii, Some('A'));
        assert_eq!(zenkaku('\u{3000}').unwrap().ascii, Some(' '));
        assert!(zenkaku('。').unwrap().ascii.is_none());
        assert!(zenkaku('a').is_none());
    }

    #[test]
    fn half_width_conversion() {
        assert_eq!(half_to_full("ﾊﾝｶｸ").as_deref(), Some("ハンカク"));
        assert_eq!(half_to_full("ｶﾞｷﾞ").as_deref(), Some("ガギ"));
        assert_eq!(half_to_full("ﾊﾟｰﾄ").as_deref(), Some("パート"));
        assert_eq!(half_to_full("ｳﾞ").as_deref(), Some("ヴ"));
        assert_eq!(half_to_full("ｱﾞ"), None);
        assert_eq!("ヲァィゥェォャュョッーアイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワン".chars().count(), 56);
    }
}
