// Runs `bun test <args>` and fails only on failures missing from scripts/known-test-failures.txt.
//
//   bun scripts/test-known-failures.ts --isolate packages/xlsx
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const root = resolve(import.meta.dir, '..');

const decode = (value: string) =>
  value.replace(/&#(\d+);/g, (_, code) => String.fromCharCode(Number(code)))
    .replace(/&quot;/g, '"').replace(/&apos;/g, "'").replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&');

/** `file: describe > test` for every failed test case in a bun JUnit report. */
export function failures(xml: string): string[] {
  const found: string[] = [];
  const suites: string[] = [];
  let file = '';
  for (const [tag] of xml.matchAll(/<\/?testsuite\b[^>]*>|<testcase\b[^>]*\/>|<testcase\b[^>]*>[\s\S]*?<\/testcase>/g)) {
    if (tag.startsWith('</testsuite')) { suites.pop(); continue; }
    const attribute = (name: string) => decode(tag.match(new RegExp(`\\b${name}="([^"]*)"`))?.[1] ?? '');
    if (tag.startsWith('<testsuite')) {
      if (!suites.length) file = attribute('file');
      suites.push(attribute('name'));
      continue;
    }
    if (!/<(failure|error)\b/.test(tag)) continue;
    found.push(`${attribute('file') || file}: ${[...suites.slice(1), attribute('name')].join(' > ')}`);
  }
  return found;
}

if (import.meta.main) {
  const known = new Set(
    readFileSync(join(root, 'scripts/known-test-failures.txt'), 'utf8')
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line && !line.startsWith('#')),
  );
  const directory = mkdtempSync(join(tmpdir(), 'known-failures-'));
  const report = join(directory, 'junit.xml');
  const run = Bun.spawnSync(['bun', 'test', '--reporter=junit', `--reporter-outfile=${report}`, ...process.argv.slice(2)], {
    cwd: root, stdio: ['inherit', 'inherit', 'inherit'], env: process.env,
  });
  let xml = '';
  try { xml = readFileSync(report, 'utf8'); } catch {}
  rmSync(directory, { recursive: true, force: true });
  if (!xml) {
    console.error(`bun test wrote no report (exit ${run.exitCode})`);
    process.exit(run.exitCode || 1);
  }
  const failed = failures(xml);
  const unexpected = failed.filter((key) => !known.has(key));
  const tolerated = failed.length - unexpected.length;
  if (tolerated) console.log(`\n${tolerated} known failure(s) tolerated (scripts/known-test-failures.txt)`);
  if (unexpected.length) {
    console.error(`\n${unexpected.length} failure(s) not in scripts/known-test-failures.txt:`);
    for (const key of unexpected) console.error(`  ${key}`);
    process.exit(1);
  }
  if (!failed.length && run.exitCode) {
    console.error(`bun test exited ${run.exitCode} with no failed test case`);
    process.exit(run.exitCode);
  }
}
