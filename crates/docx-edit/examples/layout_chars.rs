//! Lays a .docx out with the engine and prints every painted character's position, in
//! points from the page's top-left, for comparing with Word's PDF export.
//!
//!   cargo run -p nimbleoffice-docx-edit --example layout_chars -- IN.docx FONTS.json > chars.json
//!
//! FONTS.json maps engine font-chain keys ("family|bold|italic", lowercase family) to font
//! files (tools/word-parity/fonts.py builds it from Word's own fonts). A key with no exact
//! face falls back to the family's regular face, then to the face Word substitutes, then to
//! FALLBACK_KEY; every fallback is listed under "fontFallbacks" so a font difference is never
//! mistaken for a layout one.
//!
//! Output: {"pages": [{"width", "height", "chars": [{"t", "x", "y", "s"}]}],
//! "fontFallbacks": {key: used}}. Header and footer text is included.
use std::collections::HashMap;

use docx_edit::{EngineSession, seed_from_docx};
use serde_json::{Map, Value, json};

const FALLBACK_KEY: &str = "times new roman|0|0";
const PT_PER_PX: f64 = 0.75;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(docx), Some(fonts)) = (args.next(), args.next()) else {
        eprintln!("usage: layout_chars IN.docx FONTS.json");
        std::process::exit(2);
    };
    let bytes = std::fs::read(&docx).expect("read docx");
    let font_files: HashMap<String, String> =
        serde_json::from_str(&std::fs::read_to_string(&fonts).expect("read fonts"))
            .expect("fonts json");

    let package = docx_parse::parse_docx_s9_wire(&bytes, Default::default())
        .expect("parse docx")
        .document
        .package;
    let mut sections: Vec<Value> = package
        .document
        .sections
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|section| json!({"sectionId": section.id, "properties": section.properties}))
        .collect();
    let last = sections
        .last()
        .map(|section| section["properties"].clone())
        .unwrap_or_else(|| json!(package.document.final_section_properties));
    sections.push(json!({"properties": last}));
    let mut notes = Vec::new();
    for (list, kind) in [
        (&package.footnotes, "footnote"),
        (&package.endnotes, "endnote"),
    ] {
        for note in list.iter().flatten() {
            if note.note_type.is_empty() || note.note_type == "normal" {
                notes.push(json!({"id": note.id as i64, "noteKind": kind, "height": 0}));
            }
        }
    }

    // Word falls back to its own defaults (11pt, the UI language's family) when docDefaults carry no run properties and there is no theme.
    let word_default_family =
        std::env::var("WORD_PARITY_DEFAULT_FAMILY").unwrap_or("DengXian".into());
    let (mut render_env, default_family) = if package
        .styles
        .as_ref()
        .and_then(|styles| styles.doc_defaults.as_ref())
        .is_none_or(|defaults| defaults.r_pr.is_none())
        && package.theme == Default::default()
    {
        (
            json!({"docDefaultFontSize": 11}),
            word_default_family.as_str(),
        )
    } else {
        let styles = serde_json::to_value(&package.styles).unwrap_or_default();
        let names = docx_edit::layout_request::paragraph_style_names(
            styles["styles"].as_array().map_or(&[][..], Vec::as_slice),
        );
        (json!({"paragraphStyleNames": names}), "Calibri")
    };
    if package
        .settings
        .character_spacing_control
        .as_deref()
        .is_some_and(|value| value.starts_with("compressPunctuation"))
    {
        render_env["compressPunctuation"] = json!(true);
    }
    let engine = EngineSession::new(7);
    seed_from_docx(engine.doc(), &bytes).expect("seed docx");
    let mut request = json!({
        "bodyStory": "body",
        "renderEnv": render_env,
        "regions": {"sections": sections, "settings": package.settings},
        "notes": {"contents": notes},
    });
    // The app's host is JavaScript, whose JSON writes whole floats as integers; match it.
    request["measurement"] = json!({"defaults": {"fontSize": 11, "fontFamily": default_family}});
    integral_floats_to_ints(&mut request);
    let request_text = request.to_string();
    let requirements: Vec<Value> = match engine.layout_font_requirements_json(&request_text) {
        Ok(text) => serde_json::from_str(&text).expect("requirements json"),
        Err(e) => {
            let message = format!("{e:?}");
            // point at the request field the engine refused
            let column = message
                .rsplit("column ")
                .next()
                .and_then(|c| {
                    c.trim_end_matches(|ch: char| !ch.is_ascii_digit())
                        .parse::<usize>()
                        .ok()
                })
                .unwrap_or(0);
            let from = column.saturating_sub(160).min(request_text.len());
            let to = (column + 40).min(request_text.len());
            panic!(
                "font requirements: {message}\n  near: {}",
                &request_text[from..to]
            );
        }
    };

    let mut registered: HashMap<String, u32> = HashMap::new();
    let mut register = |path: &str| -> u32 {
        if let Some(id) = registered.get(path) {
            return *id;
        }
        let data = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
        let id = docx_layout::register_measure_font(&data)
            .unwrap_or_else(|e| panic!("register {path}: {e:?}"));
        registered.insert(path.to_owned(), id);
        id
    };
    let mut chains = Map::new();
    let mut fallbacks = Map::new();
    for requirement in &requirements {
        let key = requirement["key"]
            .as_str()
            .expect("requirement key")
            .to_owned();
        let family = key.split('|').next().unwrap_or_default();
        let regular = format!("{family}|0|0");
        let substitute = word_substitute(family).map(|name| format!("{name}|0|0"));
        let (used, path) = [key.as_str(), regular.as_str()]
            .into_iter()
            .chain(substitute.as_deref())
            .chain([FALLBACK_KEY])
            .find_map(|candidate| {
                font_files
                    .get(candidate)
                    .map(|path| (candidate.to_owned(), path.clone()))
            })
            .expect("fallback font missing from FONTS.json");
        if used != key {
            fallbacks.insert(key.clone(), json!(used));
        }
        chains.insert(key, json!([register(&path)]));
    }
    request["measurement"] = json!({
        "fontChains": chains,
        "defaults": {"fontSize": 11, "fontFamily": default_family},
        "authoritativeShaping": true,
    });

    let layout = engine
        .layout_document_with_regions_json(&request.to_string())
        .expect("layout");
    let mut layout_value: Value = serde_json::from_str(&layout).expect("layout json");
    let mut leader_chains = request["measurement"]["fontChains"].clone();
    leader_chains["@nimbleoffice/fonts/leader"] = json!([]);
    layout_value["fontChains"] = leader_chains;
    let layout = layout_value.to_string();
    let display: Value = serde_json::from_str(
        &engine
            .build_display_list_json(&layout)
            .expect("display list"),
    )
    .expect("display json");

    if let Ok(path) = std::env::var("LAYOUT_CHARS_LAYOUT") {
        std::fs::write(path, &layout).expect("write layout dump");
    }
    if let Ok(path) = std::env::var("LAYOUT_CHARS_DUMP") {
        std::fs::write(path, display.to_string()).expect("write display dump");
    }
    let mut pages = Vec::new();
    for page in display["pages"].as_array().into_iter().flatten() {
        let mut chars = Vec::new();
        let mut regions = vec![&page["primitives"]];
        for hf in ["header", "footer"] {
            regions.push(&page[hf]["primitives"]);
        }
        for primitives in regions {
            for primitive in primitives.as_array().into_iter().flatten() {
                collect(primitive, &mut chars);
            }
        }
        pages.push(json!({
            "width": page["width"].as_f64().unwrap_or(0.0) * PT_PER_PX,
            "height": page["height"].as_f64().unwrap_or(0.0) * PT_PER_PX,
            "chars": chars,
        }));
    }
    println!("{}", json!({"pages": pages, "fontFallbacks": fallbacks}));
}

/// The face Word for Mac substitutes for an uninstalled Chinese font, as seen in its PDFs.
fn word_substitute(family: &str) -> Option<&str> {
    if let Some(base) = family.strip_suffix("_gb2312") {
        return Some(base);
    }
    family.ends_with("_gbk").then_some("microsoft yahei")
}

/// Appends the characters a text or glyph-run primitive paints; containers recurse.
fn collect(primitive: &Value, chars: &mut Vec<Value>) {
    let pt = |v: &Value| (v.as_f64().unwrap_or(0.0) * PT_PER_PX * 100.0).round() / 100.0;
    if primitive["hidden"].as_bool() == Some(true) {
        return;
    }
    // A row split across pages paints its lines on both pages, clipped to each page's
    // part; only what lands inside the clip is visible.
    let clip = &primitive["paintClip"];
    let visible = |y_px: f64| match (clip["y"].as_f64(), clip["h"].as_f64()) {
        (Some(top), Some(h)) => y_px >= top && y_px <= top + h,
        _ => true,
    };
    match primitive["kind"].as_str() {
        Some("text") => {
            let size = css_size_px(primitive["font"].as_str().unwrap_or_default()) * PT_PER_PX;
            // One entry per run; x is the pen origin. Text primitives carry no per-glyph x.
            let text = primitive["text"].as_str().unwrap_or_default();
            let text = if primitive["allCaps"].as_bool() == Some(true) {
                text.to_uppercase()
            } else {
                text.to_owned()
            };
            if !text.trim().is_empty() && visible(primitive["baselineY"].as_f64().unwrap_or(0.0)) {
                chars.push(
                    json!({"t": text, "x": pt(&primitive["x"]), "y": pt(&primitive["baselineY"]),
                                  "w": pt(&primitive["width"]), "s": size}),
                );
            }
        }
        Some("glyphRun") => {
            let text: Vec<char> = primitive["text"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .collect();
            let size = primitive["size"].as_f64().unwrap_or(0.0) * PT_PER_PX;
            let glyphs = primitive["glyphs"].as_array().cloned().unwrap_or_default();
            let mut seen = std::collections::HashSet::new();
            for glyph in glyphs {
                let cluster = glyph["cluster"].as_u64().unwrap_or(0) as usize;
                if !seen.insert(cluster) {
                    continue;
                }
                // clusters index UTF-16 code units; BMP text maps one to one
                let Some(ch) = text.get(cluster) else {
                    continue;
                };
                if ch.is_whitespace() || !visible(glyph["y"].as_f64().unwrap_or(0.0)) {
                    continue;
                }
                chars.push(json!({"t": ch.to_string(), "x": pt(&glyph["x"]), "y": pt(&glyph["y"]), "s": size}));
            }
        }
        _ => {
            for key in ["primitives", "children", "content"] {
                for child in primitive[key].as_array().into_iter().flatten() {
                    collect(child, chars);
                }
            }
        }
    }
}

fn css_size_px(font: &str) -> f64 {
    font.split_whitespace()
        .find_map(|part| part.strip_suffix("px").and_then(|n| n.parse::<f64>().ok()))
        .unwrap_or(16.0)
}

/// Rewrites every whole-valued float (1.0) as an integer (1), as JavaScript's JSON does.
fn integral_floats_to_ints(value: &mut Value) {
    match value {
        Value::Number(n) => {
            if let Some(f) = n.as_f64()
                && n.is_f64()
                && f.fract() == 0.0
                && f.abs() < 9.0e15
            {
                *value = json!(f as i64);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(integral_floats_to_ints),
        Value::Object(map) => map.values_mut().for_each(integral_floats_to_ints),
        _ => {}
    }
}
