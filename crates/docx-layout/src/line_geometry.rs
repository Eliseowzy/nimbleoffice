#[derive(Clone, Copy)]
pub(crate) struct HorizontalLineGeometry<'a> {
    pub frag_x: f64,
    pub frag_width: f64,
    pub indent_left: f64,
    pub indent_right: f64,
    pub hanging: f64,
    pub first_line: f64,
    pub is_first_line: bool,
    pub is_last_line: bool,
    pub is_rtl: bool,
    pub alignment: Option<&'a str>,
    pub has_list_marker: bool,
    pub left_offset: f64,
    pub right_offset: f64,
    pub marker_tab_offset: f64,
    pub marker_inline_width: f64,
    pub marker_font_size_pt: Option<f64>,
}

#[derive(Clone, Copy)]
pub(crate) struct MarkerGeometry {
    pub x: f64,
    pub width: f64,
}

pub(crate) struct HorizontalLineBox {
    pub echo_x: f64,
    pub pen_x: f64,
    pub usable_width: f64,
    pub marker: Option<MarkerGeometry>,
    pub left: f64,
    pub right: f64,
}

pub(crate) fn line_text_insets(
    indent_left: f64,
    hanging: f64,
    first_line: f64,
    is_first_line: bool,
    has_list_marker: bool,
    overrun: f64,
) -> (f64, f64) {
    if indent_left < 0.0 || hanging < 0.0 || first_line < 0.0 {
        let text_indent = if !is_first_line {
            0.0
        } else if has_list_marker && hanging > 0.0 {
            overrun
        } else {
            first_line - hanging
        };
        return (indent_left, text_indent);
    }
    let has_hanging = hanging > 0.0;
    let has_first_line = first_line > 0.0;
    let mut pad_left = indent_left;
    let mut text_indent = 0.0;
    if is_first_line {
        if indent_left > 0.0 && has_hanging {
            text_indent = if has_list_marker {
                (hanging - indent_left).max(0.0) + overrun
            } else {
                -hanging
            };
        } else if has_first_line {
            text_indent = first_line;
        } else if has_list_marker && has_hanging {
            text_indent = hanging;
        }
    } else if indent_left <= 0.0 && has_hanging {
        pad_left = hanging;
    }
    (pad_left, text_indent)
}

pub(crate) fn horizontal_line_box(
    geom: HorizontalLineGeometry<'_>,
    effective_line_width: f64,
) -> HorizontalLineBox {
    let has_hanging = geom.hanging > 0.0;
    let overrun = if geom.is_first_line
        && geom.has_list_marker
        && geom.indent_left > 0.0
        && has_hanging
        && !geom.is_rtl
    {
        geom.marker_tab_offset
    } else {
        0.0
    };
    let (pad_left, text_indent) = line_text_insets(
        geom.indent_left,
        geom.hanging,
        geom.first_line,
        geom.is_first_line,
        geom.has_list_marker,
        overrun,
    );
    let marker_inline =
        if geom.is_first_line && geom.has_list_marker && !has_hanging && !geom.is_rtl {
            geom.marker_inline_width.max(0.0)
        } else {
            0.0
        };
    let text_indent = text_indent + marker_inline;
    let usable_width = (geom.frag_width
        - pad_left
        - text_indent
        - geom.indent_right
        - geom.left_offset
        - geom.right_offset)
        .max(0.0);
    let align_shift = match geom.alignment {
        Some("center") => ((usable_width - effective_line_width) / 2.0).max(0.0),
        Some("right") => (usable_width - effective_line_width).max(0.0),
        None if geom.is_rtl => (usable_width - effective_line_width).max(0.0),
        _ => 0.0,
    };
    let align_shift = if geom.alignment == Some("justify") && !geom.is_last_line {
        0.0
    } else {
        align_shift
    };
    let echo_x = geom.frag_x + pad_left + text_indent + geom.left_offset;
    let pen_x = echo_x + align_shift;
    let marker = (geom.is_first_line && geom.has_list_marker).then(|| {
        let width = if marker_inline > 0.0 {
            marker_inline
        } else if overrun > 0.0 {
            geom.hanging + overrun
        } else {
            geom.hanging
                .max(geom.marker_font_size_pt.unwrap_or(11.0) * 96.0 / 72.0)
        };
        let x = if geom.is_rtl {
            pen_x + effective_line_width
        } else {
            pen_x - width
        };
        MarkerGeometry { x, width }
    });
    let mut left = pen_x;
    let mut right = pen_x + effective_line_width;
    if let Some(marker) = marker {
        left = left.min(marker.x);
        right = right.max(marker.x + marker.width);
    }
    HorizontalLineBox {
        echo_x,
        pen_x,
        usable_width,
        marker,
        left,
        right,
    }
}

fn is_rtl_strong(c: char) -> bool {
    matches!(u32::from(c),
        0x0590..=0x085F | 0x08A0..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF)
}

fn is_ltr_strong(c: char) -> bool {
    matches!(u32::from(c),
        0x0041..=0x005A | 0x0061..=0x007A | 0x00C0..=0x02B8 | 0x0370..=0x0589
        | 0x10A0..=0x10FF | 0x1E00..=0x1FFF)
}

pub(crate) fn runs_base_is_rtl<'a>(runs: impl Iterator<Item = (bool, &'a str)> + Clone) -> bool {
    if !runs.clone().any(|(rtl, _)| rtl) {
        return false;
    }
    for (_, text) in runs {
        for c in text.chars() {
            if is_rtl_strong(c) {
                return true;
            }
            if is_ltr_strong(c) {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry() -> HorizontalLineGeometry<'static> {
        HorizontalLineGeometry {
            frag_x: 100.0,
            frag_width: 500.0,
            indent_left: 0.0,
            indent_right: 0.0,
            hanging: 0.0,
            first_line: 0.0,
            is_first_line: true,
            is_last_line: false,
            is_rtl: false,
            alignment: None,
            has_list_marker: false,
            left_offset: 0.0,
            right_offset: 0.0,
            marker_tab_offset: 0.0,
            marker_inline_width: 0.0,
            marker_font_size_pt: None,
        }
    }

    #[test]
    fn offsets_and_first_line_indent_position_the_text_box() {
        let geom = HorizontalLineGeometry {
            indent_left: 40.0,
            indent_right: 20.0,
            first_line: 30.0,
            left_offset: 10.0,
            right_offset: 5.0,
            ..geometry()
        };
        let line = horizontal_line_box(geom, 120.0);
        assert_eq!(line.echo_x, 180.0);
        assert_eq!(line.pen_x, 180.0);
        assert_eq!(line.usable_width, 395.0);
        assert_eq!((line.left, line.right), (180.0, 300.0));
    }

    #[test]
    fn hanging_indent_applies_to_continuation_lines_without_a_marker() {
        let geom = HorizontalLineGeometry {
            hanging: 24.0,
            is_first_line: false,
            has_list_marker: true,
            ..geometry()
        };
        let line = horizontal_line_box(geom, 80.0);
        assert_eq!(line.echo_x, 124.0);
        assert_eq!((line.left, line.right), (124.0, 204.0));
        assert!(line.marker.is_none());
    }

    #[test]
    fn center_and_right_alignment_shift_the_same_text_box() {
        let centered = horizontal_line_box(
            HorizontalLineGeometry {
                frag_width: 300.0,
                alignment: Some("center"),
                ..geometry()
            },
            100.0,
        );
        let right = horizontal_line_box(
            HorizontalLineGeometry {
                frag_width: 300.0,
                alignment: Some("right"),
                ..geometry()
            },
            100.0,
        );
        assert_eq!((centered.left, centered.right), (200.0, 300.0));
        assert_eq!((right.left, right.right), (300.0, 400.0));
    }

    #[test]
    fn hanging_marker_overrun_extends_the_collision_box_leftward() {
        let geom = HorizontalLineGeometry {
            indent_left: 48.0,
            hanging: 24.0,
            has_list_marker: true,
            marker_tab_offset: 8.0,
            ..geometry()
        };
        let line = horizontal_line_box(geom, 80.0);
        assert_eq!(line.pen_x, 156.0);
        let marker = line.marker.unwrap();
        assert_eq!((marker.x, marker.width), (124.0, 32.0));
        assert_eq!((line.left, line.right), (124.0, 236.0));
    }

    #[test]
    fn inline_marker_is_part_of_the_horizontal_line_box() {
        let geom = HorizontalLineGeometry {
            indent_left: 36.0,
            has_list_marker: true,
            marker_inline_width: 20.0,
            ..geometry()
        };
        let line = horizontal_line_box(geom, 80.0);
        assert_eq!(line.pen_x, 156.0);
        let marker = line.marker.unwrap();
        assert_eq!((marker.x, marker.width), (136.0, 20.0));
        assert_eq!((line.left, line.right), (136.0, 236.0));
    }

    #[test]
    fn rtl_alignment_and_marker_extend_the_right_edge() {
        let geom = HorizontalLineGeometry {
            frag_width: 300.0,
            indent_left: 20.0,
            hanging: 16.0,
            is_rtl: true,
            has_list_marker: true,
            marker_font_size_pt: Some(12.0),
            ..geometry()
        };
        let line = horizontal_line_box(geom, 80.0);
        assert_eq!(line.pen_x, 320.0);
        let marker = line.marker.unwrap();
        assert_eq!((marker.x, marker.width), (400.0, 16.0));
        assert_eq!((line.left, line.right), (320.0, 416.0));
    }

    #[test]
    fn automatic_direction_follows_the_first_strong_character() {
        let runs = [(true, "123"), (false, "אבג")];
        assert!(runs_base_is_rtl(runs.into_iter()));
        let runs = [(true, "123"), (false, "abc")];
        assert!(!runs_base_is_rtl(runs.into_iter()));
        let runs = [(false, "אבג")];
        assert!(!runs_base_is_rtl(runs.into_iter()));
    }
}
