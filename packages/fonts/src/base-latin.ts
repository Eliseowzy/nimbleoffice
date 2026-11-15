export const BASE_LATIN_FONT_ASSET_URLS: Record<string, () => URL> = {
  'NimbleOfficeBaseLatin-Regular.ttf.zst': () => new URL('../assets/NimbleOfficeBaseLatin-Regular.ttf.zst', import.meta.url),
  'NimbleOfficeBaseLatin-Bold.ttf.zst': () => new URL('../assets/NimbleOfficeBaseLatin-Bold.ttf.zst', import.meta.url),
  'NimbleOfficeBaseLatin-Italic.ttf.zst': () => new URL('../assets/NimbleOfficeBaseLatin-Italic.ttf.zst', import.meta.url),
  'NimbleOfficeBaseLatin-BoldItalic.ttf.zst': () => new URL('../assets/NimbleOfficeBaseLatin-BoldItalic.ttf.zst', import.meta.url),
};
