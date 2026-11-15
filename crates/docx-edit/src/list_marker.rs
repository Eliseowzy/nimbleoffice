//! List counters and marker templates shared by the render bridge and the structured export, so
//! both read the same marker for every list paragraph.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use docx_parse::NumberingMap;
use yrs::Any;

use crate::bridge::{any_bool, any_str, map_number, value_number, value_string};

/// The numbering levels OOXML defines, `0..=8`.
pub(crate) const LEVELS: usize = 9;

/// The largest level start honoured; larger starts are clamped.
const MAX_START: i64 = i32::MAX as i64;

/// The largest number Roman numerals write without overlines.
const MAX_ROMAN: i64 = 3_999;

/// A list number its level's format cannot write, such as a Roman numeral past 3,999.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Unrenderable {
    pub value: i64,
    pub format: String,
}

/// List numbering carried across the whole story.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ListState {
    /// The current number of every level of each list, keyed by [`List::key`].
    pub(crate) counters: BTreeMap<String, Vec<i64>>,
    /// The `{numId}:{level}` pairs already numbered, so a `listStartOverride` read without
    /// definitions applies once rather than on every paragraph at that level.
    seen_num_ids: BTreeSet<String>,
    /// The document's numbering definitions, when the session retains them.
    numbering: Option<Arc<NumberingMap>>,
}

impl ListState {
    pub(crate) fn new(numbering: Option<Arc<NumberingMap>>) -> Self {
        Self {
            numbering,
            ..Self::default()
        }
    }

    pub(crate) fn marker_fonts(&self, num_id: f64, level: f64) -> Option<docx_parse::FontFamily> {
        self.numbering
            .as_ref()?
            .get_level(num_id, level)?
            .r_pr?
            .font_family
    }

    pub(crate) fn first_picture_bullet(&self) -> Option<&docx_parse::numbering::PictureBulletSize> {
        self.numbering
            .as_ref()?
            .definitions
            .first_picture_bullet
            .as_ref()
    }
}

/// The level a numbering property names, when it is one of the nine OOXML defines.
pub(crate) fn numbering_level(value: f64) -> Option<usize> {
    (value.fract() == 0.0 && (0.0..LEVELS as f64).contains(&value)).then_some(value as usize)
}

/// One abstract definition's counters, as Word keeps them.
///
/// Every numbering instance (`w:num`) of an abstract definition shares one counter per level.
/// An instance's `w:startOverride` for a level applies once, the first time that instance
/// numbers that level: it sets that level's counter and touches no other level, so a restart
/// ("Restart numbering" writes a new instance overriding level 0) begins again at the override,
/// later paragraphs of any instance keep counting, and an instance first met at a deeper level
/// never rewinds or advances the levels above it. This mirrors LibreOffice's
/// `m_aListOverrideApplied`. Levels begin at `w:start` (1 when absent) and restart as
/// `w:lvlRestart` says.
struct List {
    key: String,
    /// Per level: its start and the `w:lvlRestart` value.
    levels: [(i64, Option<f64>); LEVELS],
    /// Per level: this instance's `w:startOverride`.
    overrides: [Option<i64>; LEVELS],
}

impl List {
    fn resolve(numbering: &NumberingMap, num_id: f64) -> Option<Self> {
        let instance = numbering.get_instance(num_id)?;
        let level_overrides = instance.level_overrides.as_deref().unwrap_or_default();
        let clamp = |start: f64| start.clamp(0.0, MAX_START as f64) as i64;
        let mut levels = [(1, None); LEVELS];
        let mut overrides = [None; LEVELS];
        for (level, entry) in levels.iter_mut().enumerate() {
            let start_override = level_overrides
                .iter()
                .find(|value| value.ilvl == level as f64)
                .and_then(|value| value.start_override)
                .filter(|start| start.is_finite())
                .map(clamp);
            overrides[level] = start_override;
            let Some(definition) = numbering.get_level(num_id, level as f64) else {
                continue;
            };
            let start = definition
                .start
                .filter(|start| start.is_finite())
                .map_or(1, clamp);
            *entry = (start, definition.lvl_restart);
        }
        Some(Self {
            key: instance.abstract_num_id.to_string(),
            levels,
            overrides,
        })
    }

    fn initial(&self) -> Vec<i64> {
        self.levels.iter().map(|(start, _)| start - 1).collect()
    }

    /// Restarts the levels below `level` that numbering it restarts.
    fn restart_below(&self, counters: &mut [i64], level: usize) {
        for (deeper, (counter, (start, restart))) in counters
            .iter_mut()
            .zip(self.levels)
            .enumerate()
            .skip(level + 1)
        {
            let restarts = match restart {
                Some(0.0) => false,
                Some(value) if value >= 1.0 && value - 1.0 < deeper as f64 => {
                    level as f64 <= value - 1.0
                }
                _ => true,
            };
            if restarts {
                *counter = start - 1;
            }
        }
    }
}

fn format_roman(value: i64, uppercase: bool) -> String {
    if !(1..=MAX_ROMAN).contains(&value) {
        return String::new();
    }
    const ONES: [&str; 10] = ["", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX"];
    const TENS: [&str; 10] = ["", "X", "XX", "XXX", "XL", "L", "LX", "LXX", "LXXX", "XC"];
    const HUNDREDS: [&str; 10] = ["", "C", "CC", "CCC", "CD", "D", "DC", "DCC", "DCCC", "CM"];
    let mut result = "M".repeat((value / 1000) as usize);
    result.push_str(HUNDREDS[((value / 100) % 10) as usize]);
    result.push_str(TENS[((value / 10) % 10) as usize]);
    result.push_str(ONES[(value % 10) as usize]);
    if uppercase {
        result
    } else {
        result.to_ascii_lowercase()
    }
}

fn format_alpha(mut value: i64, uppercase: bool) -> String {
    if value <= 0 {
        return String::new();
    }
    let mut chars = Vec::new();
    while value > 0 {
        chars.push((b'A' + ((value - 1) % 26) as u8) as char);
        value = (value - 1) / 26;
    }
    let result: String = chars.into_iter().rev().collect();
    if uppercase {
        result
    } else {
        result.to_ascii_lowercase()
    }
}

/// Whether [`format_list_counter`] renders `format` itself rather than falling back to decimal.
pub(crate) fn renders_format(format: &str) -> bool {
    matches!(
        format,
        "decimal"
            | "upperRoman"
            | "lowerRoman"
            | "upperLetter"
            | "lowerLetter"
            | "decimalZero"
            | "decimalZero3"
            | "decimalZero4"
            | "decimalZero5"
            | "none"
            | "bullet"
    )
}

/// `value` written in `format`, in space bounded by the value's digits.
fn format_list_counter(value: i64, format: Option<&str>) -> Result<String, Unrenderable> {
    if value <= 0 {
        return Ok(String::new());
    }
    Ok(match format {
        Some(roman @ ("upperRoman" | "lowerRoman")) if value > MAX_ROMAN => {
            return Err(Unrenderable {
                value,
                format: roman.to_owned(),
            });
        }
        Some("upperRoman") => format_roman(value, true),
        Some("lowerRoman") => format_roman(value, false),
        Some("upperLetter") => format_alpha(value, true),
        Some("lowerLetter") => format_alpha(value, false),
        Some("decimalZero") => format!("{value:02}"),
        Some("decimalZero3") => format!("{value:03}"),
        Some("decimalZero4") => format!("{value:04}"),
        Some("decimalZero5") => format!("{value:05}"),
        Some("none") => String::new(),
        Some(
            "chineseCounting"
            | "chineseCountingThousand"
            | "taiwaneseCounting"
            | "taiwaneseCountingThousand",
        ) => format_chinese_counting(value, &CHINESE_DIGITS, &CHINESE_UNITS),
        Some("chineseLegalSimplified") => {
            format_chinese_counting(value, &CHINESE_LEGAL_DIGITS, &CHINESE_LEGAL_UNITS)
        }
        Some("ideographTraditional") if value <= 10 => {
            ["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"][value as usize - 1]
                .to_owned()
        }
        _ => value.to_string(),
    })
}

const CHINESE_DIGITS: [&str; 10] = ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
const CHINESE_UNITS: [&str; 4] = ["", "十", "百", "千"];
const CHINESE_LEGAL_DIGITS: [&str; 10] =
    ["零", "壹", "贰", "叁", "肆", "伍", "陆", "柒", "捌", "玖"];
const CHINESE_LEGAL_UNITS: [&str; 4] = ["", "拾", "佰", "仟"];

/// Word's Chinese counting (`chineseCounting`, `chineseCountingThousand`):
/// 1 一, 10 十, 11 十一, 20 二十, 101 一百零一, 110 一百一十. Values past 9999
/// are written in decimal.
fn format_chinese_counting(value: i64, digits: &[&str; 10], units: &[&str; 4]) -> String {
    if !(1..=9999).contains(&value) {
        return value.to_string();
    }
    let places = [
        value / 1000 % 10,
        value / 100 % 10,
        value / 10 % 10,
        value % 10,
    ];
    let mut output = String::new();
    let mut pending_zero = false;
    for (index, &digit) in places.iter().enumerate() {
        let unit = 3 - index;
        if digit == 0 {
            pending_zero = !output.is_empty();
            continue;
        }
        if pending_zero {
            output.push_str(digits[0]);
            pending_zero = false;
        }
        // A leading ten is written 十, not 一十 (Word's 10–19).
        if !(digit == 1 && unit == 1 && output.is_empty() && digits[1] == "一") {
            output.push_str(digits[digit as usize]);
        }
        output.push_str(units[unit]);
    }
    output
}

pub(crate) fn list_level_formats(values: &BTreeMap<String, Any>) -> Vec<String> {
    let Some(Any::Array(formats)) = values.get("listLevelNumFmts") else {
        return Vec::new();
    };
    formats
        .iter()
        .filter_map(any_str)
        .map(str::to_owned)
        .collect()
}

fn resolve_list_template(
    template: &str,
    counters: &[i64],
    formats: &[String],
) -> Result<String, Unrenderable> {
    let chars: Vec<char> = template.chars().collect();
    let mut result = String::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '%' && index + 1 < chars.len() {
            if let Some(digit) = chars[index + 1].to_digit(10) {
                if digit == 0 {
                    index += 2;
                    if chars
                        .get(index)
                        .is_some_and(|ch| matches!(ch, '.' | ')' | ':' | ']'))
                    {
                        index += 1;
                    }
                    continue;
                } else {
                    let counter_index = digit as usize - 1;
                    let value = counters.get(counter_index).copied().unwrap_or(0);
                    let formatted =
                        format_list_counter(value, formats.get(counter_index).map(String::as_str))?;
                    index += 2;
                    let punctuation = chars
                        .get(index)
                        .copied()
                        .filter(|ch| matches!(ch, '.' | ')' | ':' | ']'));
                    if !formatted.is_empty() {
                        result.push_str(&formatted);
                        if let Some(punctuation) = punctuation {
                            result.push(punctuation);
                        }
                    }
                    if punctuation.is_some() {
                        index += 1;
                    }
                    continue;
                }
            }
        }
        result.push(chars[index]);
        index += 1;
    }
    Ok(result)
}

/// [`list_marker`] without the reason a marker could not be written.
pub(crate) fn compute_list_marker(
    values: &BTreeMap<String, Any>,
    state: &mut ListState,
) -> Option<String> {
    list_marker(values, state).and_then(Result::ok)
}

/// The rendered list marker for one paragraph, advancing `state`. Bullets and authored literal
/// markers pass through; a marker containing `%` is a template resolved against the counter stack,
/// and a paragraph with no marker at all gets the dotted counter path (`"1.2."`). With retained
/// definitions, lists are counted as [`List`] describes; without them numbering a level resets
/// every deeper level. A level outside `0..=8` has no marker, and a number the level's format
/// cannot write is reported instead of written.
pub(crate) fn list_marker(
    values: &BTreeMap<String, Any>,
    state: &mut ListState,
) -> Option<Result<String, Unrenderable>> {
    let marker = value_string(values.get("listMarker"));
    let Some(Any::Map(num_pr)) = values.get("numPr") else {
        return marker.map(Ok);
    };
    let Some(num_id) = map_number(num_pr, "numId") else {
        return marker.map(Ok);
    };
    if num_id == 0.0 {
        return marker.map(Ok);
    }
    if values.get("listIsBullet").and_then(any_bool) == Some(true) {
        return Some(Ok(marker.unwrap_or_default()));
    }

    let level = numbering_level(map_number(num_pr, "ilvl").unwrap_or(0.0))?;
    let mut formats = list_level_formats(values);
    formats.truncate(LEVELS);
    let level_format = formats
        .get(level)
        .cloned()
        .or_else(|| value_string(values.get("listNumFmt")));
    let list = state
        .numbering
        .as_deref()
        .and_then(|numbering| List::resolve(numbering, num_id));
    let counter_key = match &list {
        Some(list) => list.key.clone(),
        None => value_number(values.get("listAbstractNumId"))
            .unwrap_or(num_id)
            .to_string(),
    };
    if level_format.as_deref() == Some("none") {
        if formats.len() <= level {
            formats.resize(level + 1, "decimal".to_owned());
            formats[level] = "none".to_owned();
        }
        let counters = state
            .counters
            .get(&counter_key)
            .map(Vec::as_slice)
            .unwrap_or_default();
        return marker
            .map(|template| resolve_list_template(&template, counters, &formats))
            .filter(|value| !matches!(value, Ok(text) if text.is_empty()));
    }

    let counters = state
        .counters
        .entry(counter_key)
        .or_insert_with(|| list.as_ref().map_or_else(|| vec![0; LEVELS], List::initial));
    if counters.len() < LEVELS {
        counters.resize(LEVELS, 0);
    }
    match &list {
        Some(list) => {
            if state.seen_num_ids.insert(format!("{num_id}:{level}"))
                && let Some(start) = list.overrides[level]
            {
                counters[level] = start - 1;
            }
            counters[level] = counters[level].saturating_add(1);
            list.restart_below(counters, level);
        }
        None => {
            if state.seen_num_ids.insert(format!("{num_id}:{level}"))
                && let Some(start) =
                    value_number(values.get("listStartOverride")).filter(|start| start.is_finite())
            {
                counters[level] = start.clamp(0.0, MAX_START as f64) as i64 - 1;
            }
            counters[level] = counters[level].saturating_add(1);
            for value in counters.iter_mut().skip(level + 1) {
                *value = 0;
            }
        }
    }

    if let Some(marker) = marker {
        if marker.contains('%') {
            return Some(resolve_list_template(&marker, counters, &formats));
        }
        return Some(Ok(marker));
    }
    let mut parts = Vec::new();
    for value in counters.iter().take(level + 1) {
        if *value <= 0 {
            break;
        }
        parts.push(value.to_string());
    }
    Some(Ok(if parts.is_empty() {
        "1.".to_owned()
    } else {
        format!("{}.", parts.join("."))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definitions(levels: &str, instances: &str) -> Arc<NumberingMap> {
        let xml = format!(
            r#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:abstractNum w:abstractNumId="4">{levels}</w:abstractNum>{instances}</w:numbering>"#
        );
        let limits = docx_parse::ParseLimits::default();
        Arc::new(
            docx_parse::numbering::parse_numbering(
                Some(xml.as_bytes()),
                "word/numbering.xml",
                &mut docx_parse::ParseBudget::new(&limits),
            )
            .unwrap(),
        )
    }

    fn paragraph(num_id: f64, level: f64, template: &str) -> BTreeMap<String, Any> {
        BTreeMap::from([
            (
                "numPr".to_owned(),
                Any::from(std::collections::HashMap::from([
                    ("numId".to_owned(), Any::Number(num_id)),
                    ("ilvl".to_owned(), Any::Number(level)),
                ])),
            ),
            ("listMarker".to_owned(), Any::from(template)),
            ("listAbstractNumId".to_owned(), Any::Number(4.0)),
            (
                "listLevelNumFmts".to_owned(),
                Any::from_json(r#"["decimal","decimal","decimal"]"#).unwrap(),
            ),
        ])
    }

    #[test]
    fn a_restart_level_names_the_level_that_restarts_it() {
        let level = |ilvl: u32, extra: &str| {
            format!(
                r#"<w:lvl w:ilvl="{ilvl}"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%{}."/>{extra}</w:lvl>"#,
                ilvl + 1
            )
        };
        let numbering = definitions(
            &[
                level(0, ""),
                level(1, ""),
                level(2, r#"<w:lvlRestart w:val="1"/>"#),
            ]
            .concat(),
            r#"<w:num w:numId="9"><w:abstractNumId w:val="4"/></w:num>"#,
        );
        let mut state = ListState::new(Some(numbering));
        let markers: Vec<Option<String>> = [
            (2.0, "%3."),
            (1.0, "%2."),
            (2.0, "%3."),
            (0.0, "%1."),
            (2.0, "%3."),
        ]
        .into_iter()
        .map(|(level, template)| compute_list_marker(&paragraph(9.0, level, template), &mut state))
        .collect();
        assert_eq!(
            markers,
            ["1.", "1.", "2.", "1.", "1."].map(|marker| Some(marker.to_owned())),
            "level 2 keeps counting past level 1 and restarts after level 0"
        );
    }

    fn decimal_level(start: u32) -> String {
        format!(
            r#"<w:lvl w:ilvl="0"><w:start w:val="{start}"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl>"#
        )
    }

    fn markers(state: &mut ListState, paragraphs: &[BTreeMap<String, Any>]) -> Vec<String> {
        paragraphs
            .iter()
            .map(|values| compute_list_marker(values, state).unwrap_or_default())
            .collect()
    }

    #[test]
    fn instances_of_one_definition_continue_each_other() {
        let numbering = definitions(
            &decimal_level(1),
            r#"<w:num w:numId="1"><w:abstractNumId w:val="4"/></w:num><w:num w:numId="2"><w:abstractNumId w:val="4"/></w:num>"#,
        );
        let mut state = ListState::new(Some(numbering));
        assert_eq!(
            markers(
                &mut state,
                &[
                    paragraph(1.0, 0.0, "%1."),
                    paragraph(1.0, 0.0, "%1."),
                    paragraph(2.0, 0.0, "%1."),
                    paragraph(1.0, 0.0, "%1."),
                ]
            ),
            ["1.", "2.", "3.", "4."]
        );
    }

    #[test]
    fn a_restart_numbering_instance_counts_again_from_its_override() {
        let numbering = definitions(
            &decimal_level(1),
            concat!(
                r#"<w:num w:numId="1"><w:abstractNumId w:val="4"/></w:num>"#,
                r#"<w:num w:numId="2"><w:abstractNumId w:val="4"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="1"/></w:lvlOverride></w:num>"#,
            ),
        );
        let mut state = ListState::new(Some(numbering));
        assert_eq!(
            markers(
                &mut state,
                &[
                    paragraph(1.0, 0.0, "%1."),
                    paragraph(1.0, 0.0, "%1."),
                    paragraph(1.0, 0.0, "%1."),
                    paragraph(2.0, 0.0, "%1."),
                    paragraph(2.0, 0.0, "%1."),
                ]
            ),
            ["1.", "2.", "3.", "1.", "2."]
        );
    }

    #[test]
    fn an_override_applies_once_and_instances_share_counters() {
        let lvl = |ilvl: u32, start: u32, extra: &str| {
            format!(
                r#"<w:lvl w:ilvl="{ilvl}"><w:start w:val="{start}"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1.%2"/>{extra}</w:lvl>"#
            )
        };
        let over = |level: u32, value: u32| {
            format!(
                r#"<w:lvlOverride w:ilvl="{level}"><w:startOverride w:val="{value}"/></w:lvlOverride>"#
            )
        };
        let instance = |id: u32, chapter: u32| {
            format!(
                r#"<w:num w:numId="{id}"><w:abstractNumId w:val="4"/>{}{}</w:num>"#,
                over(0, chapter),
                over(1, 1)
            )
        };
        let numbering = definitions(
            &[lvl(0, 1, ""), lvl(1, 3, r#"<w:lvlRestart w:val="0"/>"#)].concat(),
            &[
                r#"<w:num w:numId="5"><w:abstractNumId w:val="4"/></w:num>"#.to_owned(),
                instance(24, 1),
                instance(25, 2),
            ]
            .concat(),
        );
        let mut state = ListState::new(Some(numbering));
        let heading = |level: f64| paragraph(5.0, level, if level == 0.0 { "%1" } else { "%1.%2" });
        let first = |id: f64| paragraph(id, 1.0, "%1.%2");
        assert_eq!(
            markers(
                &mut state,
                &[
                    heading(0.0),
                    first(24.0),
                    heading(1.0),
                    first(24.0),
                    heading(0.0),
                    first(25.0),
                    heading(1.0),
                ]
            ),
            ["1", "1.1", "1.2", "1.3", "2", "2.1", "2.2"],
            "lvlRestart=0 never rewinds level 1; a repeated numId does not reapply its override"
        );
    }

    #[test]
    fn numbers_a_format_cannot_write_are_reported() {
        let level = r#"<w:lvl w:ilvl="0"><w:start w:val="2000000000"/><w:numFmt w:val="upperRoman"/><w:lvlText w:val="%1."/></w:lvl>"#;
        let numbering = definitions(
            level,
            r#"<w:num w:numId="1"><w:abstractNumId w:val="4"/></w:num>"#,
        );
        let mut state = ListState::new(Some(numbering));
        let mut values = paragraph(1.0, 0.0, "%1.");
        values.insert(
            "listLevelNumFmts".to_owned(),
            Any::from_json(r#"["upperRoman"]"#).unwrap(),
        );
        assert_eq!(
            list_marker(&values, &mut state),
            Some(Err(Unrenderable {
                value: 2_000_000_000,
                format: "upperRoman".to_owned()
            }))
        );
        assert_eq!(compute_list_marker(&values, &mut state), None);
    }

    #[test]
    fn levels_outside_the_schema_allocate_no_counters() {
        let mut state = ListState::default();
        assert_eq!(
            compute_list_marker(&paragraph(9.0, 4_294_967_295.0, "%1."), &mut state),
            None
        );
        assert!(state.counters.is_empty());
    }

    #[test]
    fn chinese_counting_writes_word_numerals() {
        for (value, text) in [
            (1, "一"),
            (9, "九"),
            (10, "十"),
            (11, "十一"),
            (20, "二十"),
            (23, "二十三"),
            (100, "一百"),
            (101, "一百零一"),
            (110, "一百一十"),
            (1001, "一千零一"),
            (2020, "二千零二十"),
        ] {
            assert_eq!(
                format_list_counter(value, Some("chineseCountingThousand")).unwrap(),
                text
            );
        }
        assert_eq!(
            format_list_counter(12, Some("chineseLegalSimplified")).unwrap(),
            "壹拾贰"
        );
        assert_eq!(
            format_list_counter(3, Some("ideographTraditional")).unwrap(),
            "丙"
        );
        assert_eq!(
            format_list_counter(10000, Some("chineseCounting")).unwrap(),
            "10000"
        );
    }
}
