import type { EndnoteProperties, FootnoteProperties } from '../types/document';

export interface CompatibilityFlags {
  compatibilityMode: number;
  noLeading: boolean;
  doNotExpandShiftReturn: boolean;
  useWord97LineBreakRules: boolean;
  balanceSingleByteDoubleByteWidth: boolean;
  doNotUseHTMLParagraphAutoSpacing?: boolean;
  suppressSpBfAfterPgBrk?: boolean;
  allowSpaceOfSameStyleInTable?: boolean;
  adjustLineHeightInTable?: boolean;
}

export const DEFAULT_COMPATIBILITY_FLAGS: CompatibilityFlags = {
  compatibilityMode: 12,
  noLeading: false,
  doNotExpandShiftReturn: false,
  useWord97LineBreakRules: false,
  balanceSingleByteDoubleByteWidth: false,
  doNotUseHTMLParagraphAutoSpacing: false,
  suppressSpBfAfterPgBrk: false,
  allowSpaceOfSameStyleInTable: false,
  adjustLineHeightInTable: false,
};

/** Public model contract rehydrated by the Rust parser. */
export interface DocumentSettings {
  defaultTabStop: number;
  defaultTableStyle?: string;
  themeFontLang?: { eastAsia?: string; bidi?: string };
  compatibilityFlags: CompatibilityFlags;
  updateFields?: boolean;
  evenAndOddHeaders?: boolean;
  trackRevisions?: boolean;
  doNotTrackMoves?: boolean;
  doNotTrackFormatting?: boolean;
  revisionView?: {
    markup?: boolean;
    comments?: boolean;
    insertionsDeletions?: boolean;
    formatting?: boolean;
  };
  footnotePr?: FootnoteProperties;
  endnotePr?: EndnoteProperties;
  /** `w:characterSpacingControl`: `doNotCompress`, `compressPunctuation` or `compressPunctuationAndJapaneseKana`. */
  characterSpacingControl?: string;
}

/** Whether the settings compress fullwidth punctuation to half an em. */
export function compressesPunctuation(settings: DocumentSettings | undefined | null): boolean {
  const control = settings?.characterSpacingControl;
  return control === 'compressPunctuation' || control === 'compressPunctuationAndJapaneseKana';
}

export const DEFAULT_TAB_STOP_TWIPS = 720;
