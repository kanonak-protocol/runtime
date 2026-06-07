# Publishing the Kanonak runtime libraries

This repo holds the **public, open-source** runtime stack that every generated
Kanonak SDK depends on, so the runtime is **never inlined** into each SDK:

- **`kanonak-canonical`** — the canonical form + content hash (reference
  implementation of `kanonak.org/canonical-form`, `canonicalFormVersion "1"`).
- **`kanonak-codec`** — the generic codec runtime; builds the canonical input
  from typed nodes + an embedded schema, content-addresses via `canonical`, and
  (de)serializes the wire form. **Depends on `canonical`.**

The machine-readable source of truth for every registry connection is
[`release-targets.yml`](./release-targets.yml). This document is the human guide.

## Identity / naming

One identity (**kanonak**, domain **kanonak.org**) rendered per registry idiom +
the bare lib name (`canonical` / `codec`):

| Language | Registry | `canonical` | `codec` |
|---|---|---|---|
| Python | PyPI | `kanonak-canonical` | `kanonak-codec` |
| Rust | crates.io | `kanonak-canonical` | `kanonak-codec` |
| TypeScript | npm | `@kanonak-protocol/canonical` | `@kanonak-protocol/codec` |
| C# | NuGet | `Kanonak.Canonical` | `Kanonak.Codec` |
| Java | Maven Central | `org.kanonak:kanonak-canonical` | `org.kanonak:kanonak-codec` |
| Go | module proxy | `github.com/kanonak-protocol/runtime/kanonak-canonical/go` | `.../kanonak-codec/go` |

Every package: `homepage` = `https://kanonak.org`, `repository` =
`https://github.com/kanonak-protocol/runtime`, license **Apache-2.0**.

## Connection + secrets matrix

Full detail (accounts, registry-side trusted-publisher config, setup steps) is in
`release-targets.yml`. Summary:

| Language | Registry | Auth | GitHub secrets needed |
|---|---|---|---|
| Python | PyPI | OIDC trusted publishing | — (fallback `PYPI_API_TOKEN`) |
| Rust | crates.io | OIDC trusted publishing | — (fallback `CARGO_REGISTRY_TOKEN`) |
| TypeScript | npm | OIDC + `--provenance` | — (fallback `NPM_TOKEN`) |
| C# | NuGet.org | API key | `NUGET_API_KEY` |
| Java | Maven Central | Portal token + GPG | `MAVEN_CENTRAL_USERNAME`, `MAVEN_CENTRAL_PASSWORD`, `MAVEN_GPG_PRIVATE_KEY`, `MAVEN_GPG_PASSPHRASE` |
| Go | module proxy | none | — |

**Recommended-path secret set (OIDC for PyPI/crates/npm):**
`NUGET_API_KEY`, `MAVEN_CENTRAL_USERNAME`, `MAVEN_CENTRAL_PASSWORD`,
`MAVEN_GPG_PRIVATE_KEY`, `MAVEN_GPG_PASSPHRASE`. Everything else is keyless.

With OIDC, you configure a **trusted publisher on the registry** (pointing at
`kanonak-protocol/runtime` + the release workflow) instead of storing a token —
see each target's `registry_side_config` in `release-targets.yml`.

## Release pipeline

- **Trigger:** tag `v<semver>` (or manual dispatch); default a **dry run**, set
  `publish=true` to push.
- **Order (always):** `canonical` → wait for the index → `codec`, per language.
- **Gating:** the per-language **conformance tests against `*/vectors/`** must
  pass before publish — that's what guarantees a byte-identical content address
  across all six languages.
- **OIDC workflow permissions:**
  `permissions: { id-token: write, contents: read, attestations: write }`.
- **Versions are immutable;** bump the manifests before a real release.

## One-time setup checklist (per `release-targets.yml`)

1. Reserve names: PyPI (`kanonak-canonical`/`kanonak-codec`), crates.io (same),
   NuGet `Kanonak.*` prefix, npm org `@kanonak-protocol`, Sonatype `org.kanonak`.
2. Configure trusted publishers (PyPI, crates.io, npm) → repo + `release.yml`.
3. Create the secrets listed above (NuGet key; Maven Portal token + GPG).
4. Go: push subdir tags `kanonak-canonical/go/vX.Y.Z` then `kanonak-codec/go/vX.Y.Z` (no infra; canonical first).
