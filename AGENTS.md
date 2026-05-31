# AGENTS.md

Operating guide for agents (and humans) working on the **Ports** menu-bar app.
These conventions are mandatory; the merge gate enforces the mechanical parts.

---

## 1. Project overview & architecture

Ports is being turned from a terminal UI into a native macOS **menu-bar app**
(`Ports.app`). Two components:

- **Rust daemon (`portsd`)** — owns *all* SSH and port-forwarding logic,
  reusing the existing `ssh::` and `forward::` modules. It speaks a
  **newline-delimited JSON** protocol over a **Unix domain socket**.
- **Swift menu-bar app (`Ports.app`)** — a SwiftUI `MenuBarExtra` client.
  It contains **no** SSH logic; it only talks to the daemon over the socket.

The protocol is the contract between the two and lives in `src/protocol/`
(pure types + serde + golden fixtures). Requests carry an `id` + tagged
`type`; the daemon replies with `State` snapshots, `Ack`s (optional error),
and `Event`s.

**Explicitly OUT of scope for v1** (do not add these):
- **No SwiftData.**
- **No AppIntents.**
- **No App Store distribution.**

## 2. Branch naming

Use one of these **seven** prefixes:

`feature/`, `fix/`, `refactor/`, `docs/`, `test/`, `chore/`, `perf/`

## 3. Frequent-commit policy

- Commit **frequently**, in **small logical units** (one concern per commit).
- Commit messages are **emoji-free**.
- Every commit message ends with the trailer line:
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
- Never commit `dbg!`/`println!` debug output or commented-out code.
- Push after meaningful milestones.

## 4. Worktree-per-agent & shared build cache

- Each agent/feature gets its **own git worktree**; never work in another
  agent's worktree and never commit to `main`.
- Share the cargo build cache across worktrees by exporting a shared target
  directory in every shell:
  `export CARGO_TARGET_DIR="$HOME/.cache/ports-target"`
  Prefix cargo commands with it so artifacts are reused across worktrees.

## 5. Two-tier merge gate

- **Tier 1 — fast gate** (`scripts/gate-fast.sh`), run on every commit and
  before every push: `cargo fmt --all --check`, `cargo clippy --all-targets`,
  `cargo test --all`. Must end with `fast gate OK`.
- **Tier 2 — full gate** (`scripts/gate-full.sh`), run at the merge point:
  the fast gate, **plus** the Swift build/test stage (guarded by
  `if [ -f app/Package.swift ]`), **plus** the explicitly-invoked
  thermo-nuclear review skills (see §9). Tier 2 is required before merge.

## 6. Rust daemon & protocol conventions

- **Channels:** `watch` for state, `oneshot` for replies, **bounded `mpsc`**
  for work queues. **`tokio::sync::mpsc::unbounded_channel` is banned** — an
  unbounded queue is an unbounded memory leak under backpressure.
- **Cancellation:** every spawned task takes a `CancellationToken`; on
  shutdown, **cancel and then await** the task (cancel-and-await), never
  detach-and-forget.
- **No lock across await:** `clippy::await_holding_lock` is **denied**; drop
  guards before `.await`.
- **Errors:** `thiserror` for the **serde-serializable wire error**
  (`ProtocolError`) sent to the client; `anyhow` for **internal** error
  plumbing. Convert internal → wire at the boundary.
- **Newtype IDs:** wrap raw integers/ports in newtypes (`ForwardId`, `Port`)
  rather than passing bare `u64`/`u16` around.
- **Parse, don't validate:** at the socket boundary, parse untrusted bytes
  into well-typed protocol values once; downstream code receives only valid
  types. Reject malformed input with `BadRequest`.
- **No `unwrap`/`expect` outside tests:** `clippy::unwrap_used` and
  `clippy::expect_used` warn crate-wide; in non-test code, handle the error.
- **Docs:** doc-comment **all public protocol types** and the **`Engine`
  trait** (the daemon's core abstraction).
- **Logging:** use the **`log`** crate facade, **not `tracing`**.

## 7. Swift conventions

- **Concurrency:** **async/await only — no GCD** (no `DispatchQueue`,
  `dispatch_async`, etc.).
- **State:** the state model is a single **`@MainActor`** observable type;
  UI reads from it on the main actor.
- **Protocol mirror:** the Swift side has a **`Sendable` + `Codable`** mirror
  of the Rust protocol types, kept **in lockstep** with Rust via a **drift
  test** that decodes the committed Rust golden fixtures.
- **Views:** keep views **thin (model-view)** — logic lives in the state
  model, not in view bodies; refactor large views into small composable ones.

## 8. Security

- **Never log, serialize, or persist** SSH keys, passphrases, or any
  credentials.
- **Redact in `Debug`:** types that carry secrets implement `Debug` to print
  a redacted placeholder, never the secret.
- **Scrub errors to the client:** wire errors (`ProtocolError`) must not leak
  key material, file contents, or internal paths beyond what the user needs;
  sanitize details before they cross the socket.

## 9. Skills

- The vendored skills live under **`.claude/skills/`** (registry:
  `.claude/skills/SKILLS.md`).
- **superpowers** skills are the **default** workflow for everyday work
  (planning, TDD, debugging, review, git worktrees, etc.).
- The **thermo-nuclear** review skills (`thermo-nuclear-review`,
  `thermo-nuclear-code-quality-review`) are **invoked explicitly at the merge
  gate (Tier 2)** and **COMPOSE WITH — do not replace —** the superpowers
  skills.
