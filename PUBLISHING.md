# Publishing the Kanonak runtime libraries

This repo holds the **public, open-source** runtime stack that every generated
Kanonak SDK depends on, so the runtime is **never inlined** into each SDK:

- **`kanonak-canonical`** — the canonical form + content hash (reference
  implementation of `kanonak.org/canonical-form`, `canonicalFormVersion "1"`).
- **`kanonak-codec`** — the generic codec runtime; builds the canonical input
  from typed nodes + an embedded schema, content-addresses via `canonical`, and
  (de)serializes the wire form. **Depends on `canonical`.**
- **`kanonak-expression`** — the deterministic expression runtime
  (`expressionRuntimeVersion "1"`). No cross-dependencies.
- **`kanonak-wire`** — the binary wire kernel for hot-path protocols
  (`wireFormatVersion "1"`). No cross-dependencies.

The machine-readable source of truth for every registry connection is
[`release-targets.yml`](./release-targets.yml). This document is the human guide.

## Identity / naming

One identity (**kanonak**, domain **kanonak.org**) rendered per registry idiom +
the bare lib name (`canonical` / `codec` / `expression` / `wire`):

| Language | Registry | `canonical` | `codec` | `expression` | `wire` |
|---|---|---|---|---|---|
| Python | PyPI | `kanonak-canonical` | `kanonak-codec` | `kanonak-expression` | `kanonak-wire` |
| Rust | crates.io | `kanonak-canonical` | `kanonak-codec` | `kanonak-expression` | `kanonak-wire` |
| TypeScript | npm | `@kanonak-protocol/canonical` | `@kanonak-protocol/codec` | `@kanonak-protocol/expression` | `@kanonak-protocol/wire` |
| C# | NuGet | `Kanonak.Canonical` | `Kanonak.Codec` | `Kanonak.Expression` | `Kanonak.Wire` |
| Java | Maven Central | `org.kanonak:kanonak-canonical` | `org.kanonak:kanonak-codec` | `org.kanonak:kanonak-expression` | `org.kanonak:kanonak-wire` |
| Go | module proxy | `github.com/kanonak-protocol/runtime/kanonak-canonical/go` | `.../kanonak-codec/go` | `.../kanonak-expression/go` | `.../kanonak-wire/go` |
| Wasm component | GHCR (OCI) | — | `ghcr.io/kanonak-protocol/codec` | — | — |
| Swift | this repo + root `v*` tags | product `KanonakCanonical` | product `KanonakCodec` | — | — |

Every package: `homepage` = `https://kanonak.org`, `repository` =
`https://github.com/kanonak-protocol/runtime`, license **Apache-2.0**.

**The Wasm component is codec-only** (the 7th codec port). It has no
language-native registry — the Component-Model ecosystem distributes via OCI
artifacts — so it ships as a wkg-format Wasm OCI Artifact on GHCR, tagged with
the codec version (native `codec@X.Y.Z` and `codec:X.Y.Z` are one coordinated
release). Fetch: `wkg oci pull ghcr.io/kanonak-protocol/codec:<ver> -o codec.wasm`.

**Swift has no central registry.** SwiftPM resolves a public git URL against
ROOT semver tags, so a Swift release IS a root `v<ver>` tag (one version
covers both products, behind the root `Package.swift`) — pushed by the
`publish-swift` job from `meta.swift_package_version` in
`release-targets.yml`, NEVER manually (the release workflow is the one thing
anyone must remember to run). Idempotent: an existing tag is never touched;
bump `swift_package_version` and re-run for a new release. Root tags
predating `Package.swift` (v0.2.0–v0.4.0) cannot resolve; consumers pin
`from:` the first tag that contains it. Consume:
`.package(url: "https://github.com/kanonak-protocol/runtime", from: "<ver>")`
+ products `KanonakCanonical` / `KanonakCodec`.

## Connection + secrets matrix

Full detail (accounts, registry-side trusted-publisher config, setup steps) is in
`release-targets.yml`. Summary:

| Language | Registry | Auth | GitHub secrets needed |
|---|---|---|---|
| Python | PyPI | OIDC trusted publishing | — (fallback `PYPI_API_TOKEN`) |
| Rust | crates.io | OIDC trusted publishing | — (fallback `CARGO_REGISTRY_TOKEN`) |
| TypeScript | npm | OIDC + `--provenance` | — (fallback `NPM_TOKEN`) |
| C# | NuGet.org | OIDC trusted publishing | — (fallback `NUGET_API_KEY`) |
| Java | Maven Central | Portal token + GPG | `MAVEN_CENTRAL_PASSWORD`, `MAVEN_GPG_PRIVATE_KEY`, `MAVEN_GPG_PASSPHRASE` (+ var `MAVEN_CENTRAL_USERNAME`) |
| Go | module proxy | none | — |
| Wasm | GHCR (OCI) | ambient `GITHUB_TOKEN` (`packages: write`) | — |
| Swift | this repo (root `v*` tags) | none | — |

**Recommended-path secret set (OIDC for PyPI/crates/npm/NuGet):**
`MAVEN_CENTRAL_PASSWORD`, `MAVEN_GPG_PRIVATE_KEY`, `MAVEN_GPG_PASSPHRASE` (Maven
only). `NUGET_API_KEY` only if you skip NuGet OIDC.

**Variables (readable, not secret):** `NUGET_USER` (= `kanonak-oss`) and
`MAVEN_CENTRAL_USERNAME` (the Sonatype token username).

**All publish jobs run in the `production` GitHub environment** — configure each
trusted publisher (PyPI/crates/npm/NuGet) with environment `production` so the
OIDC claim matches, and optionally add a required-reviewer rule to gate releases.

With OIDC, you configure a **trusted publisher on the registry** (pointing at
`kanonak-protocol/runtime` + the release workflow) instead of storing a token —
see each target's `registry_side_config` in `release-targets.yml`.

## Release pipeline

- **Trigger:** tag `v<semver>` (or manual dispatch); default a **dry run**, set
  `publish=true` to push.
- **Order (always):** `canonical` → wait for the index → `codec`, per language.
  `expression` and `wire` have no cross-dependencies and ride the same pipeline
  in any order.
- **Gating:** the per-language **conformance tests against `*/vectors/`** must
  pass before publish — that's what guarantees a byte-identical content address
  across all six languages.
- **OIDC workflow permissions:**
  `permissions: { id-token: write, contents: read, attestations: write }`
  (job-level overrides: `publish-go` takes `contents: write` to push tags;
  `publish-wasm` takes `packages: write` to push the GHCR artifact).
- **Versions are immutable;** bump the manifests before a real release.

## The cold-start contract (WE WILL FORGET — the machinery remembers)

The workflow is the ONLY release path. There are **no manual steps**, no
required context beyond this repo, and **any run is safe at any time** —
including re-running `publish=true` on an already-released state. Every stage
is idempotent against its registry's AUTHORITATIVE source, because read-side
pre-checks provably lag fresh publishes (both bit us on 2026-07-03):

| Registry | Idempotency mechanism |
|---|---|
| PyPI | `skip-existing` (server-side) |
| crates.io | pre-check against crates.io's own API (same system as the write path) |
| npm | `npm view` pre-check (replica, optimization only) + the registry's E403 "cannot publish over previously published versions" treated as already-published |
| NuGet | `--skip-duplicate` (server-side) |
| Maven Central | repo1 pre-check (mirror, optimization only) + on failure the Portal **status API** is queried and a duplicate deployment is treated as already-published |
| Go | `publish-go` job: tag exists → skip (tags are immutable once proxied) |
| GHCR (Wasm) | authenticated manifest pre-check → skip existing tag (release tags are immutable by discipline; the auth means the check works even before the visibility flip) |
| Swift | `publish-swift` job: root `v<ver>` tag exists → skip (immutable once resolved; bump `swift_package_version` for a new release) |

**Go has no registry.** A release IS the git tag
`kanonak-<member>/go/v<version>` (from `meta.go_module_versions` in
`release-targets.yml`) — pushed by the `publish-go` job, never by hand.
GitHub Releases are irrelevant; `proxy.golang.org` fetches the tag on first
`go get` and caches it **forever** (never move/reuse a tag). A missing tag is
*silent* drift — consumers get unpinned pseudo-versions with no error — which
is why the audit below hard-fails on it.

**Every publish run ends with `release-audit`:** it derives each member's
expected versions from the committed manifests (repo state is the only
memory) and checks all six registries + Go tags. Authoritative sources
missing = red, with the exact thing to check in the error. Mirror-backed
reads (npm replicas, NuGet CDN, repo1) may report **PENDING** shortly after a
fresh publish — that is lag, not failure; re-run later and the audit goes
all-OK. When something fails mid-release, fix the cause and **re-run the
whole workflow** — never hand-finish a partial release.

## One-time setup checklist (per `release-targets.yml`)

1. Reserve names: PyPI (`kanonak-canonical`/`kanonak-codec`/`kanonak-expression`/`kanonak-wire`),
   crates.io (same), NuGet `Kanonak.*` prefix, npm org `@kanonak-protocol`, Sonatype `org.kanonak`.
2. Configure trusted publishers (PyPI, crates.io, npm, NuGet) → repo + `release.yml` + environment `production`.
3. Create a **`production` GitHub environment** (Settings → Environments; optionally add required reviewers to gate publishing).
4. Create the Maven secrets (`MAVEN_CENTRAL_*`, `MAVEN_GPG_*`). PyPI/crates/npm/NuGet are keyless via OIDC.
5. Go: push subdir tags `kanonak-canonical/go/vX.Y.Z` then `kanonak-codec/go/vX.Y.Z`
   (no infra; canonical first); `kanonak-expression/go/vX.Y.Z` and
   `kanonak-wire/go/vX.Y.Z` any time.
6. GHCR (Wasm): nothing before the first release. **After the first publish**,
   set the org package `ghcr.io/kanonak-protocol/codec` to **Public** (org →
   Packages → codec → package settings) — anonymous `wkg oci pull` is the
   acceptance bar, and `release-audit` checks it anonymously and hard-fails
   until the flip is done.
