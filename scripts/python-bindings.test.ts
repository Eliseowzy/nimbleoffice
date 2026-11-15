import { describe, expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  FONT_PACK_NAMES,
  PYPI_DISTRIBUTIONS,
  PYTHON_BINDINGS,
  PYTHON_BINDING_NAMES,
  PYTHON_PUBLISH_NAMES,
  bindingVersion,
  fontPackVersion,
  pendingPublishNames
} from './python-bindings.mjs';

const script = fileURLToPath(new URL('./python-bindings.mjs', import.meta.url));
const repository = fileURLToPath(new URL('..', import.meta.url));

function cli(...args: string[]) {
  const result = spawnSync('node', [script, ...args], { encoding: 'utf8' });
  expect(result.status).toBe(0);
  return result.stdout.trim();
}

describe('registry', () => {
  test('every publishable binding is a registered binding', () => {
    expect(PYTHON_BINDING_NAMES).toEqual(PYTHON_BINDINGS.map((p) => p.split('/python-')[1]));
    for (const name of PYTHON_PUBLISH_NAMES) {
      expect(PYTHON_BINDING_NAMES).toContain(name);
    }
  });

  test('docx publishes', () => {
    expect(PYTHON_BINDINGS).toContain('bindings/python-docx');
    expect(PYTHON_BINDING_NAMES).toContain('docx');
    expect(PYTHON_PUBLISH_NAMES).toContain('docx');
    expect(PYPI_DISTRIBUTIONS).toContain('nimbleoffice-docx');
  });

  test('pptx publishes', () => {
    expect(PYTHON_BINDINGS).toContain('bindings/python-pptx');
    expect(PYTHON_BINDING_NAMES).toContain('pptx');
    expect(PYTHON_PUBLISH_NAMES).toContain('pptx');
    expect(PYPI_DISTRIBUTIONS).toContain('nimbleoffice-pptx');
  });

  test('xlsx publishes', () => {
    expect(PYTHON_PUBLISH_NAMES).toContain('xlsx');
    expect(PYPI_DISTRIBUTIONS).toContain('nimbleoffice-xlsx');
  });

  test('a held-back binding stays out of the publish matrix', () => {
    const held = PYTHON_BINDING_NAMES.filter((name) => !PYTHON_PUBLISH_NAMES.includes(name));
    for (const name of held) {
      expect(PYPI_DISTRIBUTIONS).not.toContain(`nimbleoffice-${name}`);
    }
    expect(PYPI_DISTRIBUTIONS).toHaveLength(PYTHON_PUBLISH_NAMES.length);
  });

  test('the CLI reports the registry three ways', () => {
    expect(cli('--paths').split('\n')).toEqual(PYTHON_BINDINGS);
    expect(JSON.parse(cli())).toEqual(PYTHON_BINDING_NAMES);
    expect(JSON.parse(cli('--publish'))).toEqual(PYTHON_PUBLISH_NAMES);
  });

  test('an unknown argument fails instead of printing every binding', () => {
    const result = spawnSync('node', [script, '--publsh'], { encoding: 'utf8' });
    expect(result.status).toBe(1);
    expect(result.stderr).toContain('--publsh');
    expect(result.stdout).toBe('');
  });
});

describe('font pack wheels', () => {
  const versions = Object.fromEntries(FONT_PACK_NAMES.map((name) => [name, fontPackVersion(name)]));

  test('every pack version is a semver', () => {
    for (const version of Object.values(versions)) expect(version).toMatch(/^\d+\.\d+\.\d+$/);
  });

  test('every publishable format binding pins all three packs at the committed versions', () => {
    for (const name of PYTHON_PUBLISH_NAMES) {
      const pyproject = Bun.TOML.parse(
        readFileSync(join(repository, `bindings/python-${name}/pyproject.toml`), 'utf8')
      ) as any;
      expect(pyproject.project.dependencies).toEqual(
        FONT_PACK_NAMES.map((pack) => `nimbleoffice-${pack}==${versions[pack]}`)
      );
    }
  });

  test('the builder writes a py3-none-any wheel per pack with every face and the font licenses', () => {
    const out = mkdtempSync(join(tmpdir(), 'font-packs-'));
    try {
      const result = spawnSync('python3', ['-I', 'scripts/python_font_packs.py', '--out', out], {
        cwd: repository,
        encoding: 'utf8'
      });
      expect(result.status, result.stderr).toBe(0);
      for (const name of FONT_PACK_NAMES) {
        const pack = name.replace('fonts-', '');
        const version = versions[name];
        const wheel = `nimbleoffice_fonts_${pack}-${version}-py3-none-any.whl`;
        expect(result.stdout).toContain(wheel);
        const listing = spawnSync(
          'python3',
          ['-I', '-c', 'import sys, zipfile; print("\\n".join(zipfile.ZipFile(sys.argv[1]).namelist()))', join(out, wheel)],
          { encoding: 'utf8' }
        ).stdout.trim().split('\n');
        const manifest = JSON.parse(
          readFileSync(join(repository, `packages/${name}/assets/manifest.json`), 'utf8')
        );
        const data = listing.filter((entry) => entry.startsWith('nimbleoffice_fonts_packs/'));
        expect(data.sort()).toEqual(
          ['manifest.json', ...manifest.faces.map((face: { file: string }) => face.file)]
            .map((file) => `nimbleoffice_fonts_packs/${pack}/${version}/${file}`)
            .sort()
        );
        const info = `nimbleoffice_fonts_${pack}-${version}.dist-info`;
        for (const file of ['METADATA', 'WHEEL', 'RECORD', 'licenses/LICENSE']) {
          expect(listing).toContain(`${info}/${file}`);
        }
        expect(listing.some((entry) => /licenses\/LICENSES\/OFL-.+\.txt$/.test(entry))).toBe(true);
      }
    } finally {
      rmSync(out, { recursive: true, force: true });
    }
  }, 60_000);
});

describe('pending publish selection', () => {
  const live = () =>
    Object.fromEntries([
      ...FONT_PACK_NAMES.map((name) => [`nimbleoffice-${name}`, [fontPackVersion(name)]]),
      ...PYTHON_BINDINGS.map((path) => [
        `nimbleoffice-${path.replace('bindings/python-', '')}`,
        [bindingVersion(path)]
      ])
    ]);

  type Files = { yanked?: boolean }[];

  function pypi(releases: Record<string, string[] | Record<string, Files> | null>) {
    const requested: string[] = [];
    const fetchImpl = async (url: string) => {
      requested.push(url);
      const project = url.match(/\/pypi\/([^/]+)\/json$/)?.[1] ?? '';
      const entry = releases[project];
      if (entry == null) return { status: 404, ok: false } as Response;
      const map = Array.isArray(entry)
        ? Object.fromEntries(entry.map((version) => [version, [{ yanked: false }]]))
        : entry;
      return {
        status: 200,
        ok: true,
        json: async () => ({ releases: map })
      } as unknown as Response;
    };
    return { fetchImpl, requested };
  }

  test('every binding version is a semver', () => {
    for (const path of PYTHON_BINDINGS) expect(bindingVersion(path)).toMatch(/^\d+\.\d+\.\d+$/);
  });

  test('nothing is pending when PyPI serves every version', async () => {
    const { fetchImpl, requested } = pypi(live());
    expect(await pendingPublishNames({ fetchImpl })).toEqual([]);
    expect(requested).toEqual(
      [...FONT_PACK_NAMES.map((name) => `nimbleoffice-${name}`), ...PYPI_DISTRIBUTIONS].map(
        (d) => `https://pypi.org/pypi/${d}/json`
      )
    );
  });

  test('a font pack version PyPI does not serve is pending, ahead of the bindings', async () => {
    const releases = live();
    releases['nimbleoffice-fonts-cjk'] = null;
    releases['nimbleoffice-docx'] = ['0.0.0'];
    const { fetchImpl } = pypi(releases);
    expect(await pendingPublishNames({ fetchImpl })).toEqual(['fonts-cjk', 'docx']);
  });

  test('a version PyPI does not serve is pending', async () => {
    const releases = live();
    releases['nimbleoffice-docx'] = ['0.0.0'];
    const { fetchImpl } = pypi(releases);
    expect(await pendingPublishNames({ fetchImpl })).toEqual(['docx']);
  });

  test('a project PyPI does not know is pending', async () => {
    const releases = live();
    releases['nimbleoffice-xlsx'] = null;
    const { fetchImpl } = pypi(releases);
    expect(await pendingPublishNames({ fetchImpl })).toEqual(['xlsx']);
  });

  test('a version whose files are all deleted fails instead of counting as released', async () => {
    const releases = live();
    releases['nimbleoffice-docx'] = { [bindingVersion('bindings/python-docx')]: [] };
    const { fetchImpl } = pypi(releases);
    await expect(pendingPublishNames({ fetchImpl })).rejects.toThrow('no installable file');
  });

  test('a version whose files are all yanked fails instead of counting as released', async () => {
    const releases = live();
    releases['nimbleoffice-docx'] = {
      [bindingVersion('bindings/python-docx')]: [{ yanked: true }, { yanked: true }]
    };
    const { fetchImpl } = pypi(releases);
    await expect(pendingPublishNames({ fetchImpl })).rejects.toThrow('no installable file');
  });

  test('one live file among yanked ones counts as released', async () => {
    const releases = live();
    releases['nimbleoffice-docx'] = {
      [bindingVersion('bindings/python-docx')]: [{ yanked: true }, { yanked: false }]
    };
    const { fetchImpl } = pypi(releases);
    expect(await pendingPublishNames({ fetchImpl })).toEqual([]);
  });

  test('a version is read from [package], not another table', () => {
    const fixture = 'scripts/.manifest-fixture';
    const directory = join(repository, fixture);
    const read = (body: string) => {
      mkdirSync(directory, { recursive: true });
      writeFileSync(join(directory, 'Cargo.toml'), body);
      try {
        return bindingVersion(fixture);
      } catch (error) {
        return `THROWS: ${(error as Error).message}`;
      }
    };

    try {
      expect(read('[package]\nversion = "1.2.3" # release\n')).toBe('1.2.3');
      expect(read('[other]\nversion = "9.9.9"\n\n[package]\nversion = "1.2.3"\n')).toBe('1.2.3');
      expect(read('[package]\nversion.workspace = true\n\n[other]\nversion = "9.9.9"\n')).toContain(
        'THROWS'
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  test('an unreadable PyPI fails the run instead of skipping a publish', async () => {
    const fetchImpl = async () => ({ status: 503, ok: false }) as Response;
    await expect(
      pendingPublishNames({ fetchImpl, attempts: 2, retryDelayMs: 0 })
    ).rejects.toThrow('cannot read PyPI');
  });

  test('a transient error is retried', async () => {
    let calls = 0;
    const good = pypi(live()).fetchImpl;
    const fetchImpl = async (url: string) => {
      calls += 1;
      if (calls === 1) throw new Error('reset');
      return good(url);
    };
    expect(await pendingPublishNames({ fetchImpl, retryDelayMs: 0 })).toEqual([]);
  });
});
