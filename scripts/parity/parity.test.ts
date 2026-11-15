import { expect, test } from 'bun:test';

import { compareOrWrite, formats, readExpected, run } from './harness';

for (const [format, target] of Object.entries(formats)) {
  test(`${format} scripted API parity matches the shared golden`, async () => {
    expect(target.engineKind).toBe(
      (globalThis as Record<symbol, unknown>)[Symbol.for('nimbleoffice.testEngine')] ?? 'wasm'
    );
    const results = await run(format);
    compareOrWrite(format, results, process.env.NIMBLEOFFICE_PARITY_WRITE === '1');
    expect(results).toEqual(readExpected(format));
  });
}
