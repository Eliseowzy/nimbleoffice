#![allow(dead_code)]

use docx_edit::{EngineSession, seed_from_docx};
use serde_json::{Value, json};

const SERIF: &[u8] =
    include_bytes!("../../../../packages/fonts/assets/LiberationSerif-Regular.ttf");
const SANS: &[u8] = include_bytes!("../../../ooxml-text/tests/fonts/LiberationSans-Regular.ttf");
const NOTO_SANS_SC: &[u8] =
    include_bytes!("../../../../packages/fonts-cjk/assets/NotoSansSC-Regular.otf.zst");
const PT_PER_PX: f64 = 0.75;

/// One laid-out line: its text, baseline and right edge in points.
#[derive(Debug, Clone)]
pub struct Line {
    pub text: String,
    pub y: f64,
    pub right: f64,
}

/// A laid-out page: text baselines (whitespace dropped, as in `word.json`) and image boxes in points from the top-left.
#[derive(Debug, Default)]
pub struct Page {
    pub width: f64,
    pub height: f64,
    pub lines: Vec<Line>,
    pub images: Vec<(f64, f64)>,
    pub parity_filler: bool,
}

impl Page {
    pub fn line(&self, text: &str) -> &Line {
        self.lines
            .iter()
            .find(|line| line.text == text)
            .unwrap_or_else(|| panic!("no {text} line in {:?}", self.lines))
    }

    pub fn baseline(&self, text: &str) -> f64 {
        self.line(text).y
    }
}

/// Lays `bytes` out as the app requests it, with Liberation Serif for Times New Roman
/// and Noto Sans SC for every Chinese face.
pub fn layout(bytes: &[u8]) -> Vec<Page> {
    let serif = docx_layout::register_measure_font(SERIF).unwrap();
    let sans = docx_layout::register_measure_font(SANS).unwrap();
    let cjk = nimbleoffice_fonts::decode(NOTO_SANS_SC, None, 64 << 20).unwrap();
    let cjk = docx_layout::register_measure_font(&cjk).unwrap();
    let package = docx_parse::parse_docx_s9_wire(bytes, Default::default())
        .unwrap()
        .document
        .package;
    let engine = EngineSession::new(76600);
    seed_from_docx(engine.doc(), bytes).unwrap();
    let mut request = docx_edit::layout_request::region_layout_request(
        &serde_json::to_value(&package).unwrap(),
        0.0,
    );
    let mut chains = serde_json::Map::new();
    for (family, chain) in [
        ("times new roman", vec![serif, cjk]),
        ("arial", vec![sans, cjk]),
        ("宋体", vec![cjk]),
        ("simsun", vec![cjk]),
    ] {
        for style in ["0|0", "1|0"] {
            chains.insert(format!("{family}|{style}"), json!(chain));
        }
    }
    request["measurement"] = json!({
        "fontChains": chains,
        "defaults": {"fontFamily": "Times New Roman", "fontSize": 12},
        "authoritativeShaping": true
    });
    let layout = engine
        .layout_document_with_regions_json(&request.to_string())
        .unwrap();
    let layout_value: Value = serde_json::from_str(&layout).unwrap();
    let layout_pages = layout_value["layout"]["pages"].as_array().unwrap();
    let fillers: Vec<bool> = layout_value["layout"]["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|page| page["parityFiller"] == true)
        .collect();
    let display: Value =
        serde_json::from_str(&engine.build_display_list_json(&layout).unwrap()).unwrap();
    display["pages"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fillers)
        .zip(layout_pages)
        .map(|((page, parity_filler), layout_page)| {
            let mut out = Page {
                width: layout_page["size"]["w"].as_f64().unwrap() * PT_PER_PX,
                height: layout_page["size"]["h"].as_f64().unwrap() * PT_PER_PX,
                parity_filler,
                ..Page::default()
            };
            for region in [
                &page["primitives"],
                &page["header"]["primitives"],
                &page["footer"]["primitives"],
            ] {
                for primitive in region.as_array().into_iter().flatten() {
                    collect(primitive, &mut out);
                }
            }
            out.lines.sort_by(|a, b| a.y.total_cmp(&b.y));
            out
        })
        .collect()
}

fn collect(primitive: &Value, page: &mut Page) {
    match primitive["kind"].as_str() {
        Some("text") => {
            let text: String = primitive["text"]
                .as_str()
                .unwrap_or_default()
                .split_whitespace()
                .collect();
            if text.is_empty() {
                return;
            }
            let y = primitive["baselineY"].as_f64().unwrap() * PT_PER_PX;
            let right = (primitive["x"].as_f64().unwrap_or(0.0)
                + primitive["width"].as_f64().unwrap_or(0.0))
                * PT_PER_PX;
            match page.lines.iter_mut().find(|line| (line.y - y).abs() < 0.01) {
                Some(line) => {
                    line.text.push_str(&text);
                    line.right = line.right.max(right);
                }
                None => page.lines.push(Line { text, y, right }),
            }
        }
        Some("image") => {
            let y = primitive["y"].as_f64().unwrap();
            let h = primitive["h"]
                .as_f64()
                .or(primitive["height"].as_f64())
                .unwrap();
            page.images.push((y * PT_PER_PX, (y + h) * PT_PER_PX));
        }
        _ => {
            for child in primitive["children"].as_array().into_iter().flatten() {
                collect(child, page);
            }
        }
    }
}

/// Word's line `text` on page `index`, from a fixture's `word.json`.
pub fn word_line<'a>(word: &'a Value, fixture: &str, index: usize, text: &str) -> &'a Value {
    word[fixture][index]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|line| line["text"] == text)
        .unwrap_or_else(|| panic!("Word has no {text} on {fixture} page {index}"))
}

/// Word's baseline of `text` on page `index`, from a fixture's `word.json`.
pub fn word_baseline(word: &Value, fixture: &str, index: usize, text: &str) -> f64 {
    word_line(word, fixture, index, text)["y"].as_f64().unwrap()
}

/// Word's line texts on page `index`, in order.
pub fn word_texts(word: &Value, fixture: &str, index: usize) -> Vec<String> {
    word[fixture][index]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line["text"].as_str().unwrap().to_owned())
        .collect()
}
