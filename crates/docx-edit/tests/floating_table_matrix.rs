//! Floating-table combinations the other fixture groups leave uncovered
//! (docs/floating-tables.md, section 6), compared with Word word by word.
//!
//! Every word in a fixture is a unique token (fixtures/floating-table-matrix/
//! generate.py); word.json holds where Word 16.114 put each one (measure.py). A
//! token agrees when it lands on the same page with its baseline within 0.5pt and
//! its left edge within 2pt. A token Word clips off the page (absent from its PDF)
//! must be absent from the engine's pages too.
//!
//! `BASELINE` pins how many tokens of each case disagree today. The gate is a
//! ratchet: a case may never disagree more; when it disagrees less, lower its entry.

use std::collections::BTreeMap;

use docx_edit::{EngineSession, seed_from_docx};
use serde_json::{Value, json};

const FONT: &[u8] = include_bytes!("../../ooxml-text/tests/fonts/LiberationSans-Regular.ttf");
const PT_PER_PX: f64 = 0.75;
const PAGE_HEIGHT_PT: f64 = 792.0;

/// Disagreeing tokens per case at the time the fixtures were added.
const BASELINE: &[(&str, usize)] = &[
    ("cols2-c14", 1),
    ("cols2-c15", 5),
    ("fullwidth-over-inline-c14", 36),
    ("fullwidth-over-inline-c15", 0),
    ("margin-c14", 0),
    ("margin-c15", 0),
    ("margin-pretext-c14", 0),
    ("margin-pretext-c15", 0),
    ("outside-c14", 0),
    ("outside-c15", 0),
    ("text-pretext-c14", 0),
    ("text-pretext-c15", 0),
    ("ybottom-c14", 36),
    ("ybottom-c15", 32),
    ("ycenter-c14", 0),
    ("ycenter-c15", 0),
];

type Tokens = BTreeMap<String, (usize, f64, f64)>;

fn dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/floating-table-matrix")
}

/// Splits a line's characters into tokens: a lowercase letter followed by digits.
fn split_tokens(page: usize, y: f64, chars: &[(f64, char)], out: &mut Tokens) {
    let mut i = 0;
    while i < chars.len() {
        let (x, c) = chars[i];
        let digits = chars[i + 1..]
            .iter()
            .take_while(|(_, d)| d.is_ascii_digit())
            .count();
        if c.is_ascii_lowercase() && digits > 0 {
            let token: String = chars[i..=i + digits].iter().map(|(_, ch)| *ch).collect();
            out.insert(token, (page, y, x));
            i += digits + 1;
        } else {
            i += 1;
        }
    }
}

fn engine_tokens(bytes: &[u8]) -> Tokens {
    docx_layout::with_private_measure_fonts(|| {
        let font = docx_layout::register_measure_font(FONT).unwrap();
        let engine = EngineSession::new(78903);
        seed_from_docx(engine.doc(), bytes).unwrap();
        let package = docx_parse::parse_docx_s9_wire(bytes, Default::default())
            .unwrap()
            .document
            .package;
        let request = json!({
            "bodyStory": "body", "renderEnv": {},
            "regions": {
                "sections": [{"properties": package.document.final_section_properties}],
                "settings": package.settings
            },
            "measurement": {
                "fontChains": {"arial|0|0": [font], "arial|1|0": [font]},
                "defaults": {"fontFamily": "Arial", "fontSize": 12},
                "authoritativeShaping": true
            }
        });
        let layout = engine
            .layout_document_with_regions_json(&request.to_string())
            .unwrap();
        if std::env::var("MATRIX_DUMP_LAYOUT").is_ok() {
            let parsed: Value = serde_json::from_str(&layout).unwrap();
            let pages = parsed["layout"]["pages"]
                .as_array()
                .or_else(|| parsed["pages"].as_array())
                .unwrap();
            for (i, page) in pages.iter().enumerate() {
                for fragment in page["fragments"].as_array().unwrap().iter() {
                    println!("LAYOUT page {i}: {fragment}");
                }
            }
        }
        let display: Value =
            serde_json::from_str(&engine.build_display_list_json(&layout).unwrap()).unwrap();
        let mut chars: Vec<(usize, f64, f64, char)> = Vec::new();
        for (page, page_value) in display["pages"].as_array().unwrap().iter().enumerate() {
            let mut stack: Vec<&Value> = page_value["primitives"]
                .as_array()
                .unwrap()
                .iter()
                .collect();
            while let Some(primitive) = stack.pop() {
                if let Some(children) = primitive["children"].as_array() {
                    stack.extend(children);
                }
                if primitive["kind"].as_str() != Some("text") {
                    continue;
                }
                let (Some(x), Some(y), Some(text)) = (
                    primitive["x"].as_f64(),
                    primitive["baselineY"].as_f64(),
                    primitive["text"].as_str(),
                ) else {
                    continue;
                };
                // A row cut by a page break paints its straddling lines on both
                // pages and clips each to its fragment; only the visible copy counts.
                let clip = &primitive["clipGroup"]["clip"];
                if let (Some(top), Some(height)) = (clip["y"].as_f64(), clip["h"].as_f64())
                    && !(top..=top + height).contains(&y)
                {
                    continue;
                }
                let y_pt = y * PT_PER_PX;
                // Word's PDF drops what lies off the page; do the same.
                if !(0.0..=PAGE_HEIGHT_PT).contains(&y_pt) {
                    continue;
                }
                for c in text.chars() {
                    chars.push((page, y_pt, x * PT_PER_PX, c));
                }
            }
        }
        chars.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(a.1.total_cmp(&b.1))
                .then(a.2.total_cmp(&b.2))
        });
        let mut out = Tokens::new();
        let mut start = 0;
        while start < chars.len() {
            let (page, y, _, _) = chars[start];
            let end = start
                + chars[start..]
                    .iter()
                    .take_while(|(p, cy, _, _)| *p == page && (cy - y).abs() < 0.01)
                    .count();
            let line: Vec<(f64, char)> = chars[start..end].iter().map(|c| (c.2, c.3)).collect();
            split_tokens(page, y, &line, &mut out);
            start = end;
        }
        out
    })
}

fn word_cases() -> BTreeMap<String, Tokens> {
    let json: Value =
        serde_json::from_str(&std::fs::read_to_string(dir().join("word.json")).unwrap()).unwrap();
    json["cases"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, tokens)| {
            let tokens = tokens
                .as_object()
                .unwrap()
                .iter()
                .map(|(token, v)| {
                    let v = v.as_array().unwrap();
                    (
                        token.clone(),
                        (
                            v[0].as_u64().unwrap() as usize,
                            v[1].as_f64().unwrap(),
                            v[2].as_f64().unwrap(),
                        ),
                    )
                })
                .collect();
            (name.clone(), tokens)
        })
        .collect()
}

/// Disagreeing tokens, in reading order, each with a short reason.
fn disagreements(word: &Tokens, engine: &Tokens) -> Vec<String> {
    let mut out = Vec::new();
    let mut keys: Vec<&String> = word.keys().chain(engine.keys()).collect();
    keys.sort();
    keys.dedup();
    let order = |k: &String| word.get(k).or(engine.get(k)).copied().unwrap();
    keys.sort_by(|a, b| {
        let (pa, ya, xa) = order(a);
        let (pb, yb, xb) = order(b);
        pa.cmp(&pb).then(ya.total_cmp(&yb)).then(xa.total_cmp(&xb))
    });
    for key in keys {
        match (word.get(key), engine.get(key)) {
            (Some(w), Some(e)) => {
                let placed = w.0 == e.0 && (w.1 - e.1).abs() < 0.5;
                if !placed || (w.2 - e.2).abs() >= 2.0 {
                    out.push(format!(
                        "{key} [{}]: word p{} y{:.2} x{:.2}, engine p{} y{:.2} x{:.2}",
                        if placed { "x" } else { "y" },
                        w.0,
                        w.1,
                        w.2,
                        e.0,
                        e.1,
                        e.2
                    ));
                }
            }
            (Some(w), None) => out.push(format!(
                "{key}: word p{} y{:.2} x{:.2}, engine missing",
                w.0, w.1, w.2
            )),
            (None, Some(e)) => out.push(format!(
                "{key}: word clipped, engine p{} y{:.2} x{:.2}",
                e.0, e.1, e.2
            )),
            (None, None) => unreachable!(),
        }
    }
    out
}

#[test]
fn floating_table_matrix_does_not_regress_from_word() {
    let baseline: BTreeMap<&str, usize> = BASELINE.iter().copied().collect();
    let mut report = Vec::new();
    let mut regressions = Vec::new();
    for (name, word) in word_cases() {
        if std::env::var("MATRIX_ONLY").is_ok_and(|only| only != name) {
            continue;
        }
        let bytes = std::fs::read(dir().join(format!("{name}.docx"))).unwrap();
        let diff = disagreements(&word, &engine_tokens(&bytes));
        if std::env::var("MATRIX_DUMP").as_deref() == Ok(name.as_str()) {
            for line in &diff {
                println!("{name}: {line}");
            }
        }
        let allowed = *baseline
            .get(name.as_str())
            .unwrap_or_else(|| panic!("{name}: add it to BASELINE"));
        let kind = |needle: &str| diff.iter().filter(|d| d.contains(needle)).count();
        let x_only = diff
            .iter()
            .filter(|d| d.starts_with('x') || d.contains("[x]"))
            .count();
        let first = diff
            .iter()
            .find(|d| !d.contains("[x]"))
            .or(diff.first())
            .cloned()
            .unwrap_or_default();
        report.push(format!(
            "{name:26} {:3}/{:3} (baseline {allowed:3}; x-only {x_only}, page/y {}, missing {}, clipped {}) first: {first}",
            diff.len(),
            word.len(),
            kind("[y]"),
            kind("engine missing"),
            kind("word clipped"),
        ));
        if diff.len() > allowed {
            regressions.push(name.clone());
        } else if diff.len() < allowed {
            report.push(format!(
                "  {name}: improved, lower its BASELINE entry to {}",
                diff.len()
            ));
        }
    }
    let report = report.join("\n");
    println!("{report}");
    assert!(
        regressions.is_empty(),
        "regressed: {regressions:?}\n{report}"
    );
}
