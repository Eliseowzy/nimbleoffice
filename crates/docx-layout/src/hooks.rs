//! Feature seams the placement walk calls for per-block pagination decisions.
//!
//! Break policy, keep-with-next, section breaks and column balancing delegate
//! to their owning modules; table placement lives here because it is driven
//! entirely by paginator state. Every entry point returns a `Result` so a
//! feature the engine cannot paginate surfaces `LayoutError::Unsupported` to
//! the caller rather than producing wrong geometry.

use crate::LayoutError;
use crate::cell_layout::table_compat_x_adjust;
use crate::float_plan::{
    self, ContinuationRule, FloatInputs, FloatPlan, FullWidthBand, InlineTableBox, Limit,
    VertAnchor,
};
use crate::page_flow::Paginator;
use crate::prescan::SectionLayoutConfig;
use crate::table_row_break::{
    RowBreaks, first_table_fragment_height, minimum_row_slice, snap_row_break,
};
use crate::types::{
    Fragment, LayoutBlock, MeasuredBlock, SectionBreakBlock, SectionBreakType, TableBlock,
    TableExtent, TableFragment, WordLayoutEra,
};
use crate::{break_policy, column_balancing, keep_together, section_breaks};

fn unsupported(feature: &str) -> LayoutError {
    LayoutError::Unsupported(feature.to_string())
}

// the scan types live with their producer; re-exported so the spine's
// `prescan`/`place` keep importing them from the hooks seam
pub use crate::keep_together::{KeepWithNextGroup, KeepWithNextScan};

pub fn breaks_before_block(
    block: &LayoutBlock,
) -> Result<Option<break_policy::AuthoredBreak>, LayoutError> {
    Ok(break_policy::breaks_before_block(block))
}

pub fn analyze_keep_with_next(measured: &[MeasuredBlock]) -> Result<KeepWithNextScan, LayoutError> {
    Ok(keep_together::analyze_keep_with_next(measured))
}

pub fn measure_keep_with_next_group(
    group: &KeepWithNextGroup,
    measured: &[MeasuredBlock],
) -> Result<f64, LayoutError> {
    Ok(keep_together::measure_keep_with_next_group(group, measured))
}

pub fn measure_keep_with_next_group_at(
    group: &KeepWithNextGroup,
    measured: &[MeasuredBlock],
    leading: impl Fn(f64) -> f64,
    deferred: f64,
    capacity: f64,
) -> Result<f64, LayoutError> {
    Ok(keep_together::measure_keep_with_next_group_at(
        group, measured, leading, deferred, capacity,
    ))
}

pub fn keep_with_next_group_must_advance(
    group_height: f64,
    available_height: f64,
    page_content_height: f64,
    page_has_content: bool,
) -> Result<bool, LayoutError> {
    Ok(break_policy::keep_with_next_group_must_advance(
        break_policy::KeepWithNextFit {
            group_height,
            available_height,
            page_content_height,
            page_has_content,
        },
    ))
}

pub fn keep_with_next_group_must_advance_from(
    group_height: f64,
    fresh_page_height: f64,
    available_height: f64,
    page_content_height: f64,
    page_has_content: bool,
) -> Result<bool, LayoutError> {
    Ok(break_policy::keep_with_next_group_must_advance_from(
        break_policy::KeepWithNextFit {
            group_height,
            available_height,
            page_content_height,
            page_has_content,
        },
        fresh_page_height,
    ))
}

pub fn handle_section_break(
    block: &SectionBreakBlock,
    paginator: &mut Paginator,
    next_section_config: &SectionLayoutConfig,
    next_section_type: Option<SectionBreakType>,
    parity_restart: Option<u64>,
) -> Result<bool, LayoutError> {
    section_breaks::handle_section_break(
        block,
        paginator,
        next_section_config,
        next_section_type,
        parity_restart,
    )
}

pub fn balance_terminal_continuous_text_columns(
    measured: &[MeasuredBlock],
    paginator: &mut Paginator,
    start: usize,
    end: usize,
) -> Result<(), LayoutError> {
    column_balancing::balance_terminal_continuous_text_columns(measured, paginator, start, end);
    Ok(())
}

pub(crate) fn balance_terminal_continuous_text_columns_with_marks(
    measured: &[MeasuredBlock],
    paginator: &mut Paginator,
    start: usize,
    end: usize,
    section_marks: &[bool],
) -> Result<(), LayoutError> {
    column_balancing::balance_terminal_continuous_text_columns_with_marks(
        measured,
        paginator,
        start,
        end,
        section_marks,
    );
    Ok(())
}

/// The boundary band a page break redraws at a cut through `row`: the row's
/// contiguous top band (zero in the half-share model, which carries none).
fn row_cut_band(measure: &TableExtent, row: usize) -> f64 {
    measure
        .rows
        .get(row)
        .into_iter()
        .flat_map(|row| row.cells.iter())
        .filter_map(|cell| cell.top_band)
        .fold(0.0, f64::max)
}

/// The band closing `row` from below when a fragment ends with it: the next
/// row's boundary band, or the table's bottom band after the last row.
fn trailing_band(measure: &TableExtent, row: usize) -> f64 {
    if row + 1 < measure.rows.len() {
        row_cut_band(measure, row + 1)
    } else {
        measure.bottom_band.unwrap_or(0.0)
    }
}

/// Length of the leading run of `w:tblHeader` rows, the band that repeats on
/// continuation fragments. Header rows after a non-header row do not count.
fn tally_header_rows(block: &TableBlock) -> usize {
    let mut count = 0usize;
    for row in &block.rows {
        if row.is_header.unwrap_or(false) {
            count += 1;
        } else {
            break;
        }
    }
    count
}

/// Vertical cost a continuation fragment pays to repeat the header band.
fn get_header_rows_height(measure: &TableExtent, header_row_count: usize) -> f64 {
    let mut height = 0.0f64;
    let mut i = 0usize;
    while i < header_row_count && i < measure.rows.len() {
        height += measure.rows[i].height;
        i += 1;
    }
    height
}

/// Per row that starts a keep-with-next row chain (a row whose first cell
/// ends in a keepNext paragraph, as Word decides a row's keep with next,
/// not inside a longer chain): the height of the chain's rows and the row
/// that follows it.
pub(crate) fn row_keep_chains(
    block: &TableBlock,
    measure: &TableExtent,
) -> Vec<Option<(f64, usize)>> {
    let rows = measure.rows.len();
    let keeps_next = |index: usize| {
        index + 1 < rows
            && block.rows.get(index).is_some_and(|row| {
                row.cells.first().is_some_and(|cell| {
                    matches!(cell.blocks.last(), Some(LayoutBlock::Paragraph(paragraph))
                        if paragraph.attrs.as_ref().and_then(|attrs| attrs.keep_next) == Some(true))
                })
            })
    };
    let mut chains = vec![None; rows];
    let mut index = 0;
    while index < rows {
        if !keeps_next(index) {
            index += 1;
            continue;
        }
        let head = index;
        let mut height = 0.0;
        while keeps_next(index) {
            height += measure.rows[index].height;
            index += 1;
        }
        chains[head] = Some((height, index));
        index += 1;
    }
    chains
}

/// The height a keep-with-next row chain keeps on one page in a column
/// `capacity` tall: its rows and the smallest slice of the row after it, as
/// Word keeps a row with the next row's start, or that whole row when its
/// paragraph rules leave no break in such a column. Where every-cell starts
/// apply, the start of a row from `body_row` on is measured in a column
/// `body_capacity` tall.
pub(crate) fn row_keep_height(
    chain: Option<(f64, usize)>,
    block: &TableBlock,
    measure: &TableExtent,
    breaks: &RowBreaks,
    capacity: f64,
    body_row: usize,
    body_capacity: f64,
    limited_by_float: bool,
) -> f64 {
    let Some((rows, follower)) = chain else {
        return 0.0;
    };
    let mut slice = minimum_row_slice(block, measure, &breaks.kept, follower, 0.0);
    if !limited_by_float {
        slice = slice.min(
            breaks
                .first_cell_slice(
                    follower,
                    0.0,
                    if follower >= body_row && breaks.every_cell_starts() {
                        body_capacity
                    } else {
                        capacity
                    },
                )
                .unwrap_or(slice),
        );
    }
    rows + if slice <= capacity {
        slice
    } else {
        measure.rows[follower].height
    }
}

/// Fits `height` at the cursor and reports whether that moved it. A retry that
/// leaves the cursor in place (rounding can make a height that failed the
/// caller's check pass `ensure_fits`) places there instead of looping.
fn fit_moved_cursor(paginator: &mut Paginator, height: f64) -> bool {
    let before = paginator.get_current();
    let (column, pen_y) = {
        let state = paginator.state(before);
        (state.column_index, state.pen_y)
    };
    let after = paginator.ensure_fits(height);
    let state = paginator.state(after);
    after != before || state.column_index != column || state.pen_y > pen_y
}

/// Places an in-flow table, emitting one fragment per page or column it spans.
///
/// The cursor is `(row_index, consumed)`, where `consumed` is how many pixels
/// of `row_index` a previous fragment already placed. Rows go in order; a row
/// that overruns the remaining space breaks at the deepest whole-line boundary
/// that fits (Word's "allow row to break across pages"), which keeps the row's
/// other columns on the page where they start and lets a tall vertically merged
/// cell flow across the boundary. `w:cantSplit` rows (§17.4.6) break only when
/// taller than a fresh column's body room. `w:trHeight w:hRule="exact"`
/// rows are likewise atomic: their break geometry offers only the full-height
/// boundary, so the whole row moves to the next page rather than slicing
/// mid-row. A fresh fragment where not one line fits places the row's remainder
/// with overflow instead of looping.
///
/// A continuation fragment repeats the leading header band, but only when the
/// band plus the smallest legal body slice still fits the column; otherwise it
/// pays no header overhead. Deferred spacing from the preceding block is
/// consumed by the first fragment only.
pub fn layout_table(
    block: &TableBlock,
    measure: &TableExtent,
    paginator: &mut Paginator,
) -> Result<(), LayoutError> {
    layout_table_with_position(block, measure, paginator, None)
}

fn layout_table_with_position(
    block: &TableBlock,
    measure: &TableExtent,
    paginator: &mut Paginator,
    floating_x: Option<f64>,
) -> Result<(), LayoutError> {
    layout_table_with_position_and_limit(
        block,
        measure,
        paginator,
        floating_x,
        Limit::TextBottom,
        ContinuationRule::SplitMidRow,
    )
}

/// Lays a table's rows out as fragments. `first_limit` is where the first
/// fragment stops: a legacy page-anchored split floating table fills it to
/// its deadline (R1, see `float_plan::plan`), splitting rows mid-content
/// there; every other table keeps the text limit. `continuation` says whether
/// later fragments break a row that does not fit mid-content or move it whole
/// (R6).
fn layout_table_with_position_and_limit(
    block: &TableBlock,
    measure: &TableExtent,
    paginator: &mut Paginator,
    floating_x: Option<f64>,
    first_limit: Limit,
    continuation: ContinuationRule,
) -> Result<(), LayoutError> {
    let rows = &measure.rows;
    if rows.is_empty() {
        return Ok(());
    }

    let header_row_count = tally_header_rows(block);
    let header_rows_height = get_header_rows_height(measure, header_row_count);
    let breaks = RowBreaks::new(block, measure);
    let first_fragment_height = first_table_fragment_height(block, measure, &breaks.kept);
    let keep_chains = row_keep_chains(block, measure);

    let mut row_index = 0usize;
    let mut consumed = 0.0f64; // px of rows[row_index] already placed on a previous fragment
    let mut cell_tops: Option<Vec<f64>> = None;
    // (state, original limit) pairs widened to the float's page limit.
    let mut extended_limits: Vec<(usize, f64)> = Vec::new();
    // The deadline belongs to the page the float is anchored on: once the
    // first row has moved on (nothing fits under the deadline), a fresh page
    // is a continuation fragment and keeps the text limit — re-applying the
    // deadline there would strand an empty page between the two.
    let mut deadline_extended = false;

    'rows: while row_index < rows.len() {
        breaks.set_every_cell_starts(
            !paginator.has_float_bands() && !paginator.next_page_may_be_shorter(),
        );
        let state_idx = paginator.get_current();
        // A legacy page-anchored split float's first fragment fills to its
        // deadline (wider or narrower than the text limit); the limit is
        // restored on exit so the body flow keeps the text limit.
        if !deadline_extended
            && let Limit::LegacyDeadline(limit) = first_limit
            && row_index == 0
            && consumed == 0.0
        {
            deadline_extended = true;
            if paginator.state(state_idx).content_limit != limit {
                extended_limits.push((state_idx, paginator.state(state_idx).content_limit));
                paginator.set_content_limit(state_idx, limit);
            }
        }
        let available_height = paginator.get_available_height();
        let limited_by_float = available_height
            < paginator.state(state_idx).content_limit - paginator.state(state_idx).pen_y;
        let is_first_fragment = row_index == 0 && consumed == 0.0;
        // The tallest stretch a fresh column offers between float bands.
        let column_capacity = paginator.get_column_capacity();
        let body_capacity = if header_row_count > 0 && header_rows_height <= column_capacity {
            column_capacity - header_rows_height
        } else {
            column_capacity
        };
        let row_remaining_at_start = cell_tops
            .as_deref()
            .map_or(rows[row_index].height - consumed, |tops| {
                breaks.cell_remaining(row_index, tops)
            });
        let row_cant_split = block
            .rows
            .get(row_index)
            .and_then(|row| row.cant_split)
            .unwrap_or(false);
        // An exact-height row is atomic (its break geometry holds only the
        // full-height boundary), so like `cantSplit` it must move whole to the
        // next page when it fits there but not in the remaining space. A row
        // taller than the whole column still progresses: `ensure_fits`
        // advances at most once for oversized heights and the fresh-fragment
        // overflow guard below then places it rather than looping.
        let row_is_exact = block
            .rows
            .get(row_index)
            .is_some_and(|row| row.is_exact_height());
        if (row_cant_split || row_is_exact)
            && consumed == 0.0
            && row_remaining_at_start > available_height
            && paginator.state(state_idx).pen_y != paginator.state(state_idx).content_top
            && fit_moved_cursor(paginator, row_remaining_at_start)
        {
            continue;
        }

        // Only the first fragment consumes deferred spacing.
        let pending_spacing = if is_first_fragment {
            paginator.state(state_idx).deferred_spacing
        } else {
            0.0
        };
        // Without cantSplit, a first body row whose paragraph rules leave no break
        // below the header band starts with the band at its first line, as main does.
        let first_body_kept_oversized = is_first_fragment
            && header_row_count > 0
            && header_row_count < rows.len()
            && !block
                .rows
                .get(header_row_count)
                .is_some_and(|row| row.cant_split.unwrap_or(false))
            && breaks.kept_oversized(header_row_count, 0.0, body_capacity);
        let header_budget = available_height - pending_spacing - header_rows_height;
        let cell_header_slice = if is_first_fragment && header_row_count > 0 && !limited_by_float {
            breaks
                .cell_slice(
                    header_row_count,
                    0.0,
                    None,
                    header_budget,
                    snap_row_break(&breaks.kept, header_row_count, 0.0, header_budget),
                    false,
                    body_capacity,
                )
                .filter(|slice| {
                    !slice.starved
                        || breaks
                            .every_cell_start(header_row_count, 0.0, body_capacity)
                            .is_none()
                })
        } else {
            None
        };
        let shared_start_height = if first_body_kept_oversized {
            header_rows_height + breaks.fresh_slice(header_row_count, 0.0, body_capacity)
        } else if first_fragment_height <= column_capacity {
            first_fragment_height
        } else {
            header_rows_height
        };
        // The per-cell start only replaces a shared start that would move the table,
        // and still holds every keep-with-next chain the row loop would hold.
        let header_start_height = match cell_header_slice {
            Some(slice) if shared_start_height + pending_spacing > available_height => {
                let tops: Vec<_> = slice.clips.iter().map(|clip| clip.top).collect();
                let mut height = header_rows_height
                    + breaks.cell_fresh_slice(header_row_count, &tops, body_capacity);
                let mut top = 0.0;
                for (index, chain) in keep_chains.iter().enumerate().take(header_row_count + 1) {
                    let room = if index >= header_row_count {
                        body_capacity
                    } else {
                        column_capacity
                    };
                    let keep = row_keep_height(
                        *chain,
                        block,
                        measure,
                        &breaks,
                        room,
                        header_row_count,
                        body_capacity,
                        limited_by_float,
                    );
                    if keep > 0.0 && keep <= room {
                        height = height.max(top + keep);
                    }
                    top += rows[index].height;
                }
                height
            }
            _ => shared_start_height,
        };
        if is_first_fragment
            && header_row_count > 0
            && (header_start_height <= column_capacity || first_body_kept_oversized)
            && header_start_height + pending_spacing > available_height
            && paginator.state(state_idx).pen_y != paginator.state(state_idx).content_top
            && fit_moved_cursor(paginator, header_start_height + pending_spacing)
        {
            continue;
        }
        let mut minimum_body_slice = cell_tops.as_deref().map_or_else(
            || breaks.fresh_slice(row_index, consumed, body_capacity),
            |tops| breaks.cell_fresh_slice(row_index, tops, body_capacity),
        );
        if !is_first_fragment
            && cell_tops.is_none()
            && !limited_by_float
            && header_row_count > 0
            && let Some(slice) = breaks.cell_slice(
                row_index,
                consumed,
                None,
                header_budget,
                snap_row_break(&breaks.kept, row_index, consumed, header_budget),
                false,
                body_capacity,
            )
        {
            let tops: Vec<_> = slice.clips.iter().map(|clip| clip.top).collect();
            minimum_body_slice = breaks.cell_fresh_slice(row_index, &tops, body_capacity);
        }
        let header_overhead = if !is_first_fragment
            && row_index >= header_row_count
            && header_row_count > 0
            && header_rows_height + minimum_body_slice.max(0.0) <= column_capacity
        {
            header_rows_height
        } else {
            0.0
        };
        // A fragment resuming a row mid-content redraws that row's boundary
        // band at its cut edge and starts the content below it (Word-measured),
        // so the band comes out of the room before any line is fitted.
        let resume_band = if consumed > 0.0 {
            row_cut_band(measure, row_index)
        } else {
            0.0
        };
        let available_height = available_height - pending_spacing - header_overhead - resume_band;

        let start_row = row_index;
        let clip_top = consumed;
        let mut used = 0.0f64;
        let mut cur = row_index;
        // Px of `cur` already placed on a previous fragment. Only the first row of
        // this fragment can carry one (the rest start at 0); `cur == row_end` holds
        // at the top of every iteration.
        let first_row_offset = consumed;
        let mut row_end = row_index; // exclusive
        let mut clip_bottom: Option<f64> = None;
        let mut last_row_partial = false;
        let mut cell_clips = Vec::new();
        let mut next_cell_tops = None;

        while cur < rows.len() {
            // a fresh column repeats the header band above a first fragment's body
            let chain_room = if is_first_fragment && cur >= header_row_count {
                body_capacity
            } else {
                column_capacity - header_overhead
            };
            let keep_height = row_keep_height(
                keep_chains[cur],
                block,
                measure,
                &breaks,
                chain_room,
                if is_first_fragment {
                    header_row_count
                } else {
                    0
                },
                if is_first_fragment {
                    body_capacity
                } else {
                    chain_room
                },
                limited_by_float,
            );
            if (cur > start_row || consumed == 0.0)
                && keep_height > available_height - used
                && keep_height <= chain_room
            {
                if cur > start_row {
                    break;
                }
                if paginator.state(state_idx).pen_y != paginator.state(state_idx).content_top
                    && fit_moved_cursor(paginator, keep_height + header_overhead + pending_spacing)
                {
                    continue 'rows;
                }
            }
            let row_height = rows[cur].height;
            let start_off = if cur == start_row {
                first_row_offset
            } else {
                0.0
            };
            let active_tops = if cur == start_row {
                cell_tops.as_deref()
            } else {
                None
            };
            let remaining = active_tops.map_or(row_height - start_off, |tops| {
                breaks.cell_remaining(cur, tops)
            });

            // A row ending the fragment is closed by the band below it (the
            // next row's boundary band, or the table's bottom band).
            if used + remaining + trailing_band(measure, cur) <= available_height {
                // The rest of this row fits whole.
                used += remaining;
                if let Some(tops) = active_tops {
                    cell_clips.extend(breaks.cell_remainder(cur, tops).clips);
                }
                cur += 1;
                row_end = cur;
                continue;
            }

            // A legacy page float's continuation fragments move rows whole
            // across page boundaries (Word's "delayable floating table" rule,
            // Word-measured on floattable-avoid-last-manip-ofst: every later
            // page holds four whole rows): a row that does not fit the real
            // remaining space restarts on the next page rather than splitting
            // mid-content, unless it cannot fit a whole fresh fragment. The
            // first fragment splits mid-row at its deadline (the table-split
            // rows fixture with six paragraphs before the table breaks row 16
            // after its first line). The real remaining space matters, not the
            // band-limited one: the table's own wrap zone opens at or above the
            // pen and leaves a band width of room.
            if continuation == ContinuationRule::MoveRowsWhole
                && !is_first_fragment
                && remaining <= chain_room
            {
                let state = paginator.state(state_idx);
                let true_remaining = state.content_limit - state.pen_y - used;
                if remaining + trailing_band(measure, cur) > true_remaining {
                    if row_end > start_row {
                        break;
                    }
                    if state.pen_y != state.content_top {
                        paginator.advance_for_overflow();
                        continue 'rows;
                    }
                    // Oversized row at a fresh fragment top: fall through and
                    // let the mid-content split keep progress.
                }
            }

            // This row does not fully fit in the remaining space. Break it mid-content
            // at the deepest whole line that fits (Word's "allow row to break across
            // pages") — this keeps the row's other columns on the page where they
            // start and flows a tall vertically-merged cell across the boundary.
            // `w:cantSplit` rows (§17.4.6) only break when oversized. Exact-height
            // rows need no branch here: their break geometry holds only the full-height
            // boundary, so `snap_row_break` already returns 0 for a partial fit.
            // A row splitting mid-content breaks at the same limit a whole row
            // fills to: a page-anchored float's widened deadline (Word-measured,
            // the compat-14 continuation fixture's one row runs to top + body),
            // otherwise the text limit.
            let text_remaining = paginator.state(state_idx).content_limit
                - paginator.state(state_idx).pen_y
                - pending_spacing
                - header_overhead;
            // A mid-row cut redraws the row's boundary band below its last
            // visible line, so the lines must fit above that band.
            let budget = available_height.min(text_remaining - resume_band)
                - used
                - row_cut_band(measure, cur);
            let cant_split = block
                .rows
                .get(cur)
                .and_then(|r| r.cant_split)
                .unwrap_or(false);
            let row_capacity = if is_first_fragment && cur >= header_row_count {
                body_capacity
            } else {
                column_capacity - header_overhead
            };
            let unavoidable_cant_split =
                cant_split && remaining > column_capacity - header_overhead;
            let row_breaks = if cant_split {
                breaks.lines()
            } else {
                &breaks.kept
            };
            let placeable = if cant_split && !unavoidable_cant_split {
                0.0
            } else {
                snap_row_break(row_breaks, cur, start_off, budget)
            };
            let header_above_unavoidable_split = start_row == 0
                && clip_top == 0.0
                && cur == header_row_count
                && !cant_split
                && breaks.kept_oversized(cur, 0.0, row_capacity);
            let cell_slice = if active_tops.is_none() && limited_by_float {
                None
            } else {
                breaks.cell_slice(
                    cur,
                    start_off,
                    active_tops,
                    budget,
                    placeable,
                    false,
                    row_capacity,
                )
            };
            if cell_slice.as_ref().is_some_and(|slice| slice.gap_only)
                && (used > 0.0
                    || paginator.state(state_idx).pen_y != paginator.state(state_idx).content_top)
            {
                if row_end > start_row {
                    break;
                }
                paginator.advance_for_overflow();
                continue 'rows;
            }
            // A row moves whole rather than leave a cell with content no line on
            // the page, where a whole column lets every cell start.
            let cell_slice = match cell_slice {
                Some(slice)
                    if slice.starved
                        && breaks
                            .every_cell_start(cur, start_off, row_capacity)
                            .is_some() =>
                {
                    if placeable > 0.0 || (row_end > start_row && !header_above_unavoidable_split) {
                        None
                    } else if row_end == start_row
                        && paginator.state(state_idx).pen_y
                            != paginator.state(state_idx).content_top
                    {
                        paginator.advance_for_overflow();
                        continue 'rows;
                    } else {
                        Some(slice)
                    }
                }
                slice => slice,
            };
            if let Some(slice) = cell_slice {
                used += slice.height;
                row_end = cur + 1;
                if !slice.complete {
                    clip_bottom = Some(start_off + slice.height);
                    last_row_partial = true;
                    next_cell_tops = Some(slice.clips.iter().map(|clip| clip.bottom).collect());
                }
                cell_clips.extend(slice.clips);
                if slice.complete {
                    cur += 1;
                    continue;
                }
            } else if let Some(tops) = active_tops {
                if row_end > start_row {
                    break;
                }
                let minimum = breaks.cell_fresh_slice(cur, tops, row_capacity);
                if minimum <= row_capacity && minimum > budget {
                    if !paginator.has_float_bands() {
                        paginator.advance_for_overflow();
                        continue 'rows;
                    }
                    if fit_moved_cursor(paginator, minimum + header_overhead + pending_spacing) {
                        continue 'rows;
                    }
                }
                let slice = breaks
                    .cell_slice(
                        cur,
                        start_off,
                        active_tops,
                        budget,
                        placeable,
                        true,
                        row_capacity,
                    )
                    .unwrap_or_else(|| breaks.cell_remainder(cur, tops));
                used += slice.height;
                row_end = cur + 1;
                if !slice.complete {
                    clip_bottom = Some(start_off + slice.height);
                    last_row_partial = true;
                    next_cell_tops = Some(slice.clips.iter().map(|clip| clip.bottom).collect());
                }
                cell_clips.extend(slice.clips);
            } else if placeable > 0.0 {
                // Break this row mid-content at a whole-line boundary.
                used += placeable;
                row_end = cur + 1;
                clip_bottom = Some(start_off + placeable);
                last_row_partial = true;
            } else if row_end > start_row && !header_above_unavoidable_split {
                // Nothing of this row fits, but earlier rows did — end before it,
                // unless they are the header band above an unavoidable split.
            } else if !cant_split
                && !block.rows.get(cur).is_some_and(|row| row.is_exact_height())
                && snap_row_break(&breaks.kept, cur, start_off, row_capacity) > 0.0
            {
                // The paragraph rules allow a break that a whole column holds:
                // start the fragment below a float band, else in the next column.
                let slice = minimum_row_slice(block, measure, &breaks.kept, cur, start_off);
                if !paginator.has_float_bands() {
                    paginator.advance_for_overflow();
                    continue 'rows;
                }
                if fit_moved_cursor(paginator, slice + header_overhead + pending_spacing) {
                    continue 'rows;
                }
                // `ensure_fits` grants the slice at the cursor where rounding
                // left it out of the budget: place it here.
                used += slice.min(remaining);
                row_end = cur + 1;
                if slice < remaining {
                    clip_bottom = Some(start_off + slice);
                    last_row_partial = true;
                }
            } else {
                // Paragraph rules that leave no break in a column yield to whole lines.
                // If no line fits, overflow instead of looping.
                let line_break = if cant_split && !unavoidable_cant_split {
                    0.0
                } else {
                    snap_row_break(breaks.lines(), cur, start_off, budget)
                };
                if line_break > 0.0 {
                    used += line_break;
                    row_end = cur + 1;
                    clip_bottom = Some(start_off + line_break);
                    last_row_partial = true;
                } else if row_end == start_row {
                    used += remaining;
                    row_end = cur + 1;
                }
            }
            break;
        }

        // `used` is the visible content window; repeated headers stack on top of it.
        // The last fragment's bottom reaches the outer edge of the table's
        // bottom border band (contiguous row model), and a fragment resuming a
        // row mid-content redraws that row's boundary band at its cut edge, so
        // its height (and content) starts below the band.
        let is_last_fragment = row_end == rows.len() && !last_row_partial;
        let cut_band = if clip_bottom.is_some() {
            row_cut_band(measure, row_end - 1)
        } else if !is_last_fragment && row_end > start_row {
            // A fragment ending on a row boundary draws that boundary's band.
            trailing_band(measure, row_end - 1)
        } else {
            0.0
        };
        let fragment_height = header_overhead
            + resume_band
            + used
            + cut_band
            + if is_last_fragment {
                measure.bottom_band.unwrap_or(0.0)
            } else {
                0.0
            };

        let mut desired_x = paginator.get_column_x(paginator.state(state_idx).column_index);
        if block.justification.as_deref() == Some("center") {
            desired_x += (paginator.column_width() - measure.total_width) / 2.0;
        } else if block.justification.as_deref() == Some("right") {
            // Word-measured (diag-legacy-shift in-right): a mode <= 14
            // right-aligned table sits T right of its mode 15 position.
            desired_x += paginator.column_width() - measure.total_width
                + table_compat_x_adjust(
                    block.justification.as_deref(),
                    block.compatibility_mode,
                    block.cell_margin_left,
                );
        } else {
            let indent = block
                .indent
                .filter(|value| value.is_finite())
                .unwrap_or(0.0);
            let shift = table_compat_x_adjust(
                block.justification.as_deref(),
                block.compatibility_mode,
                block.cell_margin_left,
            );
            desired_x += indent + shift;
        }

        if let Some(x) = floating_x {
            desired_x = x;
        }

        let fragment = Fragment::Table(TableFragment {
            block_id: block.id.clone(),
            x: desired_x,
            y: 0.0, // set by add_fragment
            width: measure.total_width,
            height: fragment_height,
            row_start: start_row,
            row_end,
            pm_start: block.pm_start,
            pm_end: block.pm_end,
            is_floating: floating_x.map(|_| true),
            carried_from_prev: Some(!is_first_fragment),
            carried_to_next: Some(!is_last_fragment),
            header_row_count: (header_overhead > 0.0).then_some(header_row_count as f64),
            clip_top: if clip_top > 0.0 { Some(clip_top) } else { None },
            clip_bottom,
            cell_clips: (!cell_clips.is_empty()).then_some(cell_clips),
        });

        paginator.add_fragment(fragment, fragment_height, 0.0, 0.0);

        let landed_idx = paginator.get_current();
        let page_index = paginator.state(landed_idx).page_index;
        if let Some(Fragment::Table(placed)) = paginator.pages[page_index].fragments.last_mut() {
            placed.x = desired_x;
        }

        // Advance the cursor. A partial last row resumes at its break point
        // (`clip_bottom`); otherwise we move past the rows just placed.
        if last_row_partial {
            row_index = row_end - 1;
            consumed = clip_bottom.unwrap_or(0.0);
            cell_tops = next_cell_tops;
        } else {
            row_index = row_end;
            consumed = 0.0;
            cell_tops = None;
        }

        // If content remains, advance to the next column/page so the next
        // iteration sees fresh space (the current page is exhausted).
        if row_index < rows.len() {
            if cell_tops.is_some() {
                paginator.advance_for_overflow();
                continue;
            }
            let next_breaks = if block
                .rows
                .get(row_index)
                .is_some_and(|row| row.cant_split.unwrap_or(false))
            {
                breaks.lines()
            } else {
                &breaks.kept
            };
            let next_slice = minimum_row_slice(block, measure, next_breaks, row_index, consumed);
            let next_needed = if row_index >= header_row_count
                && header_row_count > 0
                && header_rows_height + next_slice <= column_capacity
            {
                header_rows_height
            } else {
                0.0
            } + next_slice;
            paginator.ensure_fits(next_needed);
        }
    }

    for (idx, limit) in extended_limits {
        paginator.set_content_limit(idx, limit);
    }
    Ok(())
}

/// Resolves a floating table's leading-edge x on the page `state_idx` (which
/// must be the paginator's current state): the anchor's horizontal band, an
/// explicit `w:tblpX`, an alignment spec — `inside`/`outside` flip with page
/// parity — or the table justification.
///
/// Then Word's legacy compatibility rule (docs/floating-tables.md §6.1,
/// Word-measured in diag-legacy-shift): in compatibilityMode <= 14 a
/// left-aligned floating table shifts left by T = the resolved left cell
/// margin, unconditionally, and its `w:tblInd` counts toward the position
/// (mode 15 ignores it); a right-aligned one (including `outside` landing on
/// the right side) shifts right by T; a centered one stays. The wrap zone
/// shifts with the table via the same [`table_compat_x_adjust`] in
/// `measure_blocks::table_floating_zone`.
fn floating_table_x(
    block: &TableBlock,
    total_width: f64,
    paginator: &Paginator,
    state_idx: usize,
) -> f64 {
    let floating = block
        .floating
        .as_ref()
        .expect("floating table hook requires tblpPr");
    let state = paginator.state(state_idx);
    let page = &paginator.pages[state.page_index];
    let column_x = paginator.get_column_x(state.column_index);
    let column_width = paginator.column_width();
    let horizontal = floating.horz_anchor.as_deref().unwrap_or("margin");
    let h_start = match horizontal {
        "page" => 0.0,
        "text" => column_x,
        _ => paginator.active_margins().left,
    };
    let h_end = match horizontal {
        "page" => page.size.w,
        "text" => column_x + column_width,
        _ => page.size.w - paginator.active_margins().right,
    };
    let inside_is_start = page.number % 2 == 1;

    let mut x = h_start;
    if let Some(offset) = floating.tblp_x.filter(|value| value.is_finite()) {
        x = h_start + offset;
    } else if let Some(spec) = floating.tblp_x_spec.as_deref() {
        let start_aligned = spec == "left"
            || (spec == "inside" && inside_is_start)
            || (spec == "outside" && !inside_is_start);
        let end_aligned = spec == "right"
            || (spec == "inside" && !inside_is_start)
            || (spec == "outside" && inside_is_start);
        if spec == "center" {
            x = h_start + (h_end - h_start - total_width) / 2.0;
        } else if end_aligned {
            x = h_end - total_width;
        } else if start_aligned {
            x = h_start;
        }
    } else if block.justification.as_deref() == Some("center") {
        x = h_start + (h_end - h_start - total_width) / 2.0;
    } else if block.justification.as_deref() == Some("right") {
        x = h_end - total_width;
    }
    // Which edge the table hangs from decides the sign of the legacy shift:
    // the spec wins, then the justification (Word-measured: a float with both
    // `tblpX` and `jc=right` still shifts right).
    let alignment = match floating.tblp_x_spec.as_deref() {
        Some("center") => Some("center"),
        Some("right") => Some("right"),
        Some("left") => Some("left"),
        Some("inside") => Some(if inside_is_start { "left" } else { "right" }),
        Some("outside") => Some(if inside_is_start { "right" } else { "left" }),
        _ => match block.justification.as_deref() {
            Some("center") => Some("center"),
            Some("right") => Some("right"),
            _ => None,
        },
    };
    if block.layout_era() == WordLayoutEra::Legacy {
        if alignment.is_none_or(|side| side == "left") {
            // A mode <= 14 float's `w:tblInd` counts toward the leading edge.
            x += block
                .indent
                .filter(|value| value.is_finite())
                .unwrap_or(0.0);
        }
        x += table_compat_x_adjust(alignment, block.compatibility_mode, block.cell_margin_left);
    }
    x
}

/// Places a `w:tblpPr` table as an overlay that normally leaves the pen alone.
///
/// A table taller than one column cannot float and falls back to
/// [`layout_table`]. Otherwise `w:horzAnchor` / `w:vertAnchor` pick the
/// page, margin or text band, an explicit `w:tblpX` / `w:tblpY` offsets from
/// that band's start, and an alignment spec resolves against it — `inside` and
/// `outside` flip with page parity. Only when the wrap gutters on both sides of
/// the table fall below the minimum wrap segment (24px) does the pen advance
/// past the table plus its `w:bottomFromText` distance, since no line could
/// wrap beside it. In a single column, text-anchored tables that cross the
/// bottom boundary use row fragmentation and retain their X position; text
/// after a side-wrapped one resumes beside its last fragment.
/// Page-relative full-width tables advance when inline collisions cannot reflow.
pub fn layout_floating_table(
    block: &TableBlock,
    measure: &TableExtent,
    paginator: &mut Paginator,
    content_width: f64,
) -> Result<(), LayoutError> {
    if block.rows.is_empty() || measure.rows.is_empty() {
        return Err(unsupported("floating table without measurable rows"));
    }
    let initial_state = paginator.get_current();
    let column_capacity =
        paginator.state(initial_state).content_limit - paginator.state(initial_state).content_top;
    let floating = block
        .floating
        .as_ref()
        .expect("floating table hook requires tblpPr");
    let mirrored_x = matches!(floating.tblp_x_spec.as_deref(), Some("inside" | "outside"));
    let single_column_at_entry = (content_width - paginator.column_width()).abs() < f64::EPSILON;
    let tblp_y_spec = float_plan::VertSpec::parse(floating.tblp_y_spec.as_deref());
    // B0 (`float_plan::plan` repeats it): decided before positioning, which
    // may advance the page. In a single column the fallback keeps the
    // float's resolved x (compatibility adjustment included); in multiple
    // columns a float anchors to its column, so the inline path's column
    // position stands. An explicit `w:tblpY` or a position-fixing
    // `w:tblpYSpec` (§6.1 ycenter/ybottom) lets the table fragment in place,
    // mirrored alignment included.
    if float_plan::falls_back_inline(
        floating.tblp_y.is_some_and(|y| y.is_finite() && y >= 0.0)
            || tblp_y_spec.is_some_and(float_plan::VertSpec::fixes_position),
        measure.total_height,
        column_capacity,
    ) {
        let floating_x = single_column_at_entry
            .then(|| floating_table_x(block, measure.total_width, paginator, initial_state));
        return layout_table_with_position(block, measure, paginator, floating_x);
    }
    let has_explicit_y = floating.tblp_y.is_some()
        || floating
            .tblp_y_spec
            .as_deref()
            .is_some_and(|spec| spec != "inline");
    if !has_explicit_y {
        paginator.ensure_fits(measure.total_height);
    }

    let state_idx = paginator.get_current();
    let state = paginator.state(state_idx);
    let page = &paginator.pages[state.page_index];
    let margins = page.body_anchor_margins.as_ref().unwrap_or(&page.margins);
    let vertical = floating.vert_anchor.as_deref().unwrap_or("text");
    let v_start = match vertical {
        "page" => 0.0,
        "text" => state.pen_y,
        _ => margins.top,
    };
    let v_end = if vertical == "page" {
        page.size.h
    } else {
        page.size.h - margins.bottom
    };
    let column_x = paginator.get_column_x(state.column_index);
    let column_width = paginator.column_width();
    let x = floating_table_x(block, measure.total_width, paginator, state_idx);

    let mut y = state.pen_y;
    if let Some(offset) = floating.tblp_y.filter(|value| value.is_finite()) {
        y = v_start + offset;
    } else if let Some(spec) = tblp_y_spec {
        // §6.1 ycenter/ybottom (Word-measured on the matrix fixtures and the
        // plan-fixes probes): a spec float taller than the body room below
        // the pen aligns by its whole height, then the top backs off by the
        // overflow, so the first fragment still starts inside the page.
        y = float_plan::spec_anchor_y(
            spec,
            v_start,
            v_end,
            measure.total_height,
            state.content_limit - state.pen_y,
        );
    }
    // Diagnosis C (§6.1, Word-measured in diag-wrap-last-fragment): a legacy
    // margin-anchored float positioned above the pen starts at the pen and
    // still splits and wraps as a float. `plan`'s above-pen exception covers
    // page anchors only, so without the clamp the table falls into B4 and
    // lays out inline, leaving no wrap zone beside its last fragment.
    let y =
        if block.layout_era() == WordLayoutEra::Legacy && vertical == "margin" && y < state.pen_y {
            state.pen_y
        } else {
            y
        };
    let single_column = (content_width - column_width).abs() < f64::EPSILON;
    // §6.1 cols2 (Word-measured on the plan-fixes D probes): a float in a
    // multi-column section never splits — it keeps its position, clamped so
    // it ends at the anchor band's bottom when it can and never starts above
    // the band's top; the part past the page bottom is clipped away.
    let y = if single_column {
        y
    } else {
        float_plan::whole_page_y(y, v_start, v_end, measure.total_height)
    };

    let finite = |value: Option<f64>| value.filter(|v| v.is_finite()).unwrap_or(0.0);
    let inline_tables = page
        .fragments
        .iter()
        .filter_map(|fragment| match fragment {
            Fragment::Table(previous)
                if previous.is_floating != Some(true)
                    && previous.carried_from_prev == Some(false)
                    && previous.carried_to_next == Some(false)
                    && previous.row_start == 0
                    && previous.clip_top.is_none()
                    && previous.clip_bottom.is_none() =>
            {
                Some(InlineTableBox {
                    x: previous.x,
                    y: previous.y,
                    width: previous.width,
                    height: previous.height,
                })
            }
            _ => None,
        })
        .collect();
    let inputs = FloatInputs {
        era: block.layout_era(),
        vertical: VertAnchor::parse(floating.vert_anchor.as_deref()),
        tblp_y: floating.tblp_y,
        tblp_y_spec,
        mirrored_x,
        single_column_at_entry,
        entry_column_capacity: column_capacity,
        single_column,
        table_width: measure.total_width,
        table_height: measure.total_height,
        x,
        y,
        column_x,
        column_width,
        left_from_text: finite(floating.left_from_text),
        right_from_text: finite(floating.right_from_text),
        bottom_from_text: finite(floating.bottom_from_text),
        pen_y: state.pen_y,
        content_top: state.content_top,
        content_limit: state.content_limit,
        page_height: page.size.h,
        footer_distance: margins.footer,
        inline_tables,
    };

    match float_plan::plan(&inputs) {
        // B0 (already taken above), B4: the float falls back to inline
        // layout but keeps its resolved floating x.
        FloatPlan::Inline => layout_table_with_position(block, measure, paginator, Some(x)),
        // B1, B2
        FloatPlan::MoveToNextPage => {
            paginator.force_page_break();
            let next_content_width = paginator.get_content_width();
            layout_floating_table(block, measure, paginator, next_content_width)
        }
        // B3
        FloatPlan::Split {
            first_limit,
            continuation,
            zero_space_before,
            full_width,
        } => {
            let pen_y = inputs.pen_y;
            let spacing = paginator.spacing(state_idx);
            if zero_space_before {
                paginator.set_spacing(state_idx, (0.0, spacing.1));
            }
            paginator.set_pen_y(state_idx, y);
            layout_table_with_position_and_limit(
                block,
                measure,
                paginator,
                Some(x),
                first_limit,
                continuation,
            )?;
            let last_state = paginator.get_current();
            let resume = if full_width {
                paginator.state(last_state).pen_y + inputs.bottom_from_text.max(0.0)
            } else if last_state == state_idx {
                paginator.set_spacing(state_idx, spacing);
                pen_y
            } else {
                paginator.set_spacing(last_state, (0.0, f64::INFINITY));
                let last = paginator.state(last_state);
                paginator.pages[last.page_index]
                    .fragments
                    .iter()
                    .rev()
                    .find_map(|fragment| match fragment {
                        Fragment::Table(table) if table.block_id == block.id => Some(table.y),
                        _ => None,
                    })
                    .unwrap_or(last.content_top)
            };
            paginator.set_pen_y(last_state, resume);
            Ok(())
        }
        // B5, B5a
        FloatPlan::Whole { full_width } => {
            let fragment = Fragment::Table(TableFragment {
                block_id: block.id.clone(),
                x,
                y,
                width: measure.total_width,
                height: measure.total_height,
                row_start: 0,
                row_end: block.rows.len(),
                pm_start: block.pm_start,
                pm_end: block.pm_end,
                is_floating: Some(true),
                carried_from_prev: None,
                carried_to_next: None,
                header_row_count: None,
                clip_top: None,
                clip_bottom: None,
                cell_clips: None,
            });
            paginator.push_fragment_direct(fragment);

            if let Some(band) = full_width {
                let band_bottom = y + measure.total_height + inputs.bottom_from_text;
                if band == FullWidthBand::ClearOrPush
                    && paginator
                        .clear_float_band(state_idx, y, band_bottom)
                        .is_some()
                {
                    return Ok(());
                }
                // Charges the band to the page even when it opens above the pen; flow
                // already emitted into it keeps its place.
                let pen_y = paginator.state(state_idx).pen_y;
                let advance_to = pen_y.max(y) + measure.total_height + inputs.bottom_from_text;
                if advance_to > pen_y {
                    paginator.set_pen_y(state_idx, advance_to);
                }
            }
            Ok(())
        }
    }
}
