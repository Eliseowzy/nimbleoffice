/** Shared vertical geometry for canvas page hosts. */

import type { DisplayListQueries } from './displayListQueries';

/** vertical gap between canvas pages (flex `gap`) */
export const CANVAS_PAGE_GAP_PX = 16;
/** padding around the canvas pages column */
export const CANVAS_PAGES_PADDING_PX = 24;

/**
 * How page geometry maps to device pixels. `rasterScale` is the canvas
 * backing scale (`devicePixelRatio * zoom`); `unitScale` is the device pixels
 * per unit of the column the boxes are laid out in (`rasterScale` when a
 * transform applies the zoom, `devicePixelRatio` when the column is sized at
 * the zoom).
 */
export interface CanvasPageScale {
  rasterScale: number;
  unitScale: number;
}

/** A page box in column units. */
export interface CanvasPageBox {
  top: number;
  width: number;
  height: number;
}

/** How a page column is stacked. */
export interface CanvasPageStackOptions {
  /** Snaps every box to whole device pixels at this scale; omitted, boxes keep the exact page size. */
  scale?: CanvasPageScale;
  /** Column units per page unit when no `scale` is given (a column sized at the zoom). */
  zoom?: number;
  padding?: number;
}

function validScale(scale: CanvasPageScale | undefined): CanvasPageScale | undefined {
  return scale && Number.isFinite(scale.rasterScale) && scale.rasterScale > 0 &&
    Number.isFinite(scale.unitScale) && scale.unitScale > 0 ? scale : undefined;
}

/** Page length in column units; with a scale, it covers exactly its canvas backing pixels. */
export function canvasPageLength(length: number, scale?: CanvasPageScale, zoom = 1): number {
  const valid = validScale(scale);
  const page = Math.max(0, length);
  return valid ? Math.ceil(page * valid.rasterScale) / valid.unitScale : page * zoom;
}

/** A column length, rounded to whole device pixels when a scale is given. */
export function snapCanvasLength(length: number, scale?: CanvasPageScale): number {
  const valid = validScale(scale);
  return valid ? Math.round(length * valid.unitScale) / valid.unitScale : length;
}

/** A page as the column stacks it. */
export interface CanvasStackPage {
  width: number;
  height: number;
  /** A parity filler takes no room in the column: Word's Print Layout hides it. */
  parityFiller?: boolean;
}

/**
 * Page boxes stacked down a column. With a scale, each box spans its canvas
 * backing pixels exactly and every top falls on a whole device pixel, so page
 * bitmaps are shown 1:1 instead of resampled. Every surface that converts
 * between page and column coordinates reads its tops from here. A parity
 * filler keeps its index but gets a zero-height box and no gap, matching the
 * `display: none` page the canvas column renders for it.
 */
export function canvasPageStack(
  pages: readonly CanvasStackPage[],
  { scale, zoom = 1, padding = 0 }: CanvasPageStackOptions = {}
): CanvasPageBox[] {
  const gap = snapCanvasLength(CANVAS_PAGE_GAP_PX, scale);
  let top = snapCanvasLength(padding, scale);
  return pages.map((page) => {
    if (page.parityFiller) return { top, width: canvasPageLength(page.width, scale, zoom), height: 0 };
    const box = { top, width: canvasPageLength(page.width, scale, zoom), height: canvasPageLength(page.height, scale, zoom) };
    top += box.height + gap;
    return box;
  });
}

/**
 * Top Y of each canvas page inside the `.canvas-pages` host, derived from the
 * display list's own page heights. `tops[i] + pageLocalY` converts a
 * display-list rect into host coordinates.
 */
export function canvasPageTops(queries: DisplayListQueries): number[] {
  const pages = queries.displayList?.pages ?? [];
  const sizes = Array.from({ length: queries.pageCount() }, (_, index) => ({
    ...(queries.pageSize(index) ?? { width: 0, height: 0 }),
    parityFiller: pages[index]?.parityFiller === true,
  }));
  return canvasPageStack(sizes, { padding: CANVAS_PAGES_PADDING_PX }).map(({ top }) => top);
}
