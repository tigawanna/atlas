use tantivy::tokenizer::{BoxTokenStream, Token, TokenStream, Tokenizer};
use unicode_normalization::UnicodeNormalization;

pub fn strip_diacritics(text: &str) -> String {
    text.nfd()
        .filter(
            |c| !matches!(c.len_utf8(), _ if unicode_normalization::char::is_combining_mark(*c)),
        )
        .nfc()
        .collect()
}

pub fn detect_lang(text: &str, hint: Option<&atlas_core::Lang>) -> Option<atlas_core::Lang> {
    if let Some(h) = hint {
        return Some(h.clone());
    }
    let info = whatlang::detect(text)?;
    let lang = match info.lang() {
        whatlang::Lang::Eng => atlas_core::Lang::En,
        whatlang::Lang::Fra => atlas_core::Lang::Fr,
        whatlang::Lang::Ara => atlas_core::Lang::Ar,
        other => atlas_core::Lang::Other(other.code().to_string()),
    };
    Some(lang)
}

fn normalize_arabic_alef(c: char) -> char {
    match c {
        'أ' | 'إ' | 'آ' | 'ٱ' => 'ا',
        other => other,
    }
}

fn is_arabic_tashkeel(c: char) -> bool {
    matches!(c, '\u{064B}'..='\u{065F}' | '\u{0610}'..='\u{061A}' | '\u{0670}')
}

fn strip_arabic(text: &str) -> String {
    text.chars()
        .filter(|c| !is_arabic_tashkeel(*c))
        .map(normalize_arabic_alef)
        .collect()
}

fn strip_french_elision(token: &str) -> &str {
    for prefix in &["l'", "d'", "s'", "n'", "L'", "D'", "S'", "N'"] {
        if let Some(rest) = token.strip_prefix(prefix) {
            return rest;
        }
    }
    token
}

fn tokenize_text(text: &str) -> Vec<String> {
    let normalized: String = text.nfc().collect();
    let lowered = normalized.to_lowercase();
    lowered
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn apply_arabic_rules(tokens: Vec<String>) -> Vec<String> {
    tokens.into_iter().map(|t| strip_arabic(&t)).collect()
}

fn apply_french_rules(tokens: Vec<String>) -> Vec<String> {
    tokens
        .into_iter()
        .map(|t| strip_french_elision(&t).to_string())
        .collect()
}

pub struct VecTokenStream {
    tokens: Vec<Token>,
    index: usize,
}

impl VecTokenStream {
    fn new(texts: Vec<String>) -> Self {
        let tokens = texts
            .into_iter()
            .enumerate()
            .map(|(pos, text)| Token {
                offset_from: 0,
                offset_to: 0,
                position: pos,
                text,
                position_length: 1,
            })
            .collect();
        VecTokenStream { tokens, index: 0 }
    }
}

impl TokenStream for VecTokenStream {
    fn advance(&mut self) -> bool {
        if self.index < self.tokens.len() {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn token(&self) -> &Token {
        &self.tokens[self.index - 1]
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.tokens[self.index - 1]
    }
}

#[derive(Clone, Default)]
pub struct AtlasTokenizer;

impl Tokenizer for AtlasTokenizer {
    type TokenStream<'a> = BoxTokenStream<'a>;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        let raw_tokens = tokenize_text(text);
        let lang = detect_lang(text, None);
        let processed = match lang {
            Some(atlas_core::Lang::Ar) => apply_arabic_rules(raw_tokens),
            Some(atlas_core::Lang::Fr) => apply_french_rules(raw_tokens),
            _ => raw_tokens,
        };
        let filtered: Vec<String> = processed.into_iter().filter(|t| !t.is_empty()).collect();
        BoxTokenStream::new(VecTokenStream::new(filtered))
    }
}

#[derive(Clone, Default)]
pub struct AsciiFolder;

impl Tokenizer for AsciiFolder {
    type TokenStream<'a> = BoxTokenStream<'a>;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        let raw_tokens = tokenize_text(text);
        let folded: Vec<String> = raw_tokens
            .into_iter()
            .map(|t| strip_diacritics(&t))
            .filter(|t| !t.is_empty())
            .collect();
        BoxTokenStream::new(VecTokenStream::new(folded))
    }
}

pub fn phonetic_encode(token: &str) -> String {
    let chars: Vec<char> = token.to_uppercase().chars().collect();
    if chars.is_empty() {
        return String::new();
    }

    let mut result = String::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };

        let code: Option<&str> = match c {
            'A' | 'E' | 'I' | 'O' | 'U' => {
                if i == 0 {
                    Some("A")
                } else {
                    None
                }
            }
            'B' => {
                if prev == Some('M') && i + 1 == chars.len() {
                    None
                } else {
                    Some("P")
                }
            }
            'C' => match next {
                Some('I') | Some('E') | Some('Y') => Some("S"),
                Some('H') => {
                    i += 1;
                    Some("X")
                }
                _ => Some("K"),
            },
            'D' => match next {
                Some('G') => {
                    i += 1;
                    Some("J")
                }
                _ => Some("T"),
            },
            'F' => Some("F"),
            'G' => match next {
                Some('H') => {
                    i += 1;
                    if i < chars.len() {
                        Some("K")
                    } else {
                        None
                    }
                }
                Some('I') | Some('E') | Some('Y') => Some("J"),
                Some('N') => {
                    i += 1;
                    None
                }
                _ => Some("K"),
            },
            'H' => {
                let is_vowel_before = prev.map(|p| "AEIOU".contains(p)).unwrap_or(false);
                let is_vowel_after = next.map(|n| "AEIOU".contains(n)).unwrap_or(false);
                if is_vowel_before || !is_vowel_after {
                    None
                } else {
                    Some("H")
                }
            }
            'J' => Some("J"),
            'K' => {
                if prev == Some('C') {
                    None
                } else {
                    Some("K")
                }
            }
            'L' => Some("L"),
            'M' => Some("M"),
            'N' => Some("N"),
            'P' => {
                if next == Some('H') {
                    i += 1;
                    Some("F")
                } else {
                    Some("P")
                }
            }
            'Q' => Some("K"),
            'R' => Some("R"),
            'S' => match next {
                Some('H') | Some('I') => Some("X"),
                _ => Some("S"),
            },
            'T' => match next {
                Some('H') => {
                    i += 1;
                    Some("0")
                }
                Some('I') | Some('A') => Some("X"),
                _ => Some("T"),
            },
            'V' => Some("F"),
            'W' => {
                if next.map(|n| "AEIOU".contains(n)).unwrap_or(false) {
                    Some("W")
                } else {
                    None
                }
            }
            'X' => Some("KS"),
            'Y' => {
                if next.map(|n| "AEIOU".contains(n)).unwrap_or(false) {
                    Some("Y")
                } else {
                    None
                }
            }
            'Z' => Some("S"),
            _ => None,
        };

        if let Some(code_str) = code {
            if result.chars().last().map(|l| l.to_string()) != Some(code_str.to_string()) {
                result.push_str(code_str);
            }
        }

        i += 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_diacritics_basic() {
        assert_eq!(strip_diacritics("café"), "cafe");
        assert_eq!(strip_diacritics("naïve"), "naive");
        assert_eq!(strip_diacritics("résumé"), "resume");
        assert_eq!(strip_diacritics("hello"), "hello");
    }

    #[test]
    fn strip_diacritics_arabic_unchanged() {
        let arabic = "مرحبا";
        assert_eq!(strip_diacritics(arabic), arabic);
    }

    #[test]
    fn arabic_tashkeel_stripped() {
        let with_tashkeel = "مَرْحَبًا";
        let stripped = strip_arabic(with_tashkeel);
        assert!(!stripped.contains('\u{064E}'));
        assert!(!stripped.contains('\u{0652}'));
    }

    #[test]
    fn arabic_alef_normalization() {
        assert_eq!(normalize_arabic_alef('أ'), 'ا');
        assert_eq!(normalize_arabic_alef('إ'), 'ا');
        assert_eq!(normalize_arabic_alef('آ'), 'ا');
        assert_eq!(normalize_arabic_alef('ٱ'), 'ا');
        assert_eq!(normalize_arabic_alef('ب'), 'ب');
    }

    #[test]
    fn french_elision_stripped() {
        assert_eq!(strip_french_elision("l'église"), "église");
        assert_eq!(strip_french_elision("d'accord"), "accord");
        assert_eq!(strip_french_elision("s'il"), "il");
        assert_eq!(strip_french_elision("n'est"), "est");
        assert_eq!(strip_french_elision("maison"), "maison");
    }

    #[test]
    fn atlas_tokenizer_produces_lowercase_tokens() {
        let mut tokenizer = AtlasTokenizer;
        let mut stream = tokenizer.token_stream("Hello World");
        let mut tokens = Vec::new();
        while let Some(tok) = stream.next() {
            tokens.push(tok.text.clone());
        }
        assert_eq!(tokens, vec!["hello", "world"]);
    }

    #[test]
    fn ascii_folder_strips_diacritics() {
        let mut tokenizer = AsciiFolder;
        let mut stream = tokenizer.token_stream("café résumé");
        let mut tokens = Vec::new();
        while let Some(tok) = stream.next() {
            tokens.push(tok.text.clone());
        }
        assert_eq!(tokens, vec!["cafe", "resume"]);
    }

    #[test]
    fn detect_lang_hint_overrides() {
        let result = detect_lang("hello world", Some(&atlas_core::Lang::Fr));
        assert_eq!(result, Some(atlas_core::Lang::Fr));
    }

    #[test]
    fn detect_lang_arabic() {
        let long_arabic = "الجمهورية العربية المتحدة دولة عربية في شمال أفريقيا والشرق الأوسط";
        let result = detect_lang(long_arabic, None);
        assert_eq!(result, Some(atlas_core::Lang::Ar));
    }

    #[test]
    fn phonetic_encode_basic() {
        let code = phonetic_encode("smith");
        assert!(!code.is_empty());
        let code2 = phonetic_encode("smyth");
        assert_eq!(code, code2);
    }

    #[test]
    fn phonetic_encode_empty() {
        assert_eq!(phonetic_encode(""), "");
    }
}
