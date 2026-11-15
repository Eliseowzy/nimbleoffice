use super::*;
use crate::bridge::ShapeStyles;

fn styles() -> ShapeStyles {
    let definitions = json!({
        "docDefaults": {"rPr": {"fontSize": 22}},
        "styles": [
            {"styleId": "Normal", "type": "paragraph", "default": true,
             "pPr": {"lineSpacing": 300, "lineSpacingRule": "auto"},
             "rPr": {"fontSize": 24}},
            {"styleId": "Small", "type": "paragraph", "based": "Normal",
             "rPr": {"fontSize": 18}},
            {"styleId": "Big", "type": "character", "rPr": {"fontSize": 40}}
        ]
    });
    ShapeStyles::new(Some(crate::seed::StyleResolver::new(Some(&definitions))))
}

fn lowered(paragraph: Value) -> ParagraphBlock {
    let env = RenderEnv {
        shape_styles: styles(),
        ..RenderEnv::default()
    };
    let shape = json!({"shapeType": "textBox", "textBody": {"content": [paragraph]}});
    lower_shape_json(&shape, 1, &env)
        .unwrap()
        .inner_text
        .unwrap()
        .remove(0)
}

fn first_size(block: &ParagraphBlock) -> Option<f64> {
    match &block.runs[0] {
        Run::Text(run) => run.fmt.font_size,
        _ => None,
    }
}

#[test]
fn a_run_without_properties_takes_the_default_paragraph_style_size() {
    let block = lowered(json!({"type": "paragraph", "content": [
        {"type": "run", "content": [{"type": "text", "text": "x"}]}
    ]}));
    assert_eq!(first_size(&block), Some(12.0));
}

#[test]
fn paragraph_and_character_styles_apply_inside_a_text_box() {
    let small = lowered(
        json!({"type": "paragraph", "formatting": {"styleId": "Small"}, "content": [
            {"type": "run", "content": [{"type": "text", "text": "x"}]}
        ]}),
    );
    assert_eq!(first_size(&small), Some(9.0));
    let big = lowered(json!({"type": "paragraph", "content": [
        {"type": "run", "formatting": {"styleId": "Big"}, "content": [{"type": "text", "text": "x"}]}
    ]}));
    assert_eq!(first_size(&big), Some(20.0));
    let direct = lowered(
        json!({"type": "paragraph", "formatting": {"styleId": "Small"}, "content": [
            {"type": "run", "formatting": {"fontSize": 28}, "content": [{"type": "text", "text": "x"}]}
        ]}),
    );
    assert_eq!(first_size(&direct), Some(14.0));
}

#[test]
fn the_default_paragraph_style_line_spacing_reaches_the_text_box_paragraph() {
    let styled = lowered(json!({"type": "paragraph", "content": [
        {"type": "run", "content": [{"type": "text", "text": "x"}]}
    ]}));
    let plain = {
        let shape = json!({"shapeType": "textBox", "textBody": {"content": [
            {"type": "paragraph", "content": [{"type": "run", "content": [{"type": "text", "text": "x"}]}]}
        ]}});
        lower_shape_json(&shape, 1, &RenderEnv::default())
            .unwrap()
            .inner_text
            .unwrap()
            .remove(0)
    };
    assert_ne!(
        styled.attrs.as_ref().unwrap().spacing,
        plain.attrs.as_ref().unwrap().spacing
    );
}

#[test]
fn auto_space_before_is_dropped_on_the_first_paragraph_only() {
    let env = RenderEnv {
        compress_punctuation: true,
        ..RenderEnv::default()
    };
    let paragraph = json!({"type": "paragraph",
        "formatting": {"beforeAutospacing": true, "afterAutospacing": true},
        "content": [{"type": "run", "content": [{"type": "text", "text": "x"}]}]});
    let shape = json!({"shapeType": "textBox",
        "textBody": {"content": [paragraph.clone(), paragraph]}});
    let blocks = lower_shape_json(&shape, 1, &env)
        .unwrap()
        .inner_text
        .unwrap();
    let before = |block: &ParagraphBlock| {
        block
            .attrs
            .as_ref()
            .and_then(|attrs| attrs.spacing.as_ref())
            .and_then(|spacing| spacing.before)
    };
    assert_eq!(before(&blocks[0]), Some(0.0));
    assert_ne!(before(&blocks[1]), Some(0.0));
    assert_eq!(
        blocks[0].attrs.as_ref().unwrap().compress_punctuation,
        Some(true)
    );
}
