import type { DocumentSettings } from '@nimbleoffice/docx/types/document';

/** Whether `w:characterSpacingControl` sets fullwidth punctuation in half an em. */
export function compressesPunctuation(settings: DocumentSettings | undefined | null): boolean {
  const control = settings?.characterSpacingControl;
  return control === 'compressPunctuation' || control === 'compressPunctuationAndJapaneseKana';
}
