export function wasmAssetUrl(png = true): URL {
  return new URL(png ? 'nimbleoffice-native:xlsx' : 'nimbleoffice-native:xlsx-lean');
}

export function rasterWasmAssetUrl(): URL {
  throw new Error('@nimbleoffice/xlsx-native renders PNG in the native engine and ships no raster wasm');
}
