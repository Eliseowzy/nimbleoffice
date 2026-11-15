//! East Asian auto-spacing: `w:autoSpaceDE` and `w:autoSpaceDN` (§17.3.1.11,
//! §17.3.1.12), both defaulting to on.
//!
//! Word widens the gap where East Asian text meets Latin letters or digits.
//! Both sides of the boundary and both settings are measured off Word's own
//! PDF exports of the Japanese corpus: the advance of an East Asian character
//! standing before a Latin one is a flat 1.250 em against 1.000 em before
//! another East Asian character, and the same 0.250 em appears on the
//! Latin-then-East-Asian side. A document that turns the feature off
//! (`w:val="0"` on every paragraph) measures 1.000 em on both, which is what
//! pins the constant to the setting rather than to the face.
//!
//! Nothing is inserted next to a space. Word's exports put no extra advance
//! before or after U+0020 at a script boundary. Nor next to punctuation: a
//! fullwidth bracket or comma (U+FF08, U+FF0C, ...) is punctuation like the
//! U+3000 block, so `（2014）` takes no gap inside its brackets.
//!
//! `w:characterSpacingControl w:val="compressPunctuation"` (§17.15.1.18) is
//! carried here too, since it is the other half of how Word spaces East Asian
//! text. Punctuation keeps its full em until a line is a little too long;
//! then Word gives up the blank half of the em box of the marks already on the
//! line (right half of a comma, period, colon or closing bracket, left half of
//! an opening bracket) to pull one more character in. This module only says
//! how much each mark can give up; the line filler decides when.

/// Extra advance at an East Asian / Latin boundary, as a fraction of the East
/// Asian side's font size. Measured at 0.250 em off Word's PDF exports.
pub const AUTO_SPACE_EM: f32 = 0.25;

/// How much of its em box compressed punctuation gives up.
pub const COMPRESSED_PUNCTUATION_EM: f32 = 0.5;

/// Which auto-spacing settings a paragraph has on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoSpace {
    /// `w:autoSpaceDE`: East Asian text against Latin letters.
    pub latin: bool,
    /// `w:autoSpaceDN`: East Asian text against digits.
    pub numeric: bool,
    /// `w:characterSpacingControl w:val="compressPunctuation"`.
    pub compress_punctuation: bool,
}

impl Default for AutoSpace {
    fn default() -> Self {
        Self {
            latin: true,
            numeric: true,
            compress_punctuation: false,
        }
    }
}

/// How a compressed punctuation mark sits: `shift` moves its glyph (negative
/// is left) and `trim` comes off its advance, both in px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PunctuationTrim {
    pub shift: f32,
    pub trim: f32,
}

impl AutoSpace {
    /// Resolves the two `Option<bool>` opt-outs; absent is the OOXML default.
    pub fn from_options(latin: Option<bool>, numeric: Option<bool>) -> Self {
        Self {
            latin: latin.unwrap_or(true),
            numeric: numeric.unwrap_or(true),
            compress_punctuation: false,
        }
    }

    /// The same with punctuation compression as the document asks for it.
    pub fn compressing(self, compress_punctuation: Option<bool>) -> Self {
        Self {
            compress_punctuation: compress_punctuation.unwrap_or(false),
            ..self
        }
    }

    /// What compression can take off `ch`, drawn `advance_px` wide at
    /// `size_px`, or `None` when it keeps its width. A mark only compresses
    /// when its face sets it on a full em: a Latin face's narrow curly quote
    /// has no blank half to give up.
    pub fn punctuation_trim(
        self,
        ch: char,
        advance_px: f32,
        size_px: f32,
    ) -> Option<PunctuationTrim> {
        if !self.compress_punctuation
            || size_px.is_nan()
            || size_px <= 0.0
            || advance_px < size_px * 0.9
        {
            return None;
        }
        let trim = COMPRESSED_PUNCTUATION_EM * size_px;
        match punctuation_side(ch)? {
            Side::Leading => Some(PunctuationTrim { shift: -trim, trim }),
            Side::Trailing => Some(PunctuationTrim { shift: 0.0, trim }),
        }
    }

    pub fn any(self) -> bool {
        self.latin || self.numeric
    }

    /// The extra advance between `prev` and `next` in em, 0.0 when the pair is
    /// not a boundary this paragraph spaces.
    pub fn extra_em(self, prev: char, next: char) -> f32 {
        let (left, right) = (class(prev), class(next));
        let spaced = match (left, right) {
            (Class::EastAsian, Class::Latin) | (Class::Latin, Class::EastAsian) => self.latin,
            (Class::EastAsian, Class::Numeric) | (Class::Numeric, Class::EastAsian) => self.numeric,
            _ => false,
        };
        if spaced { AUTO_SPACE_EM } else { 0.0 }
    }

    /// The same as an advance, sized off the East Asian side of the boundary.
    pub fn extra_px(self, prev: char, next: char, prev_px: f32, next_px: f32) -> f32 {
        let em = self.extra_em(prev, next);
        if em == 0.0 {
            return 0.0;
        }
        em * if class(prev) == Class::EastAsian {
            prev_px
        } else {
            next_px
        }
    }
}

/// Which half of its em box a compressible mark leaves blank.
enum Side {
    /// Opening brackets and quotes: ink on the right.
    Leading,
    /// Commas, periods and closing brackets and quotes: ink on the left.
    Trailing,
}

/// The pause marks and brackets Word compresses. Colon and semicolon are
/// included because Word's exports compress a colon to fit a line; question
/// and exclamation marks keep their em: Chinese faces fill them.
fn punctuation_side(ch: char) -> Option<Side> {
    match ch {
        '\u{FF08}' | '\u{FF3B}' | '\u{FF5B}' | '\u{3008}' | '\u{300A}' | '\u{300C}'
        | '\u{300E}' | '\u{3010}' | '\u{3014}' | '\u{3016}' | '\u{3018}' | '\u{201C}'
        | '\u{2018}' => Some(Side::Leading),
        '\u{3001}' | '\u{3002}' | '\u{FF0C}' | '\u{FF0E}' | '\u{FF1A}' | '\u{FF1B}'
        | '\u{FF09}' | '\u{FF3D}' | '\u{FF5D}' | '\u{3009}' | '\u{300B}' | '\u{300D}'
        | '\u{300F}' | '\u{3011}' | '\u{3015}' | '\u{3017}' | '\u{3019}' | '\u{201D}'
        | '\u{2019}' => Some(Side::Trailing),
        _ => None,
    }
}

/// Ideographs, kana, hangul and the CJK and fullwidth punctuation around them.
pub fn is_east_asian_text(ch: char) -> bool {
    class(ch) == Class::EastAsian || matches!(ch as u32, 0x3001..=0x303F | 0xFF01..=0xFF65)
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Class {
    EastAsian,
    Latin,
    Numeric,
    Other,
}

/// CJK punctuation (U+3000–U+303F) is deliberately `Other`: it carries its own
/// spacing rules, and U+3000 is a space. So is the punctuation among the
/// fullwidth forms; their digits and letters stay East Asian.
fn class(ch: char) -> Class {
    let code = ch as u32;
    if matches!(code, 0xFF01..=0xFF0F | 0xFF1A..=0xFF20 | 0xFF3B..=0xFF40 | 0xFF5B..=0xFF65) {
        return Class::Other;
    }
    if matches!(code,
        0x3040..=0x30FF
        | 0x3100..=0x312F
        | 0x31A0..=0x31BF
        | 0x3190..=0x319F
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xF900..=0xFAFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7AF
        | 0x1100..=0x11FF
        | 0x3130..=0x318F
        | 0xFF01..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x20000..=0x2FA1F)
    {
        return Class::EastAsian;
    }
    if code >= 0x2000 {
        return Class::Other;
    }
    if ch.is_ascii_digit() {
        return Class::Numeric;
    }
    if ch.is_alphabetic() {
        return Class::Latin;
    }
    Class::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quarter_em_lands_on_both_sides_of_a_script_boundary() {
        let on = AutoSpace::default();
        assert_eq!(on.extra_em('国', 'a'), 0.25);
        assert_eq!(on.extra_em('a', '国'), 0.25);
        assert_eq!(on.extra_em('ぁ', '7'), 0.25);
        assert_eq!(on.extra_em('7', 'ア'), 0.25);
        assert_eq!(on.extra_em('国', '花'), 0.0);
        assert_eq!(on.extra_em('a', 'b'), 0.0);
    }

    #[test]
    fn a_space_or_ideographic_punctuation_never_takes_one() {
        let on = AutoSpace::default();
        for pair in [
            ('国', ' '),
            (' ', 'a'),
            ('国', '、'),
            ('。', 'a'),
            ('国', '\u{3000}'),
        ] {
            assert_eq!(on.extra_em(pair.0, pair.1), 0.0, "{pair:?}");
        }
    }

    #[test]
    fn fullwidth_punctuation_never_takes_one_but_fullwidth_digits_do() {
        let on = AutoSpace::default();
        for pair in [
            ('\u{FF08}', '2'),
            ('4', '\u{FF09}'),
            ('\u{FF0C}', 'a'),
            ('a', '\u{FF1A}'),
        ] {
            assert_eq!(on.extra_em(pair.0, pair.1), 0.0, "{pair:?}");
        }
        assert_eq!(on.extra_em('\u{FF12}', 'a'), 0.25);
    }

    #[test]
    fn compression_can_give_up_half_of_pause_marks_and_brackets_set_on_a_full_em() {
        let off = AutoSpace::default();
        assert_eq!(off.punctuation_trim('，', 16.0, 16.0), None);
        let on = AutoSpace::default().compressing(Some(true));
        let trailing = PunctuationTrim {
            shift: 0.0,
            trim: 8.0,
        };
        let leading = PunctuationTrim {
            shift: -8.0,
            trim: 8.0,
        };
        for ch in ['，', '。', '、', '：', '；', '）', '》', '」', '”'] {
            assert_eq!(on.punctuation_trim(ch, 16.0, 16.0), Some(trailing), "{ch}");
        }
        for ch in ['（', '《', '「', '“'] {
            assert_eq!(on.punctuation_trim(ch, 16.0, 16.0), Some(leading), "{ch}");
        }
        for ch in ['？', '！', '国', 'a'] {
            assert_eq!(on.punctuation_trim(ch, 16.0, 16.0), None, "{ch}");
        }
        // a Latin face's curly quote is already narrow
        assert_eq!(on.punctuation_trim('“', 7.0, 16.0), None);
    }

    #[test]
    fn each_setting_gates_only_its_own_boundary() {
        let no_latin = AutoSpace::from_options(Some(false), None);
        assert_eq!(no_latin.extra_em('国', 'a'), 0.0);
        assert_eq!(no_latin.extra_em('国', '1'), 0.25);
        let no_numeric = AutoSpace::from_options(None, Some(false));
        assert_eq!(no_numeric.extra_em('国', 'a'), 0.25);
        assert_eq!(no_numeric.extra_em('国', '1'), 0.0);
        assert!(!AutoSpace::from_options(Some(false), Some(false)).any());
    }

    #[test]
    fn east_asian_text_covers_ideographs_and_cjk_punctuation_only() {
        assert!(is_east_asian_text('字'));
        assert!(is_east_asian_text('，'));
        assert!(is_east_asian_text('、'));
        assert!(!is_east_asian_text('a'));
        assert!(!is_east_asian_text('['));
        assert!(!is_east_asian_text(' '));
    }
}
