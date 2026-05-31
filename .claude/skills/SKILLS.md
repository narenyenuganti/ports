# Vendored Skills Registry

This file is the honest, authoritative record of the external agent skills
the macOS menu-bar app plan chose to vendor, their pinned upstream revisions,
their purpose, and the content-review verdict for each.

## Build-environment note

Network access (DNS) was **unavailable** in the build environment used to
prepare this branch: `git clone` over both HTTPS and SSH failed with
`Could not resolve host: github.com`, and `curl https://github.com` could not
resolve the host either.

Per project policy we **do not fabricate** vendored skill bodies. Where an
upstream repository could not be cloned, the skill is recorded below with
`PINNED SHA: UNAVAILABLE: network clone failed in build env` and **no
skill body is committed**. When network access is available, re-run the
vendoring procedure (see "Re-vendoring procedure" below) to clone each repo
at the recorded path, pin the resolved HEAD SHA here, content-review the
body, and commit it under `.claude/skills/<name>/`.

The portions of these skills that this project *actually relies on* — the
Rust lint rules and the code-review heuristics — are **realized as
first-party artifacts we authored ourselves**, so the project is not blocked
by the missing upstream bodies:

- Rust correctness/style rules → the Cargo `[lints.rust]` / `[lints.clippy]`
  tables in `Cargo.toml` (deny correctness, `await_holding_lock`,
  `unused_must_use`; warn `unwrap_used`/`expect_used`).
- Review heuristics (Rust daemon/protocol + Swift conventions + security
  scrubbing) → documented in `AGENTS.md` and enforced at the two-tier merge
  gate (`scripts/gate-fast.sh`, `scripts/gate-full.sh`).

---

## Intended skills

### Swift-Concurrency-Agent-Skill
- Source URL: https://github.com/AvdLee/Swift-Concurrency-Agent-Skill
- PINNED SHA: UNAVAILABLE: network clone failed in build env
- Purpose: Guides the Swift app toward correct structured concurrency —
  async/await only (no GCD), `@MainActor` isolation for the state model,
  `Sendable` conformances, actor reentrancy hazards. Used as guidance while
  building `Ports.app`.
- License: expected MIT (AvdLee repos are MIT); MUST be confirmed against the
  cloned `LICENSE` before the body is committed.
- Content-review verdict: NOT YET REVIEWED — body not present (no network).
  When vendored, review for shell/network/exfiltration instructions and
  reject if unsafe.

### Swift-Testing-Agent-Skill
- Source URL: https://github.com/AvdLee/Swift-Testing-Agent-Skill
- PINNED SHA: UNAVAILABLE: network clone failed in build env
- Purpose: Guides use of the `Testing` framework (`@Test`, `#expect`,
  `#require`, suites/traits) for the Swift app's tests, including the
  Rust↔Swift protocol drift test.
- License: expected MIT; MUST be confirmed against the cloned `LICENSE`.
- Content-review verdict: NOT YET REVIEWED — body not present (no network).

### Dimillian/Skills — macOS-menubar
- Source URL: https://github.com/Dimillian/Skills (subdir: macOS-menubar)
- PINNED SHA: UNAVAILABLE: network clone failed in build env
- Purpose: Patterns for `MenuBarExtra`-based menu-bar apps (window/scene
  setup, activation policy, lifecycle) directly applicable to `Ports.app`.
- License: MUST be confirmed against the repo `LICENSE` (expected
  MIT/Apache/BSD/ISC; reject otherwise).
- Content-review verdict: NOT YET REVIEWED — body not present (no network).

### Dimillian/Skills — SwiftPM-packaging
- Source URL: https://github.com/Dimillian/Skills (subdir: SwiftPM-packaging)
- PINNED SHA: UNAVAILABLE: network clone failed in build env
- Purpose: Structuring the Swift app as a SwiftPM package
  (`app/Package.swift`), targets/products, building/testing from CLI.
- License: MUST be confirmed against the repo `LICENSE`.
- Content-review verdict: NOT YET REVIEWED — body not present (no network).

### Dimillian/Skills — UI-Patterns
- Source URL: https://github.com/Dimillian/Skills (subdir: UI-Patterns)
- PINNED SHA: UNAVAILABLE: network clone failed in build env
- Purpose: SwiftUI UI patterns for the thin model-view layer of the app.
- License: MUST be confirmed against the repo `LICENSE`.
- Content-review verdict: NOT YET REVIEWED — body not present (no network).

### Dimillian/Skills — View-Refactor
- Source URL: https://github.com/Dimillian/Skills (subdir: View-Refactor)
- PINNED SHA: UNAVAILABLE: network clone failed in build env
- Purpose: Refactoring SwiftUI views toward small, composable, testable
  views (keeps views thin per our Swift conventions).
- License: MUST be confirmed against the repo `LICENSE`.
- Content-review verdict: NOT YET REVIEWED — body not present (no network).

### thermo-nuclear-review
- Source URL: UNRESOLVED — could not search GitHub (no network). Likely a
  thermo/cursor plugin-style review skill; resolve the canonical repo before
  vendoring.
- PINNED SHA: UNAVAILABLE: network clone failed in build env
- Purpose: Deep, explicitly-invoked review pass run at the **merge gate**
  (Tier 2). COMPOSES WITH the superpowers skills — it does not replace them.
- License: MUST be confirmed before vendoring (reject non-permissive).
- Content-review verdict: NOT YET REVIEWED — body not present (no network).

### thermo-nuclear-code-quality-review
- Source URL: UNRESOLVED — could not search GitHub (no network). Resolve the
  canonical repo before vendoring.
- PINNED SHA: UNAVAILABLE: network clone failed in build env
- Purpose: Explicitly-invoked code-quality review pass at the merge gate
  (Tier 2). COMPOSES WITH the superpowers skills.
- License: MUST be confirmed before vendoring (reject non-permissive).
- Content-review verdict: NOT YET REVIEWED — body not present (no network).

---

## Re-vendoring procedure (run when network is available)

For each repo:

1. `git clone --depth 1 <url> /tmp/<name>` and record the resolved SHA:
   `git -C /tmp/<name> rev-parse HEAD`.
2. Confirm the `LICENSE` is MIT/Apache-2.0/BSD/ISC. If not, do **not** vendor.
3. Content-review the skill body for shell/network/exfiltration instructions
   (e.g. `curl … | sh`, base64-encoded payloads, credential reads, network
   POSTs). Reject anything unsafe.
4. For `Dimillian/Skills`, copy only the four chosen subdirs
   (`macOS-menubar`, `SwiftPM-packaging`, `UI-Patterns`, `View-Refactor`).
5. Copy the reviewed body into `.claude/skills/<name>/`, update this file
   with the resolved `PINNED SHA`, the confirmed license, and the review
   verdict, then commit.
