import type { Document, SectionProperties } from '@nimbleoffice/docx/types/document';

export function getInitialSectionProperties(
  document: Document | null | undefined
): SectionProperties | undefined {
  const body = document?.package?.document;
  return body?.sections?.[0]?.properties ?? body?.finalSectionProperties;
}
