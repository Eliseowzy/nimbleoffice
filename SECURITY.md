# Security Policy

## Reporting a Vulnerability

Report vulnerabilities privately to the maintainers. Please do not open a public issue.

We aim to acknowledge reports within 5 business days and will keep you updated on the fix and disclosure timeline.

## Audit exemptions

`bun audit --audit-level=high` gates every branch. An advisory is exempted only
when it cannot be reached in shipped code and no upgrade path exists that does
not cause a worse problem. Each exemption is listed here with its reason, and
`bun audit` still reports it unfiltered so it stays visible.

### GHSA-vfj7-8cjw-p6xm: braces stack exhaustion on deeply nested patterns

Reached only through build and dev tooling: `tsup` in every package's
devDependencies (via `chokidar`). These expand glob patterns from this
repository's own configuration and never see user input. No published package
depends on braces at runtime.

No patched release exists: every braces version up to the latest, 3.0.3, is
affected.

Remove this exemption when a patched braces release is published.
