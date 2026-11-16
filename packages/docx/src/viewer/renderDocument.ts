import { buildResidentRegionLayoutRequest } from '../editor/computeLayout';
import { renderEnvironment } from '../docx/structuredExport';
import { drawDisplayPage, sizeCanvasForPage } from '../layout/render/canvasBackend';
import { createCanvasImageResolver } from '../layout/render/canvasImageResolver';
import type { DisplayList, DisplayPage } from '../layout/render/displayList';
import { GlyphCache } from '../layout/render/glyphCache';
import {
  createRustMeasureSource,
  type ResidentFontRequirement,
} from '../layout/measure/rustMeasureSource';
import { extractEmbeddedFontFaces } from '../utils/embeddedFonts';
import { createYrsSession, type YrsSession } from '../yrs';

/** Options for {@link renderDocument}. */
export interface RenderDocumentOptions {
  /** Backing-store scale of each page canvas; defaults to `window.devicePixelRatio`. */
  devicePixelRatio?: number;
  /** Vertical gap between pages in CSS px; default 16. */
  pageGap?: number;
  /** Distance in CSS px beyond the scroll viewport within which pages stay painted; default 1000. */
  overscan?: number;
  /** The scrolling ancestor pages are tracked against; default the browser viewport. */
  scrollRoot?: Element | null;
  /** Called once, after the first page is painted. */
  onFirstPage?: () => void;
  /** Aborts loading; after it settles the preview is destroyed. */
  signal?: AbortSignal;
}

/** A read-only, paginated DOCX preview mounted in a container. */
export interface DocxPreview {
  /** Page sizes in CSS px, in document order. */
  readonly pages: readonly { width: number; height: number }[];
  /** Removes the pages and releases the engine. */
  destroy(): void;
}

/**
 * Lays out `bytes` in full and mounts one element per page in `container`, painting only pages
 * near the scroll viewport and releasing the canvases of pages that leave it. No caret,
 * selection or editing UI; the host owns scrolling and zoom. Never fetches remote resources.
 */
export async function renderDocument(
  bytes: Uint8Array,
  container: HTMLElement,
  options: RenderDocumentOptions = {}
): Promise<DocxPreview> {
  const { signal } = options;
  signal?.throwIfAborted();
  const session = await createYrsSession({ clientId: 1 });
  let mounted: DocxPreview | undefined;
  try {
    const display = await layoutDisplayList(session, bytes, signal);
    signal?.throwIfAborted();
    mounted = mountPages(session, display, container, options);
    return mounted;
  } finally {
    if (!mounted) session.destroy();
  }
}

async function layoutDisplayList(
  session: YrsSession,
  bytes: Uint8Array,
  signal: AbortSignal | undefined
): Promise<DisplayList> {
  const { document } = session.openDocx(bytes, true, { mediaTokens: true });
  const request = buildResidentRegionLayoutRequest(document, 0, renderEnvironment(document, {}));
  const measurement = createRustMeasureSource({
    engine: {
      registerFont: (data) => session.registerFont(data),
      registerSubstituteFont: (base, family) => session.registerSubstituteFont(base, family),
      clearFonts: () => session.clearFonts(),
    },
  });
  measurement.setCompat(document.package.settings?.compatibilityFlags);
  measurement.setEmbeddedFaces(await extractEmbeddedFontFaces(document));
  signal?.throwIfAborted();
  const requirements = JSON.parse(
    session.layoutFontRequirementsJson(JSON.stringify(request))
  ) as ResidentFontRequirement[];
  await measurement.prepareFontRequirements(requirements);
  signal?.throwIfAborted();
  const config = measurement.measurementConfigForRequirements(requirements);
  if (!config) throw new Error('DOCX preview fonts are not ready');
  request.measurement = config;
  const layout = JSON.parse(session.layoutDocumentWithRegionsJson(JSON.stringify(request)));
  return JSON.parse(
    session.buildDisplayListJson(JSON.stringify({ ...layout, fontChains: config.fontChains }))
  ) as DisplayList;
}

function mountPages(
  session: YrsSession,
  display: DisplayList,
  container: HTMLElement,
  options: RenderDocumentOptions
): DocxPreview {
  const scale = options.devicePixelRatio ?? globalThis.devicePixelRatio ?? 1;
  const drawOptions = {
    glyphCache: new GlyphCache({ provider: (font, glyph) => session.outlineGlyphJson(font, glyph) }),
    resolveImage: createCanvasImageResolver({
      media: (token) => session.mediaSource(token),
      mediaScope: () => session.mediaScope(),
    }),
  };
  const root = container.ownerDocument.createElement('div');
  root.style.cssText = `display:flex;flex-direction:column;align-items:center;gap:${options.pageGap ?? 16}px`;
  const slots = display.pages.map((page) => {
    const slot = container.ownerDocument.createElement('div');
    slot.style.cssText = `position:relative;flex:none;width:${page.width}px;height:${page.height}px;background:#fff`;
    root.appendChild(slot);
    return slot;
  });
  container.appendChild(root);

  let destroyed = false;
  let firstPainted = false;
  const painting = new Map<number, symbol>();
  const paint = async (index: number) => {
    const slot = slots[index]!;
    if (slot.firstChild || painting.has(index)) return;
    const token = Symbol();
    painting.set(index, token);
    const canvas = container.ownerDocument.createElement('canvas');
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const page = display.pages[index] as DisplayPage;
    sizeCanvasForPage(canvas, ctx, page, scale);
    await drawDisplayPage(ctx, page, drawOptions);
    if (destroyed || painting.get(index) !== token) return release(canvas);
    painting.delete(index);
    slot.appendChild(canvas);
    if (index === 0 && !firstPainted) {
      firstPainted = true;
      options.onFirstPage?.();
    }
  };
  const evict = (index: number) => {
    painting.delete(index);
    const canvas = slots[index]!.firstChild as HTMLCanvasElement | null;
    if (!canvas) return;
    canvas.remove();
    release(canvas);
  };
  const overscan = options.overscan ?? 1000;
  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        const index = slots.indexOf(entry.target as HTMLDivElement);
        if (entry.isIntersecting) void paint(index);
        else evict(index);
      }
    },
    { root: options.scrollRoot ?? null, rootMargin: `${overscan}px 0px` }
  );
  for (const slot of slots) observer.observe(slot);

  const destroy = () => {
    if (destroyed) return;
    destroyed = true;
    observer.disconnect();
    slots.forEach((_, index) => evict(index));
    root.remove();
    session.destroy();
  };
  options.signal?.addEventListener('abort', destroy, { once: true });
  return {
    pages: display.pages.map((page) => ({ width: page.width, height: page.height })),
    destroy,
  };
}

function release(canvas: HTMLCanvasElement): void {
  canvas.width = 0;
  canvas.height = 0;
}
