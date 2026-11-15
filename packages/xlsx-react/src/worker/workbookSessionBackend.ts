import { openWorkbookSession } from '@nimbleoffice/xlsx';
import { createWorkbookEditPeer, hydratePeer } from '@nimbleoffice/xlsx/worker';

export const editableWorkbookSessionBackend = {
  open: openWorkbookSession,
  hydrate: hydratePeer,
  attach: createWorkbookEditPeer,
};
