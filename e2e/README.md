# Editor end-to-end tests

Chromium drives the production DOCX, XLSX, and PPTX React editors through the
browser-capable desktop app.

## Browser tests

```bash
bun install --frozen-lockfile
bunx playwright install chromium
bun run test:e2e:browser
```

The command builds the production packages and app, starts a local preview
server, and runs headless Chromium. Each format opens a committed demo document,
edits using browser input, verifies undo and redo, downloads the saved document,
reopens it, and checks that attempting to open a corrupt document preserves the
current editor. The tests inspect the saved OOXML as well as the editor state.
They use bundled fonts and reject external HTTP requests. No deployed service,
credentials, or desktop window is required.

Playwright manages server startup and shutdown. Tests run with one worker and no
retries. Failure screenshots, traces, reports, and saved files go under
`.source/e2e/browser` (`NIMBLEOFFICE_BROWSER_OUTPUT` overrides). After a build,
`bunx playwright test --config e2e/browser/playwright.config.ts` reruns
just the browser tests.

A few docx tests drive the editor through a browser harness that only a
`DOCX_PEERLESS_HARNESS=1` build of `@nimbleoffice/docx-react` and the desktop
app exposes. They are tagged `@harness` and skip unless the variable is set;
with it set, a preview built without it fails them at once. Build both and run
them with the variable set each time:
`DOCX_PEERLESS_HARNESS=1 bun run --filter @nimbleoffice/docx-react build`,
`DOCX_PEERLESS_HARNESS=1 bun run --filter @nimbleoffice/desktop build`, then
`DOCX_PEERLESS_HARNESS=1 bunx playwright test --config e2e/browser/playwright.config.ts --grep @harness`.
CI runs them in the third browser shard.

CI splits both browser configs into shards by measured time per file
(`e2e/browser/shard-weights.json`), set with `NIMBLEOFFICE_BROWSER_SHARD=1/3`.
After the suite changes a lot, refresh the weights from a CI run's
`report.json` and `plugin-host-report.json` files:
`bun scripts/browser-shard-weights.ts <reports>...`.
