export { renderDocument, type DocxPreview, type RenderDocumentOptions } from './renderDocument';
export { preloadOpcWasm } from '../wasm/opc';
export { preloadEditWasm } from '../wasm/edit';
export { setGoogleFontsEnabled } from '../utils/fontLoader';
export { configureDefaultFonts, type DefaultFontOptions } from '../layout/measure/defaultFontProvider';
export type { BundledFontProvider, FontScript } from '../layout/measure/fontRegistry';
