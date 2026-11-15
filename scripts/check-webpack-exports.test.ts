import { expect, test } from 'bun:test';
import { exportSpecifiers, importTarget, packageManifests } from './check-webpack-exports.mjs';

test('every export subpath becomes a specifier, wildcards left out', () => {
  expect(
    exportSpecifiers({
      name: '@nimbleoffice/fixture',
      exports: {
        '.': { types: './dist/index.d.ts', import: './dist/index.js', default: './dist/index.js' },
        './styles.css': './dist/styles.css',
        './*': './dist/*.js',
        './package.json': './package.json',
      },
    })
  ).toEqual(['@nimbleoffice/fixture', '@nimbleoffice/fixture/styles.css', '@nimbleoffice/fixture/package.json']);
});

test('a root-only exports field is the package itself', () => {
  expect(exportSpecifiers({ name: 'a', exports: './index.js' })).toEqual(['a']);
  expect(exportSpecifiers({ name: 'b', exports: { import: './b.js', default: './b.js' } })).toEqual(['b']);
  expect(exportSpecifiers({ name: 'c' })).toEqual([]);
});

test('the import target of an export entry', () => {
  const manifest = {
    name: '@nimbleoffice/fixture-native',
    exports: {
      '.': { types: './dist/index.d.ts', browser: './dist/browser.js', import: './dist/index.js' },
      './worker': { types: './dist/worker.d.ts', browser: './dist/browser.js', import: './dist/worker.js' },
      './styles.css': './dist/styles.css',
    },
  };
  expect(importTarget(manifest, '@nimbleoffice/fixture-native')).toBe('./dist/index.js');
  expect(importTarget(manifest, '@nimbleoffice/fixture-native/worker')).toBe('./dist/worker.js');
  expect(importTarget(manifest, '@nimbleoffice/fixture-native/styles.css')).toBeUndefined();
  expect(importTarget({ name: 'b', exports: { import: './b.js', default: './b.js' } }, 'b')).toBe('./b.js');
});

test('the checked libraries are every package under packages/ with an exports map', () => {
  const names = packageManifests().map(({ manifest }) => manifest.name);
  for (const format of ['docx', 'xlsx', 'pptx', 'vsdx']) {
    for (const suffix of ['', '-react', '-i18n']) expect(names).toContain(`@nimbleoffice/${format}${suffix}`);
  }
  expect(names).toContain('@nimbleoffice/agents');
  expect(names).toContain('@nimbleoffice/fonts');
  expect(names.every((name) => name.startsWith('@nimbleoffice/'))).toBe(true);
});
