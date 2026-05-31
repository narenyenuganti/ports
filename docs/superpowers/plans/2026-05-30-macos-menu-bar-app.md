# Ports.app — macOS Menu Bar App Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a native macOS menu bar app that shows a remote dev host's listening ports and forwards any to localhost in one click — driven by a new Rust daemon over a Unix socket — without touching the existing core or TUI.

**Architecture:** Three layers. The existing Rust core (`ssh`, `forward`) is reused unchanged. A new `ports daemon` subcommand (tokio actor + Unix-socket server) owns the connection and forwards. A SwiftUI `MenuBarExtra` thin client spawns/supervises the daemon and renders the `State` it pushes over newline-delimited JSON.

**Tech Stack:** Rust (tokio, russh, serde, thiserror, tokio-util), Swift 6 (SwiftUI `MenuBarExtra`, Swift Testing, SMAppService), Unix domain socket + NDJSON protocol, SwiftPM + Makefile bundling.

**Spec:** `docs/superpowers/specs/2026-05-30-macos-menu-bar-app-design.md`

---

## How this plan was shaped (skill research)

A survey workflow (synthesis + adversarial critique, verdict **adopt-with-fixes**) decided the quality tooling. The decisions are baked into the tasks below:

**Adopted skills** (vendored in Phase 0, Task 0.4 — copies pinned to upstream commit SHAs, content-reviewed for shell/network/injection before commit):
- **Rust:** `rust-skills` rules realized as a `[lints]` table + `rustfmt.toml` + review heuristics (we do NOT vendor all 179 rules — ~16 high-signal ones become enforcement).
- **Swift:** AvdLee `Swift-Concurrency-Agent-Skill`, AvdLee `Swift-Testing-Agent-Skill`, and the macOS-relevant subdirs of `Dimillian/Skills` (macOS Menubar, SwiftPM Packaging, SwiftUI UI-Patterns, View-Refactor).
- **Review gate:** `thermo-nuclear-review` (blocking) and `thermo-nuclear-code-quality-review` (advisory, with two amendments).

**Critique fixes folded in:** pin the toolchain so clippy/rustfmt exist in every worktree (Task 0.1); choose per-lint levels so the existing 15 `.unwrap()`s don't break the build (Task 0.3); make the protocol drift test a concrete golden-fixture mechanism (Tasks 1.5 + 3.2); enforce Swift 6 strict concurrency at compile time (Task 3.1); add a signing + login-item smoke test (Task 5.2); two-tier gate (fast pre-push vs full merge) sharing one `CARGO_TARGET_DIR` (Task 0.6); one blocking LLM review pass, not four (AGENTS.md, Task 0.5); review-and-SHA-pin vendored skills (Task 0.4).

---

## Working conventions (apply to EVERY task)

- **Branches:** each phase runs in its own git worktree (superpowers:using-git-worktrees) on a branch prefixed per the convention — `feature/quality-foundation`, `feature/protocol`, `feature/daemon`, `feature/swift-app`, `feature/app-ui`, `feature/packaging`. Never commit to `main` directly. No agent-name tags in branch names.
- **Commits:** commit frequently in small logical units (the TDD step boundaries below are commit points) and push to origin regularly.
- **Merge to main:** only after the **full merge gate** (`scripts/gate-full.sh`, built in Task 0.6) passes. Use `verification-before-completion` to prove it ran green before merging.
- **Worktrees share** one `CARGO_TARGET_DIR` (set in Task 0.6) so cold worktrees reuse build artifacts.

---

## File Structure

**Created:**
- `rust-toolchain.toml` — pin channel + clippy/rustfmt components (every worktree identical).
- `rustfmt.toml` — shared formatting.
- `AGENTS.md` — conventions + two-tier quality gate + Rust/Swift design rules.
- `.claude/skills/<vendored>/…` — pinned, reviewed third-party skills.
- `.claude/skills/SKILLS.md` — provenance (source URL + commit SHA + review note) per vendored skill.
- `scripts/gate-fast.sh`, `scripts/gate-full.sh` — the two gate tiers.
- `scripts/smoke-app.sh` — codesign + login-item + socket smoke test (Phase 5).
- `src/protocol/mod.rs`, `src/protocol/ids.rs`, `src/protocol/error.rs`, `src/protocol/message.rs` — wire protocol (serde + thiserror), the Rust↔Swift contract.
- `src/daemon/mod.rs`, `src/daemon/engine.rs`, `src/daemon/actor.rs`, `src/daemon/server.rs`, `src/daemon/supervise.rs` — the daemon.
- `tests/protocol_fixtures/*.json` — committed golden fixtures (drift test, both languages decode these).
- `app/Package.swift`, `app/Sources/PortsBar/{PortsBarApp,DaemonClient,Protocol,AppModel,PopoverView,SettingsView}.swift`, `app/Tests/PortsBarTests/*.swift` — the SwiftUI app.
- `Makefile` — build Rust (universal) + Swift, assemble `Ports.app`, ad-hoc sign.

**Modified:**
- `Cargo.toml` — add `[lints]` tables, deps (`serde`, `serde_json`, `thiserror`), no profile changes (NO `panic="abort"`).
- `src/main.rs` — add the `daemon` subcommand (clap).
- `src/ssh/config.rs` — add `list_host_aliases()`.
- `src/lib.rs` — `pub mod protocol; pub mod daemon;`.

**Untouched:** `src/ssh/{connection,discovery}.rs`, `src/forward/*`, `src/tui/*`.

---

## Phase 0 — Quality foundation
**Branch:** `feature/quality-foundation`. Lands the gate against the *existing* crate. Swift/daemon gate steps are written now but guarded so they no-op until those milestones land.

### Task 0.1: Pin the toolchain so clippy + rustfmt exist everywhere

**Files:** Create `rust-toolchain.toml`

- [ ] **Step 1: Verify the gap** — Run: `cargo clippy --version; cargo fmt --version`. Expected today: at least one prints `'cargo-clippy' is not installed` / `'cargo-fmt' is not installed`. This is why the gate can't run yet.

- [ ] **Step 2: Create `rust-toolchain.toml`**

```toml
[toolchain]
channel = "stable"
components = ["clippy", "rustfmt"]
profile = "minimal"
```

- [ ] **Step 3: Materialize components** — Run: `rustup component add clippy rustfmt && cargo clippy --version && cargo fmt --version`. Expected: both print a version, no "not installed".

- [ ] **Step 4: Commit**

```bash
git add rust-toolchain.toml
git commit -m "build: pin toolchain with clippy and rustfmt components"
```

### Task 0.2: Shared formatting

**Files:** Create `rustfmt.toml`

- [ ] **Step 1: Create `rustfmt.toml`** (conservative, stable-only options)

```toml
edition = "2021"
max_width = 100
use_field_init_shorthand = true
```

- [ ] **Step 2: Format the existing tree** — Run: `cargo fmt --all`. Then `git diff --stat` to see what reformatted.

- [ ] **Step 3: Verify clean** — Run: `cargo fmt --all --check`. Expected: exit 0, no output.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "style: add rustfmt.toml and format existing tree"
```

### Task 0.3: Lints table (chosen so existing code still builds)

**Files:** Modify `Cargo.toml`

Rationale: deny only true-error lints; keep `unwrap_used`/`expect_used` at **warn** (there are 15 existing `.unwrap()`s in `src/`). The gate runs `cargo clippy --all-targets` **without** a blanket `-D warnings`, so `deny`-level lints fail the build while `warn`-level ones stay visible-but-non-blocking. New daemon code will opt into stricter denies at module level (Task 2.x).

- [ ] **Step 1: Append to `Cargo.toml`**

```toml
[lints.rust]
unused_must_use = "deny"
let_underscore_drop = "warn"

[lints.clippy]
correctness = { level = "deny", priority = -1 }
suspicious = { level = "warn", priority = -1 }
complexity = { level = "warn", priority = -1 }
perf = { level = "warn", priority = -1 }
style = { level = "warn", priority = -1 }
await_holding_lock = "deny"
unwrap_used = "warn"
expect_used = "warn"
```

- [ ] **Step 2: Verify the existing crate still builds and lints** — Run: `cargo clippy --all-targets`. Expected: exit 0; may print `unwrap_used` warnings on the existing 15 sites — that's intended, not a failure.

- [ ] **Step 3: Verify tests still pass** — Run: `cargo test`. Expected: PASS (existing suite green).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml
git commit -m "build: add clippy/rustc lint levels for the quality gate"
```

### Task 0.4: Vendor the chosen skills (reviewed + SHA-pinned)

**Files:** Create `.claude/skills/<skill>/…`, `.claude/skills/SKILLS.md`

- [ ] **Step 1: Clone each upstream at its current HEAD into a temp dir and record the SHA** for: `AvdLee/Swift-Concurrency-Agent-Skill`, `AvdLee/Swift-Testing-Agent-Skill`, `Dimillian/Skills`, and the two `cursor/plugins` thermos skills (`thermos/skills/thermo-nuclear-review`, `thermos/skills/thermo-nuclear-code-quality-review`). Run e.g. `git -C <clone> rev-parse HEAD`.

- [ ] **Step 2: CONTENT review each skill body** (not just LICENSE) for instructions that run shell/network/file-exfiltration. Reject any that do. Confirm each LICENSE is MIT/Apache-2/BSD/ISC.

- [ ] **Step 3: Copy flat into our namespace** — only the relevant Dimillian subdirs (macOS-menubar, SwiftPM-packaging, UI-Patterns, View-Refactor); the two AvdLee skills whole; the two thermos skills whole. Do not submodule.

- [ ] **Step 4: Write `.claude/skills/SKILLS.md`** recording, per skill: source URL, pinned commit SHA, what we use it for, the review verdict, and the note "third-party skills are reviewed like dependencies; update only via a reviewed change."

- [ ] **Step 5: Commit**

```bash
git add .claude/skills
git commit -m "doc: vendor reviewed, SHA-pinned Swift + review skills"
```

### Task 0.5: Write `AGENTS.md`

**Files:** Create `AGENTS.md`

- [ ] **Step 1: Write `AGENTS.md`** with these sections (content per the spec + research synthesis):
  1. **Project overview & architecture** — the three layers; explicit no-SwiftData/App-Intents/App-Store scope so agents don't over-build.
  2. **Branch-naming convention** — the seven prefixes; never commit to main; a `feature/` branch must not merge half-wired surface.
  3. **Frequent-commit & push policy** — small logical commits, push regularly, emoji-free messages, no committed `dbg!`/`println!`/commented-out code, co-author trailer.
  4. **Worktree-per-agent policy** — isolate parallel agents; shared `CARGO_TARGET_DIR`.
  5. **Merge quality gate** — the two tiers and the ordered stages (reproduce Task 0.6's pipeline); warnings enforced at the gate, NEVER via `#![deny(warnings)]` in source.
  6. **Rust daemon & protocol conventions** — `watch` for State push, `oneshot` for actor replies, **bounded** `mpsc` for queues (ban `unbounded_channel`); `CancellationToken` on every spawn + cancel-and-await on shutdown; `thiserror` serde enum for the wire error, `anyhow` + `.with_context()` internally (do not rewrite existing anyhow code); newtype IDs + serde-tagged enum states; parse NDJSON into typed commands once at the socket reader; no `unwrap/expect` outside tests; config struct over >5 params; doc-comment public protocol types + the `Engine` trait; log via the `log` crate (not tracing).
  7. **Swift client conventions** — async/await only (no GCD / `Task.sleep(nanoseconds:)`); `@MainActor`-isolate the state model; keep the Codable mirror Sendable-clean and in lockstep with the Rust types (drift test); thin MV-style views.
  8. **Security** — never log/serialize/persist SSH keys, passphrases, or credentials; redact in Debug; scrub errors sent to the client; defer to `/security-review` on credential diffs.
  9. **Skills available in this repo** — point at `.claude/skills`; note the thermo skills are invoke-explicitly (gate only) and compose with — do not replace — the superpowers.

- [ ] **Step 2: Commit**

```bash
git add AGENTS.md
git commit -m "doc: add AGENTS.md with conventions and merge quality gate"
```

### Task 0.6: Gate scripts (two tiers, shared target dir)

**Files:** Create `scripts/gate-fast.sh`, `scripts/gate-full.sh`

- [ ] **Step 1: Write `scripts/gate-fast.sh`** (runs on every commit/push; fast subset; fails fast)

```bash
#!/usr/bin/env bash
set -euo pipefail
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/ports-target}"
echo "== fmt ==";    cargo fmt --all --check
echo "== clippy =="; cargo clippy --all-targets   # deny-level lints fail; warns don't
echo "== test (lib/unit) =="; cargo test --lib
echo "fast gate OK"
```

- [ ] **Step 2: Write `scripts/gate-full.sh`** (runs before merge to main; staged; Swift stages guarded until the app exists)

```bash
#!/usr/bin/env bash
set -euo pipefail
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/ports-target}"

echo "== Stage 0: deterministic Rust =="
cargo fmt --all --check
cargo clippy --all-targets
cargo test --all

if [ -f app/Package.swift ]; then
  echo "== Stage 1: deterministic Swift =="
  ( cd app && swift build -Xswiftc -warnings-as-errors && swift test )
  command -v swiftformat >/dev/null && ( cd app && swiftformat --lint . )
  command -v swiftlint   >/dev/null && ( cd app && swiftlint --strict )
else
  echo "== Stage 1: skipped (no app/ yet) =="
fi

echo "== Stage 2: thermo-nuclear-review (blocking) =="
echo "  -> invoke the thermo-nuclear-review skill on 'git diff main...HEAD'."
echo "     Hand it the focus checklist from AGENTS.md (protocol mirror parity,"
echo "     tokio cancel/channel lifecycle, socket/spawn/login-item devex,"
echo "     feature-leak, Engine+Mock parity, secrets). High/Medium = blockers."
echo "== Stage 3: thermo-nuclear-code-quality-review (ADVISORY) =="
echo "  -> optional structural pass; file-size rule = don't push a file ACROSS 1000 lines;"
echo "     out-of-scope restructurings become follow-up refactor/ tickets."
echo "full gate (deterministic stages) OK — now run the two review skills, then verification-before-completion."
```

Note: Stages 2–3 are LLM review skills, not shell commands; the script prints the instruction so the operator (or the orchestrating agent) invokes them explicitly. They run **only** at merge, never per-commit.

- [ ] **Step 3: Make executable + smoke them** — Run: `chmod +x scripts/gate-*.sh && ./scripts/gate-fast.sh`. Expected: prints `fast gate OK`.

- [ ] **Step 4: Commit**

```bash
git add scripts/gate-fast.sh scripts/gate-full.sh
git commit -m "build: add two-tier quality gate scripts"
```

- [ ] **Step 5: Merge Phase 0 to main via the gate** — Run `./scripts/gate-full.sh` (Stage 0 only, Swift skipped), invoke the two review skills on the diff, confirm green with verification-before-completion, then merge `feature/quality-foundation` → `main` and push.

---

## Phase 1 — Wire protocol (the Rust↔Swift contract)
**Branch:** `feature/protocol`. Pure types + serde + golden fixtures; no I/O. This is the highest-leverage correctness surface.

### Task 1.1: Add protocol deps + module wiring

**Files:** Modify `Cargo.toml`, `src/lib.rs`; Create `src/protocol/mod.rs`

- [ ] **Step 1:** add to `Cargo.toml` `[dependencies]`: `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `thiserror = "1"`.
- [ ] **Step 2:** `src/lib.rs` add `pub mod protocol;`.
- [ ] **Step 3:** create `src/protocol/mod.rs` with `pub mod ids; pub mod error; pub mod message;` and `#![deny(clippy::unwrap_used, clippy::expect_used)]` at module top (stricter than crate default).
- [ ] **Step 4:** Run `cargo build`. Expected: compiles (empty submodules created in next tasks — create stub files so it builds, or sequence 1.2 first). Commit: `feat: scaffold protocol module`.

### Task 1.2: Newtype IDs

**Files:** Create `src/protocol/ids.rs`; Test: inline `#[cfg(test)]`

- [ ] **Step 1: Failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forward_id_serializes_transparently() {
        let id = ForwardId(7);
        assert_eq!(serde_json::to_string(&id).unwrap(), "7");
        assert_eq!(serde_json::from_str::<ForwardId>("7").unwrap(), id);
    }
}
```

- [ ] **Step 2:** Run `cargo test -p ports protocol::ids` → FAIL (ForwardId undefined).
- [ ] **Step 3: Implement**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ForwardId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Port(pub u16);
```

- [ ] **Step 4:** Run the test → PASS. **Step 5:** Commit `feat: protocol newtype ids`.

### Task 1.3: Wire-protocol error (thiserror + serde)

**Files:** Create `src/protocol/error.rs`

- [ ] **Step 1: Failing test** — assert a tagged-enum round-trip:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_error_roundtrips_tagged() {
        let e = ProtocolError::BindFailed { port: 3000, detail: "address in use".into() };
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"kind\":\"bind_failed\""));
        assert_eq!(serde_json::from_str::<ProtocolError>(&json).unwrap(), e);
    }
}
```

- [ ] **Step 2:** Run → FAIL. **Step 3: Implement**

```rust
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProtocolError {
    #[error("not connected to a host")]
    NotConnected,
    #[error("connect failed: {detail}")]
    ConnectFailed { detail: String },
    #[error("failed to bind local port {port}: {detail}")]
    BindFailed { port: u16, detail: String },
    #[error("unknown host alias: {alias}")]
    UnknownHost { alias: String },
    #[error("file transfer failed: {detail}")]
    SendFileFailed { detail: String },
    #[error("malformed request: {detail}")]
    BadRequest { detail: String },
}
```

- [ ] **Step 4:** Run → PASS. **Step 5:** Commit `feat: serde-serializable wire-protocol error`.

### Task 1.4: Request / message enums

**Files:** Create `src/protocol/message.rs`

Defines the full contract. `Request` (app→daemon, with `id`), and `DaemonMessage` (daemon→app): `State`, `Ack`, `Event`. Forward/connection lifecycle modeled as enums (no bool soup).

- [ ] **Step 1: Failing tests** — one round-trip per top-level type; assert tagged shape, e.g.:

```rust
#[test]
fn request_start_forward_roundtrips() {
    let r = Request { id: 42, body: RequestBody::StartForward { remote_port: Port(3000), local_port: None } };
    let v = serde_json::to_string(&r).unwrap();
    assert!(v.contains("\"type\":\"start_forward\""));
    assert_eq!(serde_json::from_str::<Request>(&v).unwrap(), r);
}

#[test]
fn state_snapshot_roundtrips() {
    let s = DaemonMessage::State(StateSnapshot {
        host: Some("dev-desktop".into()),
        status: ConnStatus::Connected,
        status_detail: None,
        ports: vec![PortEntry {
            remote_port: Port(3000), process: Some("next".into()), pid: Some(1234),
            forward: ForwardState::Forwarding { local_port: Port(3000) },
        }],
    });
    let v = serde_json::to_string(&s).unwrap();
    assert_eq!(serde_json::from_str::<DaemonMessage>(&v).unwrap(), s);
}
```

- [ ] **Step 2:** Run → FAIL. **Step 3: Implement**

```rust
use serde::{Deserialize, Serialize};
use crate::protocol::error::ProtocolError;
use crate::protocol::ids::Port;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request { pub id: u64, #[serde(flatten)] pub body: RequestBody }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RequestBody {
    SetConfig { host_alias: String, refresh_secs: u32, auto_reconnect: bool },
    Connect,
    Disconnect,
    Refresh,
    StartForward { remote_port: Port, local_port: Option<Port> },
    StopForward { remote_port: Port },
    SendFile { local_path: String, remote_path: Option<String> },
    ListHosts,
    Ping,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonMessage {
    State(StateSnapshot),
    Ack { id: u64, #[serde(skip_serializing_if = "Option::is_none")] error: Option<ProtocolError>,
          #[serde(default, skip_serializing_if = "Option::is_none")] hosts: Option<Vec<String>> },
    Event(DaemonEvent),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub host: Option<String>,
    pub status: ConnStatus,
    #[serde(skip_serializing_if = "Option::is_none")] pub status_detail: Option<String>,
    pub ports: Vec<PortEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnStatus { Disconnected, Connecting, Connected, Error }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortEntry {
    pub remote_port: Port,
    #[serde(skip_serializing_if = "Option::is_none")] pub process: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub pid: Option<u32>,
    pub forward: ForwardState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ForwardState {
    Idle,
    Forwarding { local_port: Port },
    Error { detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum DaemonEvent {
    FileTransfer { ok: bool, detail: String },
}
```

- [ ] **Step 4:** Run all `protocol::message` tests → PASS. **Step 5:** Commit `feat: wire protocol request/message types`.

### Task 1.5: Golden fixtures (Rust side of the drift test)

**Files:** Create `tests/protocol_fixtures.rs`, `tests/protocol_fixtures/*.json`

Mechanism (per critique fix #7): a test serializes one canonical value of **every** top-level protocol type to a committed `.json` golden. A Rust-side rename then fails this test (golden mismatch); the Swift test (Task 3.2) decodes the same committed files, so the two languages cannot silently diverge.

- [ ] **Step 1: Write the fixture test**

```rust
// tests/protocol_fixtures.rs
use ports::protocol::message::*;
use ports::protocol::ids::Port;
use std::fs;

fn check(name: &str, value: impl serde::Serialize) {
    let pretty = serde_json::to_string_pretty(&value).unwrap();
    let path = format!("tests/protocol_fixtures/{name}.json");
    if std::env::var("REGEN_FIXTURES").is_ok() {
        fs::write(&path, format!("{pretty}\n")).unwrap();
    }
    let golden = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing fixture {path}; run REGEN_FIXTURES=1 cargo test"));
    assert_eq!(pretty.trim(), golden.trim(), "fixture drift in {name}");
}

#[test]
fn fixtures_match() {
    check("request_start_forward", Request { id: 1, body: RequestBody::StartForward { remote_port: Port(3000), local_port: None } });
    check("state_connected", DaemonMessage::State(StateSnapshot {
        host: Some("dev-desktop".into()), status: ConnStatus::Connected, status_detail: None,
        ports: vec![PortEntry { remote_port: Port(3000), process: Some("next".into()), pid: Some(1234), forward: ForwardState::Forwarding { local_port: Port(3000) } }],
    }));
    check("ack_list_hosts", DaemonMessage::Ack { id: 2, error: None, hosts: Some(vec!["dev-desktop".into(), "staging".into()]) });
    check("event_file_transfer", DaemonMessage::Event(DaemonEvent::FileTransfer { ok: true, detail: "sent".into() }));
    // ...one check() per RequestBody/DaemonMessage/ForwardState variant
}
```

- [ ] **Step 2: Generate goldens** — Run: `REGEN_FIXTURES=1 cargo test --test protocol_fixtures`. Inspect the produced JSON files.
- [ ] **Step 3: Verify** — Run: `cargo test --test protocol_fixtures`. Expected: PASS.
- [ ] **Step 4: Commit** `test: golden JSON fixtures for protocol drift detection` (commit the `.json` files).
- [ ] **Step 5: Merge `feature/protocol` → main via the full gate.**

---

## Phase 2 — Daemon (actor + engine + socket + subcommand)
**Branch:** `feature/daemon`. Each daemon source file starts with `#![deny(clippy::unwrap_used, clippy::expect_used)]`.

### Task 2.1: `Engine` trait + `MockEngine`
**Files:** Create `src/daemon/mod.rs` (`pub mod engine; …`), `src/daemon/engine.rs`; `src/lib.rs` += `pub mod daemon;`.

- [ ] **Step 1: Failing test** for `MockEngine` returning scripted ports.
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3: Implement** the trait + mock:

```rust
use anyhow::Result;
use async_trait::async_trait;
use crate::ssh::discovery::DiscoveredPort;
use crate::ssh::config::HostConfig;

#[async_trait]
pub trait Engine: Send {
    async fn connect(&mut self, cfg: &HostConfig) -> Result<()>;
    async fn discover(&self) -> Result<Vec<DiscoveredPort>>;
    async fn start_forward(&mut self, remote: u16, local: Option<u16>) -> Result<u16>;
    fn stop_forward(&mut self, remote: u16);
    fn stop_all(&mut self);
    async fn send_file(&self, local: &str, remote: &str) -> Result<()>;
}
```

`MockEngine` holds `Vec<DiscoveredPort>` + a started-forwards map + injectable failures.

- [ ] **Step 4:** Run → PASS. **Step 5:** Commit `feat: daemon Engine trait + MockEngine`.

### Task 2.2: `SshEngine` (real impl, reuses core unchanged)
**Files:** Create `src/daemon/engine.rs` (append `SshEngine`)

- [ ] **Steps:** Implement `SshEngine` wrapping `SshSession` + `ForwardManager` + `send_file`; `connect` uses `load_host_config` + `SshSession::connect`; `discover` calls `discover_remote_ports`; `start_forward`/`stop_forward`/`stop_all` delegate to `ForwardManager`. Unit-test what's testable without a host (construction, stop on empty). Commit `feat: SshEngine wrapping the existing core`.

### Task 2.3: `list_host_aliases()`
**Files:** Modify `src/ssh/config.rs`

- [ ] **Step 1: Failing test** — parse a config string with `Host a`, `Host b *.x`, `Host *` → returns `["a","b"]` (skip pure-wildcard, expand multi-token Host lines minus globs).
- [ ] **Step 2:** FAIL. **Step 3:** Implement `pub fn parse_host_aliases(config_str: &str) -> Vec<String>` + `pub fn list_host_aliases() -> Result<Vec<String>>` reading `~/.ssh/config`. **Step 4:** PASS. **Step 5:** Commit `feat: enumerate ssh config host aliases`.

### Task 2.4: Actor (state reducer)
**Files:** Create `src/daemon/actor.rs`

The actor owns `StateSnapshot` + config + `Box<dyn Engine>`. It receives `ActorMsg` (a `RequestBody` + a `oneshot` reply sender) on a **bounded** `mpsc`, plus refresh-timer ticks; it publishes `StateSnapshot` on a `watch` channel. Channel choices per AGENTS.md (watch=state, oneshot=reply, bounded mpsc=queue).

- [ ] **Step 1: Failing tests** driven by `MockEngine` on `#[tokio::test(flavor = "current_thread", start_paused = true)]`:
  - `connect` → watch holds `Connected` + the mock's ports.
  - `start_forward` → matching `PortEntry.forward == Forwarding { local_port }`.
  - `discover` failure with `auto_reconnect` → reconnect path, forwards reset to `Idle`.
  - `Shutdown` → `stop_all` called, actor task ends.
- [ ] **Step 2:** FAIL. **Step 3:** Implement the actor loop (`tokio::select!` over the mpsc receiver, refresh `interval`, and a `CancellationToken`). Every spawned task gets a child token; shutdown cancels and awaits. **Step 4:** PASS. **Step 5:** Commit `feat: daemon actor state reducer`.

### Task 2.5: Unix-socket server (framing + fan-out)
**Files:** Create `src/daemon/server.rs`

- [ ] **Steps (TDD):** `UnixListener` at the socket path; per-connection: a reader task parses NDJSON → `Request` (parse-don't-validate at the boundary; malformed line → `Ack{error: BadRequest}`), forwards `RequestBody` to the actor with a `oneshot`, writes the `Ack`; a writer task subscribes to the `watch` and pushes `State` lines on change. Test with an in-process `MockEngine` actor + a client socket: send `Connect`, assert a `State` line arrives; send a malformed line, assert `BadRequest` ack. Commit `feat: daemon unix-socket server with NDJSON framing`.

### Task 2.6: Supervision + `daemon` subcommand
**Files:** Create `src/daemon/supervise.rs`; Modify `src/main.rs`

- [ ] **Steps:** socket path `~/Library/Application Support/<bundle-id>/daemon.sock` (mode 0600); single-instance (refuse if a live daemon answers `Ping`, else clean stale socket); `Shutdown` stops forwards + exits. Add clap `Daemon { socket: Option<PathBuf> }` subcommand calling `daemon::run(...)`. Existing TUI/`send-file` subcommands unchanged. Manual check: `cargo run -- daemon` creates the socket; `nc -U <sock>` + a `{"id":1,"type":"ping"}` line gets an `Ack`. Commit `feat: ports daemon subcommand + supervision`.

- [ ] **Final:** Merge `feature/daemon` → main via the full gate (Stage 0 + review skills; Swift still skipped).

---

## Phase 3 — Swift app skeleton (strict concurrency + drift test)
**Branch:** `feature/swift-app`.

### Task 3.1: SwiftPM package with Swift 6 strict concurrency
**Files:** Create `app/Package.swift`

- [ ] **Step 1: Write `Package.swift`** — executable target `PortsBar` + test target `PortsBarTests`, **Swift 6 language mode** so the compiler enforces the data-race rules the AvdLee skill describes:

```swift
// swift-tools-version: 6.0
import PackageDescription
let package = Package(
    name: "PortsBar",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(name: "PortsBar", swiftSettings: [.swiftLanguageMode(.v6)]),
        .testTarget(name: "PortsBarTests", dependencies: ["PortsBar"], swiftSettings: [.swiftLanguageMode(.v6)]),
    ]
)
```

- [ ] **Step 2:** Add a trivial `PortsBarApp.swift` stub so it builds. Run `cd app && swift build -Xswiftc -warnings-as-errors`. Expected: PASS. **Step 3:** Commit `feat: SwiftPM package with Swift 6 strict concurrency`.

### Task 3.2: Codable mirror + drift test (Swift side)
**Files:** Create `app/Sources/PortsBar/Protocol.swift`, `app/Tests/PortsBarTests/ProtocolDriftTests.swift`

- [ ] **Step 1: Failing test** — decode each committed Rust golden fixture, re-encode, and assert structural round-trip. Fixtures are read from `../../tests/protocol_fixtures` (or copied into the test bundle resources during Task 5.x):

```swift
import Testing
@testable import PortsBar

@Test func decodesRustStateFixture() throws {
    let data = try fixture("state_connected")
    let msg = try JSONDecoder().decode(DaemonMessage.self, from: data)
    guard case .state(let s) = msg else { Issue.record("expected state"); return }
    #expect(s.host == "dev-desktop")
    #expect(s.ports.first?.forward == .forwarding(localPort: 3000))
}
```

- [ ] **Step 2:** FAIL (types undefined). **Step 3: Implement** `Protocol.swift` — `Sendable` Codable enums/structs mirroring the Rust types 1:1, matching the serde tags (`type`/`kind`/`state`, snake_case). **Step 4:** PASS for every fixture. **Step 5:** Commit `feat: Swift Codable protocol mirror + drift test`.

### Task 3.3: DaemonClient (spawn/supervise + async socket loop)
**Files:** Create `app/Sources/PortsBar/DaemonClient.swift`

- [ ] **Steps:** an actor/`@MainActor`-aware client that locates the bundled `ports` binary (`Bundle.main`), spawns `ports daemon`, connects the Unix socket, runs an `async` read loop yielding decoded `DaemonMessage`s (cancellable on teardown), and encodes `Request`s. Use async/await only (no GCD). Test the encode side + line-framing against fixtures. Commit `feat: daemon client (spawn + async socket loop)`.

### Task 3.4: AppModel (`@MainActor` state)
**Files:** Create `app/Sources/PortsBar/AppModel.swift`

- [ ] **Steps:** `@MainActor final class AppModel: ObservableObject` holding `@Published var state: StateSnapshot`; applies decoded daemon `State` on the main actor; exposes intents (`forward`, `stop`, `setLocalPort`, `sendFile`, `refresh`, `setHost`). Unit-test the snapshot→published mapping + active-forward badge count. Commit `feat: @MainActor app model`.

### Task 3.5: MenuBarExtra entrypoint
**Files:** Modify `app/Sources/PortsBar/PortsBarApp.swift`

- [ ] **Steps:** `@main struct PortsBarApp: App` with `MenuBarExtra { PopoverView() }.menuBarExtraStyle(.window)`, icon `arrow.left.arrow.right` + active-forward badge, dimmed when not connected; `LSUIElement` set in the bundle (Phase 5). Commit `feat: MenuBarExtra entrypoint`. Merge `feature/swift-app` → main via the full gate (Swift stages now active).

---

## Phase 4 — Popover + Settings + actions
**Branch:** `feature/app-ui`.

### Task 4.1: PopoverView (tiles)
**Files:** Create `app/Sources/PortsBar/PopoverView.swift` — header (status dot + host + gear), one tile per `PortEntry` (process, `remote :p → localhost:p`, status pill), footer (Refresh · Send file… · Settings · Quit). Snapshot/state-mapping covered by AppModel tests. Commit `feat: popover view`.

### Task 4.2: Per-port actions
- [ ] forward/stop toggle → `Request.StartForward/StopForward`; **open in browser** (`NSWorkspace.open(http://localhost:<local>)` — app-side, no daemon round-trip); **copy URL** (`NSPasteboard`); **custom local port** field → `StartForward{local_port}`, UI shows the actual bound port from the next `State`. Commit `feat: per-port forward/open/copy/custom-port actions`.

### Task 4.3: SettingsView + prefs
**Files:** Create `app/Sources/PortsBar/SettingsView.swift` — host picker (populated via `ListHosts`), launch-at-login toggle, auto-refresh interval, open-browser-on-forward, auto-reconnect; persist to `UserDefaults`; on change push `SetConfig`/`Connect`. Commit `feat: settings + preferences`.

### Task 4.4: File transfer
- [ ] footer "Send file…" → `NSOpenPanel` → prompt remote dir (default `/tmp`) → `Request.SendFile` → toast from the `FileTransfer` event. Commit `feat: send-file from the app`.

### Task 4.5: Launch at login
- [ ] `SMAppService.mainApp.register()/unregister()` wired to the Settings toggle, reflecting current status. Commit `feat: launch-at-login via SMAppService`. Merge `feature/app-ui` → main via the full gate.

---

## Phase 5 — Packaging + signing smoke test
**Branch:** `feature/packaging`.

### Task 5.1: Makefile (universal binary + .app bundle + ad-hoc sign)
**Files:** Create `Makefile`

- [ ] **Steps:** target `app`: build `ports` for `aarch64-apple-darwin` + `x86_64-apple-darwin`, `lipo` into a universal binary; `swift build -c release` the `PortsBar` executable; assemble `build/Ports.app` (`Contents/MacOS/PortsBar`, `Contents/Resources/ports`, generated `Info.plist` with `LSUIElement=true`, bundle id, version); copy the protocol fixtures into test resources if needed; `codesign --force --deep -s - build/Ports.app`. Run `make app`; expect `build/Ports.app`. Commit `build: Makefile to assemble ad-hoc-signed Ports.app`.

### Task 5.2: Signing + login-item + socket smoke test
**Files:** Create `scripts/smoke-app.sh`

- [ ] **Steps (advisory gate step, runs after `make app`):** `codesign --verify --deep --strict build/Ports.app`; launch the app headless, assert the daemon socket appears and answers `Ping`; register then unregister the SMAppService login item and assert no error; print PASS/FAIL. Wire as an advisory line in `gate-full.sh` guarded by `[ -d build/Ports.app ]`. Commit `test: app signing + login-item + socket smoke test`.

### Task 5.3: Finalize + docs
- [ ] Update `AGENTS.md`/`README.md` with `make app` + install (drag to /Applications, first-launch right-click→Open) + the now-complete gate. Manual end-to-end checklist (connect, forward, open, copy, custom port, send file, reconnect after network drop, quit clears forwards, launch-at-login). Commit `doc: build + install + manual test checklist`. Merge `feature/packaging` → main via the full gate.

---

## Self-review (against the spec)

- **Spec coverage:** daemon+socket (Ph2) ✓; protocol incl. open/copy app-side (Ph1/4.2) ✓; single host + Settings switcher via `ListHosts` (2.3/4.3) ✓; remote-only ports (2.x) ✓; forward/open/copy/custom-port/file-transfer/launch-at-login (4.2/4.4/4.5) ✓; daemon lifetime=app lifetime + supervision (2.6/3.3) ✓; Engine trait + Mock (2.1) ✓; menu-bar icon + badge (3.5) ✓; error handling surfaced via `ConnStatus`/`ForwardState::Error`/`Event` (1.4/4.x) ✓; packaging personal local build (5.1) ✓; SwiftPM + Makefile, no .xcodeproj (3.1/5.1) ✓; testing (golden fixtures 1.5/3.2, actor tests 2.4) ✓.
- **Placeholder scan:** load-bearing code (lints, protocol types, error enum, Engine trait, fixture test, Package.swift, gate scripts) is given in full; Phases 2–5 task bodies specify exact files, signatures, channel primitives, commands, and tests rather than "handle edge cases."
- **Type consistency:** `ForwardId`/`Port` newtypes, `RequestBody` `#[serde(tag="type")]`, `DaemonMessage`/`StateSnapshot`/`ForwardState` names are used consistently across Rust (1.x), the fixtures (1.5), and the Swift mirror (3.2).

## Out of scope (per spec): multiple simultaneous hosts; local-ports view; notarization/Homebrew; deep per-forward health beyond "listener alive."
