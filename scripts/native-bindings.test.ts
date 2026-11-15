import { describe, expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import {
  NATIVE_PACKAGES,
  NATIVE_FORMATS,
  pendingPublishNames,
  platformPackageVersions,
  validateNativeVersions
} from './native-bindings.mjs';


describe('native package registry', () => {
  const roots = NATIVE_PACKAGES.map((path) =>
    JSON.parse(readFileSync(new URL(`../${path}/package.json`, import.meta.url), 'utf8'))
  );
  const packages = [...roots, ...platformPackageVersions()];
  const versions = new Map<string, string>(packages.map(({ name, version }) => [name, version]));
  const packageName = (url: string) => decodeURIComponent(new URL(url).pathname.slice(1));
  const published = (name: string) => Response.json({ versions: { [versions.get(name)!]: {} } });

  test('registers every native format and platform package', () => {
    expect(NATIVE_FORMATS).toEqual(['docx', 'pptx', 'xlsx']);
    expect(NATIVE_PACKAGES).toEqual(NATIVE_FORMATS.map((format) => `bindings/${format}-native`));
    const platforms = [
      'darwin-arm64',
      'darwin-x64',
      'linux-arm64-gnu',
      'linux-arm64-musl',
      'linux-x64-gnu',
      'linux-x64-musl',
      'win32-arm64-msvc',
      'win32-x64-msvc'
    ];
    expect(
      platformPackageVersions()
        .map(({ name }) => name)
        .sort()
    ).toEqual(
      NATIVE_FORMATS.flatMap((format) =>
        platforms.map((platform) => `@nimbleoffice/${format}-native-${platform}`)
      ).sort()
    );
    expect(platformPackageVersions()).toHaveLength(NATIVE_PACKAGES.length * 8);
    expect(() => validateNativeVersions()).not.toThrow();
  });

  test('keeps roots private and platform packages public', () => {
    for (const path of NATIVE_PACKAGES) {
      const root = JSON.parse(
        readFileSync(new URL(`../${path}/package.json`, import.meta.url), 'utf8')
      );
      expect(root.private).toBe(true);
      for (const identity of readdirSync(new URL(`../${path}/npm`, import.meta.url))) {
        const platform = JSON.parse(
          readFileSync(new URL(`../${path}/npm/${identity}/package.json`, import.meta.url), 'utf8')
        );
        expect(platform.private).not.toBe(true);
        expect(platform.publishConfig.access).toBe('public');
      }
    }
  });

  test('selects the Node worker variant through a private conditional import', () => {
    for (const format of ['xlsx', 'pptx', 'docx']) {
      const extension = format === 'docx' ? 'mjs' : 'js';
      const directory = format === 'docx' ? 'yrs' : 'session';
      const core = JSON.parse(readFileSync(new URL(`../packages/${format}/package.json`, import.meta.url), 'utf8'));
      const native = JSON.parse(readFileSync(new URL(`../bindings/${format}-native/package.json`, import.meta.url), 'utf8'));
      const imports = {
        '#session-worker': {
          node: `./dist/sessionWorker.node.${extension}`,
          default: `./dist/sessionWorker.missing.${extension}`,
        },
      };
      expect(core.imports).toEqual(imports);
      expect(native.imports).toEqual(imports);
      const config = JSON.parse(readFileSync(new URL(`../packages/${format}/tsconfig.json`, import.meta.url), 'utf8'));
      expect(config.compilerOptions.paths['#session-worker']).toEqual([`./src/${directory}/sessionWorker.node.ts`]);
      expect(config.compilerOptions.paths['#session-scope']).toBeUndefined();
      const worker = format === 'docx' ? "'yrs/docxWorker" : `${format}SessionWorker`;
      for (const path of [`packages/${format}/tsup.config.ts`, `bindings/${format}-native/js/tsup.config.ts`]) {
        const build = readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
        expect(build).toMatch(/external: \[[^\]]*'#session-worker'\]/);
        expect(build).not.toContain('#session-scope');
        expect(build).toContain(`${worker}.node':`);
        expect(build).toContain("'sessionWorker.node':");
        expect(build).toContain("'sessionWorker.missing':");
      }
    }
  });

  test('detects only unpublished root versions', async () => {
    const requested: string[] = [];
    const pending = await pendingPublishNames({
      fetchImpl: async (url: string, options: RequestInit) => {
        const name = packageName(url);
        requested.push(name);
        expect(options.headers).toEqual({ Accept: 'application/vnd.npm.install-v1+json' });
        expect(options.signal).toBeInstanceOf(AbortSignal);
        return name === '@nimbleoffice/pptx-native' || !roots.some((root) => root.name === name)
          ? Response.json({ versions: {} })
          : published(name);
      }
    });
    expect(pending).toEqual(['pptx']);
    expect(requested.sort()).toEqual(packages.map(({ name }) => name).sort());
  });

  test('returns no formats when every root version is published', async () => {
    expect(
      await pendingPublishNames({
        fetchImpl: async (url: string) => published(packageName(url))
      })
    ).toEqual([]);
  });

  const missingSets: [string, string[]][] = [
    ['root name', ['@nimbleoffice/docx-native']],
    ['platform name', ['@nimbleoffice/pptx-native-linux-arm64-gnu']],
    ['root names', roots.map(({ name }) => name)],
    ['platform names', platformPackageVersions().map(({ name }) => name)],
    ['root and platform names', packages.map(({ name }) => name)]
  ];

  test.each(missingSets)('reports every missing %s', async (_label, missing) => {
    const requested: string[] = [];
    const failure = await pendingPublishNames({
      fetchImpl: async (url: string) => {
        const name = packageName(url);
        requested.push(name);
        return missing.includes(name)
          ? new Response('{"error":"missing"}', { status: 404 })
          : published(name);
      }
    }).catch((error: Error) => error);
    expect(failure).toBeInstanceOf(Error);
    expect((failure as Error).message).toBe(
      `Missing npm packages: ${requested.filter((name) => missing.includes(name)).join(', ')}. ` +
        'They must be published by hand first (RELEASING.md, "Native packages"), ' +
        'because Trusted Publishing cannot create a package.'
    );
    expect(requested.sort()).toEqual(packages.map(({ name }) => name).sort());
  });

  test('registry errors stop release detection', async () => {
    for (const failed of ['@nimbleoffice/docx-native', '@nimbleoffice/docx-native-linux-x64-gnu']) {
      await expect(
        pendingPublishNames({
          fetchImpl: async (url: string) => {
            const name = packageName(url);
            return name === failed ? new Response('unavailable', { status: 503 }) : published(name);
          }
        })
      ).rejects.toThrow(`npm answered 503 for ${failed}`);
    }
  });
});

describe('native package public names', () => {
  const read = (path: string) => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');

  test('root manifests carry the -native public name and binary', () => {
    for (const path of NATIVE_PACKAGES) {
      const manifest = JSON.parse(read(`${path}/package.json`));
      const format = path.replace(/^bindings\//, '').replace(/-native$/, '');
      expect(manifest.name).toBe(`@nimbleoffice/${format}-native`);
      expect(manifest.napi.packageName).toBe(`@nimbleoffice/${format}-native`);
      expect(manifest.napi.binaryName).toBe(`nimbleoffice-${format}-native`);
      expect(manifest.description).toContain('Node.js, Bun and Deno');
      expect(manifest.keywords).toContain('deno');
    }
  });

  test('the mimalloc notice carries both upstream copyrights', () => {
    const notice = read('bindings/docx-native/npm/linux-x64-gnu/MIT-mimalloc.txt');
    expect(notice).toContain('Copyright (c) 2018-2025 Microsoft Corporation, Daan Leijen');
    expect(notice).toContain('Copyright 2019 Octavian Oncescu');
    expect(notice.match(/THE SOFTWARE IS PROVIDED "AS IS"/gi)).toHaveLength(2);
  });

  test('platform packages match the root binary and version', () => {
    for (const path of NATIVE_PACKAGES) {
      const manifest = JSON.parse(read(`${path}/package.json`));
      const format = path.replace(/^bindings\//, '').replace(/-native$/, '');
      for (const identity of readdirSync(new URL(`../${path}/npm`, import.meta.url))) {
        const platform = JSON.parse(read(`${path}/npm/${identity}/package.json`));
        expect(platform.version).toBe(manifest.version);
        expect(platform.name.startsWith(`@nimbleoffice/${format}-native-`)).toBe(true);
        const triple = platform.name.replace(`@nimbleoffice/${format}-native-`, '');
        expect(identity).toBe(triple);
        expect(platform.main).toBe(`${manifest.napi.binaryName}.${triple}.node`);
        const allocator = format === 'xlsx' ? [] : ['MIT-mimalloc.txt'];
        expect(platform.files).toEqual([platform.main, 'BSD-3-Clause-Zstandard.txt', 'MIT-wuff.txt', ...allocator]);
        for (const notice of allocator) {
          expect(read(`${path}/npm/${identity}/${notice}`)).toBe(read('bindings/docx-native/npm/linux-x64-gnu/MIT-mimalloc.txt'));
        }
        expect(read(`${path}/npm/${identity}/BSD-3-Clause-Zstandard.txt`))
          .toBe(read('crates/nimbleoffice-fonts/vendor/zstd/LICENSE'));
        expect(read(`${path}/npm/${identity}/MIT-wuff.txt`)).toBe(read('packages/fonts/LICENSES/MIT-wuff.txt'));
        expect(platform.description).toBe(manifest.description);
        expect(platform.keywords).toEqual(manifest.keywords);
      }
    }
  });

  test('ignores generated native loaders and keeps declarations', () => {
    expect(read('.gitignore').split('\n')).toContain('bindings/*-native/index.js');
    const result = spawnSync(
      'git',
      [
        'check-ignore',
        '--no-index',
        '--',
        ...NATIVE_PACKAGES.flatMap((path) => [`${path}/index.js`, `${path}/index.d.ts`])
      ],
      { cwd: fileURLToPath(new URL('../', import.meta.url)), encoding: 'utf8' }
    );
    expect(result.status).toBe(0);
    expect(result.stdout.trim().split('\n')).toEqual(
      NATIVE_PACKAGES.map((path) => `${path}/index.js`)
    );
  });

  test('every native function catches panics, so a panic fails its call instead of aborting the process', () => {
    for (const path of NATIVE_PACKAGES) {
      const source = read(`${path}/src/engine.rs`);
      const functions = [...source.matchAll(/#\[napi(\((?:[^\]]|\n)*?\))?\]\s*(?:(?:\/\/\/[^\n]*\n\s*|#\[[^\n]*\]\s*)*)pub\s+fn\s+(\w+)/g)];
      expect(functions.length).toBeGreaterThan(0);
      expect(functions.filter((match) => !match[1]?.includes('catch_unwind')).map((match) => match[2])).toEqual([]);
    }
  });

  test('panic test hooks stay out of shipped binaries and still run in CI', () => {
    const ci = Bun.YAML.parse(read('.github/workflows/ci.yml')) as any;
    const hooks = ci.jobs['native-hooks'];
    expect(hooks.name).toBe('Native panic hooks');
    expect(hooks['timeout-minutes']).toBe(30);
    expect(hooks.needs).toBeUndefined();
    expect(hooks.steps.find((step: any) => step.uses === 'Swatinem/rust-cache@v2').with)
      .toEqual({ workspaces: 'bindings', key: 'native-hooks' });
    const panics = hooks.steps.find((step: any) => step.name === 'Test panics through the test hooks');
    expect(panics.env.NIMBLEOFFICE_NATIVE_TEST_HOOKS).toBe('1');
    expect(panics.run).toContain('bunx napi build --cwd "$package" --platform --profile hooks --features test-hooks -- --locked');
    expect(panics.run).toContain('bun test "$package/panic.test.ts"');
    expect(JSON.stringify(ci.jobs.native)).not.toContain('test-hooks');
    const profile = Bun.TOML.parse(read('bindings/Cargo.toml')) as any;
    expect(profile.profile.hooks).toEqual({ inherits: 'release', lto: false });
    expect(profile.profile.release.panic).toBeUndefined();
    for (const path of NATIVE_PACKAGES) {
      expect(read(`${path}/Cargo.toml`)).toContain('test-hooks = []');
      expect(read(`${path}/Cargo.toml`)).not.toMatch(/default\s*=.*test-hooks/);
      expect(read(`${path}/index.d.ts`)).not.toContain('ForTest');
    }
  });

});
