//! The decision half of floating-table (`w:tblpPr`) layout: given where the
//! table sits and what the page already holds, which of the entry branches of
//! `docs/floating-tables.md` §3 (B0–B5a) applies. [`plan`] is pure; the
//! execution (moving the pen, calling the row loop) stays in
//! [`crate::hooks::layout_floating_table`].

use crate::floating_objects::MIN_WRAP_SEGMENT_WIDTH;
use crate::types::WordLayoutEra;

/// Word's default footer distance (0.5in at 96dpi, px) when the section gives none.
const DEFAULT_FOOTER_DISTANCE: f64 = 47.25;

/// The band a floating table's `w:vertAnchor` measures from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VertAnchor {
    Page,
    Margin,
    Text,
}

impl VertAnchor {
    /// `w:vertAnchor`, absent = `text`; unknown values measure like `margin`.
    pub(crate) fn parse(value: Option<&str>) -> Self {
        match value.unwrap_or("text") {
            "page" => Self::Page,
            "text" => Self::Text,
            _ => Self::Margin,
        }
    }
}

/// `w:tblpYSpec`: how the spec places the table inside the vertical anchor
/// band. `inline` (and an absent spec) position nothing; unknown values
/// place like `top`, as the old inline match arm did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VertSpec {
    Top,
    Center,
    Bottom,
    /// `w:tblpYSpec="outside"` places like `bottom`.
    Outside,
    /// `w:tblpYSpec="inside"` places like `top`.
    Inside,
}

impl VertSpec {
    pub(crate) fn parse(value: Option<&str>) -> Option<Self> {
        Some(match value? {
            "center" => Self::Center,
            "bottom" => Self::Bottom,
            "outside" => Self::Outside,
            "inside" => Self::Inside,
            "top" => Self::Top,
            "inline" => return None,
            _ => Self::Top,
        })
    }

    /// The specs that fix the table's page position. §6.1 Word-measured on
    /// ycenter/ybottom (and the plan-fixes probes A/E): a `center`/`bottom`
    /// float taller than the room below the pen is backed off into the page
    /// and split; `top`/`inside` were not measured, so they keep the plain
    /// position.
    pub(crate) fn fixes_position(self) -> bool {
        matches!(self, Self::Center | Self::Bottom | Self::Outside)
    }
}

/// How far a `tblpYSpec`-positioned float's effective height falls short of
/// the body room it is aligned into: Word aligns the whole height first and
/// backs the top off by the overflow, landing `room - 4.8pt` (Word-measured
/// exactly on the 12pt matrix fixtures; the residual varies ±7pt across
/// other font sizes, which no fixture pins). Px.
const SPEC_BACK_OFF: f64 = 6.4;

/// An unsplit inline table already on the page (B1 checks the float against it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct InlineTableBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Everything [`plan`] decides from. Px throughout.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FloatInputs {
    pub era: WordLayoutEra,
    pub vertical: VertAnchor,
    /// `w:tblpY`, raw.
    pub tblp_y: Option<f64>,
    /// `w:tblpYSpec`, parsed.
    pub tblp_y_spec: Option<VertSpec>,
    /// `w:tblpXSpec` is `inside` or `outside` (flips with page parity).
    pub mirrored_x: bool,
    /// One column when the table entered layout (before any `ensure_fits`).
    pub single_column_at_entry: bool,
    /// The column height on the page the table entered on.
    pub entry_column_capacity: f64,
    /// One column on the page the table is positioned on.
    pub single_column: bool,
    pub table_width: f64,
    pub table_height: f64,
    /// The resolved position of the table's top-left corner.
    pub x: f64,
    pub y: f64,
    pub column_x: f64,
    pub column_width: f64,
    /// `w:leftFromText` / `w:rightFromText` / `w:bottomFromText`, non-finite = 0.
    pub left_from_text: f64,
    pub right_from_text: f64,
    pub bottom_from_text: f64,
    pub pen_y: f64,
    pub content_top: f64,
    pub content_limit: f64,
    pub page_height: f64,
    /// The section's footer distance, raw.
    pub footer_distance: Option<f64>,
    /// Whole, unsplit inline tables already on the page.
    pub inline_tables: Vec<InlineTableBox>,
}

/// Where a split floating table's first fragment stops (§3.1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Limit {
    /// The page's text limit, like an inline table.
    TextBottom,
    /// Word <= 2010's page-anchored deadline (px from the page top).
    LegacyDeadline(f64),
}

/// How continuation fragments break a row that does not fit (R6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContinuationRule {
    /// Break the row mid-content at the deepest line that fits.
    SplitMidRow,
    /// Move the row whole to the next page (legacy page floats).
    MoveRowsWhole,
}

/// How a full-width float that is not split clears the text around it (B5a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FullWidthBand {
    /// Try to reflow the page's text around the band, else push the pen past it.
    ClearOrPush,
    /// Push the pen past the table plus `w:bottomFromText`.
    Push,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum FloatPlan {
    /// Lay the table out as an inline table (B0, B4).
    Inline,
    /// Break the page and lay the float out again on the next one (B1, B2).
    MoveToNextPage,
    /// Fragment the table across pages from its position (B3).
    Split {
        first_limit: Limit,
        continuation: ContinuationRule,
        /// Drop the paragraph spacing before while splitting (§3.2).
        zero_space_before: bool,
        /// No text wraps beside it: the pen resumes below the last fragment.
        full_width: bool,
    },
    /// Place the table whole as one floating fragment at (x, y) (B5, B5a).
    Whole { full_width: Option<FullWidthBand> },
}

/// B0's test, which runs before the table is positioned: a table taller than
/// the column that cannot fragment in place falls back to inline layout.
/// `explicit_y` covers an explicit `w:tblpY >= 0` and the position-fixing
/// `w:tblpYSpec` values; a multi-column section never sends its floats
/// inline — B5 keeps them whole (§6.1 cols2) — and `inside`/`outside`
/// alignment no longer blocks in-place fragmenting (§6.1 outside: the
/// continuation keeps the first page's side).
pub(crate) fn falls_back_inline(explicit_y: bool, table_height: f64, column_capacity: f64) -> bool {
    table_height > column_capacity && !explicit_y
}

/// Picks the entry branch for a positioned floating table: the first branch of
/// `docs/floating-tables.md` §3 whose condition holds.
pub(crate) fn plan(inputs: &FloatInputs) -> FloatPlan {
    let i = inputs;
    let overflows_column = i.table_height > i.entry_column_capacity;
    let spec_fixed = i.tblp_y_spec.is_some_and(VertSpec::fixes_position);
    // B0: taller than the column and cannot fragment in place.
    if falls_back_inline(
        i.tblp_y.is_some_and(|y| y.is_finite() && y >= 0.0) || spec_fixed,
        i.table_height,
        i.entry_column_capacity,
    ) {
        return FloatPlan::Inline;
    }

    let explicit_y = i.tblp_y.is_some_and(f64::is_finite);
    let exclusion_left = i.x - i.left_from_text;
    let exclusion_right = i.x + i.table_width + i.right_from_text;
    let left_space = exclusion_left - i.column_x;
    let right_space = i.column_x + i.column_width - exclusion_right;
    let full_width = left_space < MIN_WRAP_SEGMENT_WIDTH && right_space < MIN_WRAP_SEGMENT_WIDTH;
    let bottom = i.y + i.table_height;

    // B1: a page float whose position collides with a whole, unsplit inline
    // table already on the page, with no room for both, moves to the next
    // page and takes the same position there. §6.1 fullwidth-over-inline
    // (Word-measured): for a float that is not full-width this holds only in
    // compatibilityMode >= 15 — compat <= 14 splits the float in place below
    // the inline table (B3) — and the move does not require full width: a
    // 234pt-wide float over the inline table moves the same way. A
    // full-width float keeps the move in both modes (no Word fixture pins
    // that combination, so the historical behavior stands).
    if (i.era == WordLayoutEra::Word2013 || full_width)
        && i.vertical == VertAnchor::Page
        && (explicit_y || spec_fixed)
        && i.single_column
        && i.y >= i.content_top
        && bottom <= i.content_limit
        && i.inline_tables.iter().any(|previous| {
            i.x < previous.x + previous.width
                && i.x + i.table_width > previous.x
                && i.y < previous.y + previous.height
                && bottom > previous.y
                && bottom + i.bottom_from_text.max(0.0) + previous.height > i.content_limit
        })
    {
        return FloatPlan::MoveToNextPage;
    }

    // B2: Word 2013+ (compatibilityMode >= 15) does not split a fixed-position
    // float — a non-text anchor with an explicit `w:tblpY`, or a
    // position-fixing `w:tblpYSpec` — whose position lies above body text
    // already on the page: the float moves to the next page and takes the
    // same position there, where it splits from. A float that fits whole, or
    // one positioned at or below the pen, keeps its page. Word-measured on
    // the table-split-bands compat-15 fixtures (tblpY 85pt under a title that
    // ends at 138pt opens page two at 85pt), margin-c15 (tblpY=0 under one
    // line of text moves; tblpY=600/1440 at or below the pen splits in
    // place) and ycenter-c15/ybottom-c15 (spec center moves, spec bottom
    // stays).
    if i.era == WordLayoutEra::Word2013
        && i.vertical != VertAnchor::Text
        && (explicit_y || spec_fixed)
        && i.single_column
        && i.y < i.pen_y
        && i.pen_y > i.content_top + 0.5
        && bottom > i.content_limit
    {
        return FloatPlan::MoveToNextPage;
    }

    // B3: split from the position. Word <= 2010 (compatibilityMode < 15,
    // absent = 12) lays a page-anchored float out with its whole page: text
    // before the anchor wraps beside it too, so the float keeps its position
    // even above the pen (the text that flowed there is re-measured around
    // it; see the table wrap frames), and a float above the pen whose whole
    // table would cross the deadline splits there even when it is shorter
    // than the column (§6.1 fullwidth-over-inline: three rows stay under the
    // inline table, the fourth moves on). `inside`/`outside` alignment no
    // longer blocks the split: the continuation keeps the first page's side
    // (§6.1 outside), which the row loop's fixed floating x already gives.
    let legacy_page = i.vertical == VertAnchor::Page && i.era == WordLayoutEra::Legacy;
    // A float above the pen only forces a split when its rectangle actually
    // crosses the column band the flow occupies — a float parked beside the
    // text (§positioned_table_collision: a page float at the far right stays
    // whole on its anchor page) keeps B5. Word-measured collisions that do
    // split: the fullwidth-over-inline and ycenter fixtures both overlap the
    // column.
    let column_overlap = i.x < i.column_x + i.column_width && i.x + i.table_width > i.column_x;
    let page_float_above_pen = legacy_page && !full_width && i.y >= i.content_top && column_overlap;
    // A legacy non-text float (page or margin anchor) splits at the deadline.
    // §6.1 margin-pretext: the margin anchor shares the page's body budget
    // like the page anchor, so it gets the same first-fragment deadline.
    let legacy_float = i.era == WordLayoutEra::Legacy && i.vertical != VertAnchor::Text;
    let legacy_deadline = || {
        let footer_distance = i
            .footer_distance
            .filter(|v| v.is_finite())
            .unwrap_or(DEFAULT_FOOTER_DISTANCE);
        let body_height = i.content_limit - i.content_top;
        let text_used = (i.pen_y - i.content_top).max(0.0);
        // A spec-positioned float (§6.1 ybottom, Word-measured with and
        // without a footer part) fills to the page bottom even past the
        // footer distance: rows reach 788.88pt on a 792pt page; a tblpY
        // float stops at the footer line.
        let footer_line = if spec_fixed {
            i.page_height
        } else {
            i.page_height - footer_distance
        };
        footer_line.min(i.y.max(i.content_top) + body_height - text_used)
    };
    if i.single_column
        && (i.vertical == VertAnchor::Text || overflows_column || page_float_above_pen)
        && (i.y >= i.pen_y || page_float_above_pen)
        && (bottom > i.content_limit || (page_float_above_pen && bottom > legacy_deadline()))
    {
        // The deadline a legacy split float's first fragment fills to: min
        // (footer line, fragment top + body height - the text the page
        // already holds). The float shares the page's body budget with the
        // text laid out before it, but may run into the bottom margin up to
        // the footer line. The top + body bound is LibreOffice's
        // GetFlyAnchorBottom (sw/source/core/layout/fly.cxx, "Word <= 2010").
        // Word-measured on the table-split fixtures (1 to 30 paragraphs before
        // the table: page one's last line always ends within one line of
        // top + body - text) and floattable-avoid-last-manip-ofst (the footer
        // line binds: row 3 completes at 563.76pt). Word 2013+ splits the
        // float like an inline table at the text limit
        // (floattable-footer-2rows breaks row 3 mid-content at the body
        // bottom). Continuation fragments start at the body top, where the
        // deadline is the body bottom either way.
        let (first_limit, continuation) = if legacy_float {
            (
                Limit::LegacyDeadline(legacy_deadline()),
                ContinuationRule::MoveRowsWhole,
            )
        } else {
            (Limit::TextBottom, ContinuationRule::SplitMidRow)
        };
        return FloatPlan::Split {
            first_limit,
            continuation,
            zero_space_before: !full_width && !(overflows_column && i.vertical == VertAnchor::Text),
            full_width,
        };
    }

    // B4: still taller than the column. A multi-column section never sends
    // its floats inline (§6.1 cols2): B5 keeps them whole, clipped at the
    // page bottom.
    if overflows_column && i.single_column {
        return FloatPlan::Inline;
    }

    // B5 / B5a: one floating fragment; a full-width one (B5a) reflows the
    // page's text around its band when page-anchored inside the body.
    let full_width = full_width.then(|| {
        let band_bottom = bottom + i.bottom_from_text;
        let reflowable = i.vertical == VertAnchor::Page
            && explicit_y
            && i.y >= i.content_top
            && band_bottom <= i.content_limit;
        if reflowable {
            FullWidthBand::ClearOrPush
        } else {
            FullWidthBand::Push
        }
    });
    FloatPlan::Whole { full_width }
}

/// Resolves a position-fixing `w:tblpYSpec` (§6.1 ycenter/ybottom, measured
/// on the matrix fixtures and the plan-fixes A/E probes) against the anchor
/// band `[v_start, v_end]`. A table no taller than `available` — the body
/// room below the pen — aligns by its whole height. A taller one first aligns
/// by its whole height, then the top is backed off by the overflow past the
/// room, which lands it at an effective height of `available - 4.8pt`
/// (`SPEC_BACK_OFF`): `bottom` then starts `room - 4.8` above `v_end`,
/// `center` splits the shortfall. `top`/`inside` keep the band's start.
pub(crate) fn spec_anchor_y(
    spec: VertSpec,
    v_start: f64,
    v_end: f64,
    height: f64,
    available: f64,
) -> f64 {
    let area = v_end - v_start;
    let effective = if height > available {
        (available - SPEC_BACK_OFF).clamp(0.0, height)
    } else {
        height
    };
    match spec {
        VertSpec::Center => v_start + (area - effective) / 2.0,
        VertSpec::Bottom | VertSpec::Outside => v_end - effective,
        VertSpec::Top | VertSpec::Inside => v_start,
    }
}

/// Clamps a whole (never split) float's y for a multi-column section (§6.1
/// cols2, measured on the plan-fixes D probes: 16 rows below tblpY 90 stay
/// at 90; 20 and 30 rows move up to the page top with their tail clipped
/// below the page bottom). Only a table that would end past the band bottom
/// moves: one that fits keeps its resolved y even above the band's start (a
/// negative `tblpY` text float stays above the anchor line).
pub(crate) fn whole_page_y(y: f64, v_start: f64, v_end: f64, height: f64) -> f64 {
    let bottom = v_end - height;
    if y > bottom { bottom.max(v_start) } else { y }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 200x100 table at (100, 300) on a 816x1056 page whose body runs
    /// 96..960 in one 624-wide column at x 96, pen at 200, compat 15.
    fn inputs() -> FloatInputs {
        FloatInputs {
            era: WordLayoutEra::Word2013,
            vertical: VertAnchor::Text,
            tblp_y: None,
            tblp_y_spec: None,
            mirrored_x: false,
            single_column_at_entry: true,
            entry_column_capacity: 864.0,
            single_column: true,
            table_width: 200.0,
            table_height: 100.0,
            x: 100.0,
            y: 300.0,
            column_x: 96.0,
            column_width: 624.0,
            left_from_text: 0.0,
            right_from_text: 0.0,
            bottom_from_text: 0.0,
            pen_y: 200.0,
            content_top: 96.0,
            content_limit: 960.0,
            page_height: 1056.0,
            footer_distance: None,
            inline_tables: Vec::new(),
        }
    }

    fn full_width(mut i: FloatInputs) -> FloatInputs {
        i.x = 96.0;
        i.table_width = 624.0;
        i
    }

    #[test]
    fn b0_tall_table_that_cannot_fragment_in_place_goes_inline() {
        let mut i = inputs();
        i.table_height = 900.0;
        i.tblp_y = None;
        assert_eq!(plan(&i), FloatPlan::Inline);
        assert!(falls_back_inline(false, 900.0, 864.0));
        // An explicit position (or a position-fixing tblpYSpec) lets the
        // table fragment in place, mirrored alignment included (§6.1
        // outside), and a multi-column section never goes inline (§6.1
        // cols2): both fall through to B3/B5 instead.
        assert!(!falls_back_inline(true, 900.0, 864.0));
        i.tblp_y = Some(20.0);
        i.mirrored_x = true;
        i.single_column_at_entry = false;
        assert_ne!(plan(&i), FloatPlan::Inline);
        assert!(!falls_back_inline(false, 100.0, 864.0));
    }

    #[test]
    fn b1_page_float_over_inline_table_moves_to_next_page_only_in_2013() {
        let mut i = full_width(inputs());
        i.vertical = VertAnchor::Page;
        i.tblp_y = Some(300.0);
        i.y = 300.0;
        i.inline_tables = vec![InlineTableBox {
            x: 96.0,
            y: 250.0,
            width: 624.0,
            height: 600.0,
        }];
        // §6.1 fullwidth-over-inline: only compatibilityMode >= 15 moves.
        assert_eq!(plan(&i), FloatPlan::MoveToNextPage);
        // A full-width float keeps the historical move in both modes (no Word
        // fixture pins that combination).
        i.era = WordLayoutEra::Legacy;
        assert_eq!(plan(&i), FloatPlan::MoveToNextPage);
        i.era = WordLayoutEra::Word2013;
        // Room for both: stays.
        i.inline_tables[0].height = 100.0;
        assert_eq!(
            plan(&i),
            FloatPlan::Whole {
                full_width: Some(FullWidthBand::ClearOrPush)
            }
        );
        // The move does not need a full-width float (§6.1: a 234pt float over
        // the inline table moves the same way).
        i.table_width = 200.0;
        i.x = 100.0;
        i.inline_tables[0].height = 600.0;
        assert_eq!(plan(&i), FloatPlan::MoveToNextPage);
    }

    #[test]
    fn b2_word2013_page_float_above_text_that_does_not_fit_moves_to_next_page() {
        let mut i = inputs();
        i.vertical = VertAnchor::Page;
        i.tblp_y = Some(120.0);
        i.y = 120.0;
        i.pen_y = 180.0;
        i.table_height = 870.0;
        assert_eq!(plan(&i), FloatPlan::MoveToNextPage);
        // Legacy keeps it on the page and splits it there (B3).
        i.era = WordLayoutEra::Legacy;
        assert!(matches!(plan(&i), FloatPlan::Split { .. }));
    }

    #[test]
    fn b2_margin_and_spec_floats_above_text_move_to_the_next_page_too() {
        // §6.1 margin-c15: a margin-anchored float above the pen moves (B
        // probe: tblpY=0 under one line of text moves; tblpY=1440 at the pen
        // splits in place).
        let mut i = inputs();
        i.vertical = VertAnchor::Margin;
        i.tblp_y = Some(0.0);
        i.y = 96.0;
        i.table_height = 870.0;
        assert_eq!(plan(&i), FloatPlan::MoveToNextPage);
        i.y = 200.0;
        i.tblp_y = Some(104.0);
        assert!(matches!(plan(&i), FloatPlan::Split { .. }));
        // §6.1 ycenter-c15: a spec-centered float above the pen moves;
        // ybottom-c15 (spec bottom, below the pen) splits in place.
        i.y = 100.0;
        i.tblp_y = None;
        i.tblp_y_spec = Some(VertSpec::Center);
        assert_eq!(plan(&i), FloatPlan::MoveToNextPage);
        i.tblp_y_spec = Some(VertSpec::Bottom);
        i.y = 300.0;
        assert!(matches!(plan(&i), FloatPlan::Split { .. }));
    }

    #[test]
    fn b3_text_float_past_the_bottom_splits_at_the_text_limit() {
        let mut i = inputs();
        i.table_height = 800.0;
        assert_eq!(
            plan(&i),
            FloatPlan::Split {
                first_limit: Limit::TextBottom,
                continuation: ContinuationRule::SplitMidRow,
                zero_space_before: true,
                full_width: false,
            }
        );
    }

    #[test]
    fn b3_mirrored_float_splits_and_keeps_its_side() {
        // §6.1 outside: an inside/outside float splits like any other fixed
        // position; the row loop's fixed floating x keeps the first page's
        // side on the continuation.
        let mut i = inputs();
        i.vertical = VertAnchor::Page;
        i.tblp_y = Some(20.0);
        i.y = 250.0;
        i.mirrored_x = true;
        i.table_height = 900.0;
        assert!(matches!(plan(&i), FloatPlan::Split { .. }));
        // B0 agrees: the mirrored float does not fall back inline.
        assert!(!falls_back_inline(true, 900.0, 864.0));
    }

    #[test]
    fn b3_legacy_page_float_splits_at_its_deadline_and_moves_rows_whole() {
        let mut i = inputs();
        i.era = WordLayoutEra::Legacy;
        i.vertical = VertAnchor::Page;
        i.tblp_y = Some(150.0);
        i.y = 150.0;
        i.table_height = 1000.0;
        i.entry_column_capacity = 864.0;
        // min(1056 - 47.25, 150 + 864 - (200 - 96)) = min(1008.75, 910)
        assert_eq!(
            plan(&i),
            FloatPlan::Split {
                first_limit: Limit::LegacyDeadline(910.0),
                continuation: ContinuationRule::MoveRowsWhole,
                zero_space_before: true,
                full_width: false,
            }
        );
        i.footer_distance = Some(200.0);
        assert!(matches!(
            plan(&i),
            FloatPlan::Split {
                first_limit: Limit::LegacyDeadline(856.0),
                ..
            }
        ));
    }

    #[test]
    fn b3_legacy_margin_float_shares_the_page_body_budget() {
        // §6.1 margin-pretext: the margin anchor splits at the legacy
        // deadline, not the text limit. With y = 250, pen 200: the deadline is
        // min(1056 - 47.25, 250 + 864 - 104) = 1008.75, past the text limit
        // 960, so the first fragment reaches into the bottom margin.
        let mut i = inputs();
        i.era = WordLayoutEra::Legacy;
        i.vertical = VertAnchor::Margin;
        i.tblp_y = Some(154.0);
        i.y = 250.0;
        i.table_height = 1000.0;
        assert_eq!(
            plan(&i),
            FloatPlan::Split {
                first_limit: Limit::LegacyDeadline(1008.75),
                continuation: ContinuationRule::MoveRowsWhole,
                zero_space_before: true,
                full_width: false,
            }
        );
    }

    #[test]
    fn b3_legacy_page_float_above_the_pen_splits_at_the_deadline_when_short() {
        // §6.1 fullwidth-over-inline, compat 14: a float above the pen that
        // is shorter than the column still splits when its whole table would
        // cross the deadline; one that stops above the deadline stays whole.
        let mut i = inputs();
        i.era = WordLayoutEra::Legacy;
        i.vertical = VertAnchor::Page;
        i.tblp_y = Some(150.0);
        i.y = 150.0;
        i.table_height = 400.0;
        i.pen_y = 750.0;
        // deadline = min(1008.75, 150 + 864 - 654) = 360; bottom 550 > 360.
        assert_eq!(
            plan(&i),
            FloatPlan::Split {
                first_limit: Limit::LegacyDeadline(360.0),
                continuation: ContinuationRule::MoveRowsWhole,
                zero_space_before: true,
                full_width: false,
            }
        );
        i.table_height = 100.0; // bottom 250 <= deadline 360: stays whole.
        assert_eq!(plan(&i), FloatPlan::Whole { full_width: None });
    }

    #[test]
    fn b3_spec_positioned_legacy_float_fills_to_the_page_bottom() {
        // §6.1 ybottom compat 14: the deadline's footer-line cap is the page
        // bottom for a spec-positioned float (Word-measured with and without
        // a footer part), while a tblpY float stops at the footer line.
        let mut i = inputs();
        i.era = WordLayoutEra::Legacy;
        i.vertical = VertAnchor::Page;
        i.tblp_y_spec = Some(VertSpec::Bottom);
        i.tblp_y = None;
        i.y = 300.0;
        i.pen_y = 96.0;
        i.table_height = 1000.0;
        // deadline = min(1056, 300 + 864 - 0) = 1056, past the footer line
        // 1008.75.
        assert_eq!(
            plan(&i),
            FloatPlan::Split {
                first_limit: Limit::LegacyDeadline(1056.0),
                continuation: ContinuationRule::MoveRowsWhole,
                zero_space_before: true,
                full_width: false,
            }
        );
    }

    #[test]
    fn b4_tall_float_that_does_not_split_goes_inline() {
        let mut i = inputs();
        i.vertical = VertAnchor::Margin;
        i.tblp_y = Some(10.0);
        i.y = 106.0;
        i.table_height = 900.0;
        i.era = WordLayoutEra::Legacy;
        // Legacy above the pen: no B2, no B3 (the margin clamp starts it at
        // the pen only when hooks applies it; here it stays above), B4.
        assert_eq!(plan(&i), FloatPlan::Inline);
    }

    #[test]
    fn b5_multicolumn_float_stays_whole_instead_of_going_inline() {
        // §6.1 cols2: a multi-column section never splits or inlines its
        // floats; the whole fragment is placed (hooks clamps y to the band).
        let mut i = inputs();
        i.vertical = VertAnchor::Page;
        i.tblp_y = Some(120.0);
        i.y = 120.0;
        i.table_height = 1000.0;
        i.single_column = false;
        i.single_column_at_entry = false;
        assert_eq!(plan(&i), FloatPlan::Whole { full_width: None });
    }

    #[test]
    fn spec_anchor_y_aligns_whole_height_then_backs_off_by_the_overflow() {
        // Fits: plain spec placement against the band.
        assert_eq!(
            spec_anchor_y(VertSpec::Bottom, 0.0, 1056.0, 500.0, 864.0),
            556.0
        );
        assert_eq!(
            spec_anchor_y(VertSpec::Center, 0.0, 1056.0, 500.0, 864.0),
            278.0
        );
        assert_eq!(spec_anchor_y(VertSpec::Top, 0.0, 1056.0, 500.0, 864.0), 0.0);
        // Taller than the room below the pen: the effective height is
        // available - 6.4px (4.8pt), so bottom = v_end - (room - 6.4) and
        // center splits the shortfall.
        let bottom_y = spec_anchor_y(VertSpec::Bottom, 0.0, 1056.0, 1113.6, 864.0);
        assert!((bottom_y - 198.4).abs() < 1e-9);
        let center_y = spec_anchor_y(VertSpec::Center, 0.0, 1056.0, 1113.6, 864.0);
        assert!((center_y - 99.2).abs() < 1e-9);
        // A margin band: v_start 96, v_end 960.
        let margin_y = spec_anchor_y(VertSpec::Bottom, 96.0, 960.0, 1113.6, 844.8);
        assert!((margin_y - 121.6).abs() < 1e-9);
    }

    #[test]
    fn whole_page_y_keeps_the_float_inside_the_band() {
        // §6.1 cols2 / D probes: below-tblpY 16 rows stay; 20 and 30 rows move
        // up so the table ends at the band bottom, never above the band top.
        assert_eq!(whole_page_y(120.0, 0.0, 1056.0, 900.0), 120.0);
        assert_eq!(whole_page_y(120.0, 0.0, 1056.0, 1674.0), 0.0);
        assert_eq!(whole_page_y(120.0, 0.0, 1056.0, 1000.0), 56.0);
    }

    #[test]
    fn b5_float_that_fits_is_placed_whole() {
        assert_eq!(plan(&inputs()), FloatPlan::Whole { full_width: None });
    }

    #[test]
    fn b5a_full_width_float_clears_its_band_or_pushes_the_pen() {
        let mut i = full_width(inputs());
        assert_eq!(
            plan(&i),
            FloatPlan::Whole {
                full_width: Some(FullWidthBand::Push)
            }
        );
        i.vertical = VertAnchor::Page;
        i.tblp_y = Some(300.0);
        assert_eq!(
            plan(&i),
            FloatPlan::Whole {
                full_width: Some(FullWidthBand::ClearOrPush)
            }
        );
        i.bottom_from_text = 600.0;
        assert_eq!(
            plan(&i),
            FloatPlan::Whole {
                full_width: Some(FullWidthBand::Push)
            }
        );
    }
}
