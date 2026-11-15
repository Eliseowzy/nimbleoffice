export function wasmAssetUrl(): URL {
  return new URL('nimbleoffice-native:pptx');
}

export function sessionWasmAssetUrl(): URL {
  return wasmAssetUrl();
}

export function metafileWasmAssetUrl(): URL {
  return wasmAssetUrl();
}

export const shipsWasmAsset = false;
