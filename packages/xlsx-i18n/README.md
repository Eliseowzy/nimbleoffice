# @nimbleoffice/xlsx-i18n

UI strings, locale types and translation helpers for the
[`@nimbleoffice/xlsx-react`](https://www.npmjs.com/package/@nimbleoffice/xlsx-react) editor. `en` defines
the strings; the other locales override them.

## Install

```bash
bun add @nimbleoffice/xlsx-i18n
```

## Usage

Pass a locale to the editor's `i18n` prop:

```tsx
import { de } from '@nimbleoffice/xlsx-i18n';
import { XlsxEditor } from '@nimbleoffice/xlsx-react';

export function Editor({ file }: { file: Uint8Array }) {
  return <XlsxEditor file={file} i18n={de} />;
}
```

Override individual strings by spreading a locale. Keys that are missing or `null` in an override fall
back to English.

```ts
const custom = { ...de, toolbar: { ...de.toolbar, save: 'Sichern' } };
```

## Locales

`en` (source), `ar`, `de`, `es`, `fr`, `he`, `hi`, `id`, `it`, `ja`, `ko`, `nl`, `pl`, `pt-BR`, `ru`,
`tr`, `uk`, `vi`, `zh-CN` and `zh-TW`. The exports for `pt-BR`, `zh-CN` and `zh-TW` are `ptBR`,
`zhCN` and `zhTW`. Every locale translates every string.

Each locale is also its own subpath, for example `@nimbleoffice/xlsx-i18n/pl`, so an app that picks
the locale at runtime can load only the one it needs. `locales` maps every tag to its strings and
bundles all of them.

## Helpers

The package exports the types `LocaleStrings`, `PartialLocaleStrings`, `Translations`,
`TranslationKey`, `LocaleCode` and `TFunction`, and the functions `createT` and `deepMerge`.
Outside React, build a typed `t()` directly:

```ts
import { createT, deepMerge, en, de, type LocaleStrings } from '@nimbleoffice/xlsx-i18n';

const t = createT(deepMerge(en, de) as LocaleStrings, 'de');
t('toolbar.save'); // 'Save'
```

## Documentation

- JavaScript guide
- [`@nimbleoffice/xlsx-react`](https://www.npmjs.com/package/@nimbleoffice/xlsx-react)
- [Changelog](https://github.com/Eliseowzy/nimbleoffice/blob/main/packages/xlsx-i18n/CHANGELOG.md)
