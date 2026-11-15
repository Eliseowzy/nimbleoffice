import { expect, test } from 'bun:test';
import { importPlan } from './check-node-import.mjs';

test('every export is imported or refused, never skipped', () => {
  const plan = importPlan({
    name: '@nimbleoffice/fixture',
    exports: {
      '.': { types: './dist/index.d.ts', import: './dist/index.js' },
      './data': './dist/data.json',
      './types-only': { types: './dist/types.d.ts' },
      './browser-only': { browser: './dist/browser.js' },
      './fallbacks': { import: ['./dist/a.js', './dist/b.js'] },
      './styles.css': './dist/styles.css',
      './*': './dist/*.js',
      './package.json': './package.json',
    },
  });
  expect(plan.entries).toEqual([
    { specifier: '@nimbleoffice/fixture', json: false },
    { specifier: '@nimbleoffice/fixture/data', json: true },
    { specifier: '@nimbleoffice/fixture/package.json', json: true },
  ]);
  expect(plan.failures.map((failure) => failure.split(':')[0])).toEqual([
    '@nimbleoffice/fixture/types-only',
    '@nimbleoffice/fixture/browser-only',
    '@nimbleoffice/fixture/fallbacks',
    '@nimbleoffice/fixture/styles.css',
    '@nimbleoffice/fixture/*',
  ]);
});

test('a root export array is refused', () => {
  expect(importPlan({ name: '@nimbleoffice/fixture', exports: [] }).failures).toHaveLength(1);
});

test('the editor stylesheet is the one export left out', () => {
  const plan = importPlan({
    name: '@nimbleoffice/docx-react',
    exports: { '.': { import: './dist/index.mjs' }, './styles.css': './dist/styles.css' },
  });
  expect(plan).toEqual({
    entries: [{ specifier: '@nimbleoffice/docx-react', json: false }],
    failures: [],
  });
});
