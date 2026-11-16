import { GlobalRegistrator } from '@happy-dom/global-registrator';
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { preloadEditWasm } from '../wasm/edit';
import { renderDocument } from './renderDocument';

const ownsDom = !GlobalRegistrator.isRegistered;
if (ownsDom) GlobalRegistrator.register();
afterAll(async () => {
  if (ownsDom) await GlobalRegistrator.unregister();
});

const observers: FakeObserver[] = [];
class FakeObserver {
  readonly targets: Element[] = [];
  disconnected = false;
  constructor(readonly callback: (entries: Partial<IntersectionObserverEntry>[]) => void) {
    observers.push(this);
  }
  observe(target: Element) { this.targets.push(target); }
  disconnect() { this.disconnected = true; }
  show(indexes: number[], visible: boolean) {
    this.callback(indexes.map((index) => ({ target: this.targets[index], isIntersecting: visible })));
  }
}

const noopContext = new Proxy({}, {
  get: (_, key) => key === 'measureText' ? () => ({ width: 0 }) : key === 'canvas' ? undefined : () => {},
  set: () => true,
});

beforeAll(async () => {
  globalThis.IntersectionObserver = FakeObserver as unknown as typeof IntersectionObserver;
  HTMLCanvasElement.prototype.getContext = (() => noopContext) as never;
  await preloadEditWasm(new Uint8Array(
    readFileSync(resolve(import.meta.dir, '../wasm/generated/edit/docx_edit_bg.wasm'))
  ));
});

const demo = new Uint8Array(
  readFileSync(resolve(import.meta.dir, '../../../../fixtures/demo/nimbleoffice-demo.docx'))
);

const settle = () => new Promise((done) => setTimeout(done, 50));

test('lays out every page and paints only pages near the viewport', async () => {
  const container = document.createElement('div');
  let firstPage = 0;
  const preview = await renderDocument(demo, container, {
    devicePixelRatio: 1,
    onFirstPage: () => firstPage++,
  });
  const observer = observers.at(-1)!;
  const slots = [...container.firstElementChild!.children] as HTMLElement[];

  expect(preview.pages.length).toBeGreaterThan(1);
  expect(slots).toHaveLength(preview.pages.length);
  expect(slots.every((slot) => !slot.firstChild)).toBe(true);
  expect(slots[0]!.style.width).toBe(`${preview.pages[0]!.width}px`);

  observer.show([0], true);
  await settle();
  expect(firstPage).toBe(1);
  expect(slots.map((slot) => slot.childElementCount)).toEqual(slots.map((_, index) => index === 0 ? 1 : 0));

  observer.show([0], false);
  observer.show([1], true);
  await settle();
  expect(slots[0]!.firstChild).toBeNull();
  expect(slots[1]!.firstChild).not.toBeNull();

  observer.show([0], true);
  await settle();
  expect(firstPage).toBe(1);

  preview.destroy();
  expect(observer.disconnected).toBe(true);
  expect(container.childElementCount).toBe(0);
});

test('an aborted signal destroys the mounted preview', async () => {
  const container = document.createElement('div');
  const controller = new AbortController();
  await renderDocument(demo, container, { signal: controller.signal });
  expect(container.childElementCount).toBe(1);
  controller.abort();
  expect(container.childElementCount).toBe(0);
  await expect(renderDocument(demo, container, { signal: controller.signal })).rejects.toThrow();
});
