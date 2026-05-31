#!/usr/bin/env bash
# Advisory post-build smoke test for build/Ports.app.
#
# Runs after `make app`. Performs:
#   1. Bundle existence + codesign --verify --deep --strict   (HARD)
#   2. Bundled-daemon Unix-socket ping/ack round-trip          (HARD)
#   3. Login-item bundle-structure check                       (SOFT/best-effort)
#
# Exits non-zero on any HARD failure.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

APP="build/Ports.app"
PORTS_BIN="$APP/Contents/Resources/ports"
SWIFT_BIN="$APP/Contents/MacOS/PortsBar"
INFO_PLIST="$APP/Contents/Info.plist"
APP_ICON="$APP/Contents/Resources/AppIcon.icns"
EXPECTED_BUNDLE_ID="com.ports.app"
EXPECTED_ICON_FILE="AppIcon"

PASS_PREFIX="PASS"
FAIL_PREFIX="FAIL"
NOTE_PREFIX="NOTE"

fail() {
	echo "$FAIL_PREFIX: $*" >&2
	exit 1
}

ok() {
	echo "$PASS_PREFIX: $*"
}

note() {
	echo "$NOTE_PREFIX: $*"
}

echo "==> smoke-app: checking $APP"

# ---------------------------------------------------------------------------
# 1. Bundle existence + codesign verify (HARD)
# ---------------------------------------------------------------------------
[ -d "$APP" ] || fail "$APP does not exist (run 'make app' first)"
ok "bundle present: $APP"

if codesign --verify --deep --strict "$APP"; then
	ok "codesign --verify --deep --strict"
else
	fail "codesign verification failed for $APP"
fi

# ---------------------------------------------------------------------------
# 2. Bundled-daemon socket ping/ack (HARD)
# ---------------------------------------------------------------------------
[ -x "$PORTS_BIN" ] || fail "bundled daemon binary missing or not executable: $PORTS_BIN"

SOCK="/tmp/ports-smoke-$$.sock"
rm -f "$SOCK"

"$PORTS_BIN" daemon --socket "$SOCK" &
DAEMON_PID=$!

cleanup() {
	if kill -0 "$DAEMON_PID" 2>/dev/null; then
		kill "$DAEMON_PID" 2>/dev/null || true
		wait "$DAEMON_PID" 2>/dev/null || true
	fi
	rm -f "$SOCK"
}
trap cleanup EXIT

# Poll up to ~5s for the socket file to appear.
for _ in $(seq 1 50); do
	[ -S "$SOCK" ] && break
	# If the daemon died early, bail out.
	if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
		fail "daemon exited before creating socket $SOCK"
	fi
	sleep 0.1
done
[ -S "$SOCK" ] || fail "socket $SOCK did not appear within ~5s"
ok "daemon socket appeared: $SOCK"

# Send one NDJSON ping and require an ack for id 1 in the reply stream.
# The daemon also pushes a State line on connect; accept either ordering.
PING_RESULT="$(SOCK_PATH="$SOCK" python3 - <<'PY'
import json, os, socket, sys

sock_path = os.environ["SOCK_PATH"]
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(5.0)
s.connect(sock_path)
s.sendall(b'{"id":1,"type":"ping"}\n')

buf = b""
ack_found = False
deadline_chunks = 0
try:
    while deadline_chunks < 50:
        try:
            chunk = s.recv(4096)
        except socket.timeout:
            break
        if not chunk:
            break
        buf += chunk
        while b"\n" in buf:
            line, buf = buf.split(b"\n", 1)
            line = line.strip()
            if not line:
                continue
            try:
                msg = json.loads(line.decode("utf-8"))
            except Exception:
                continue
            if msg.get("type") == "ack" and msg.get("id") == 1:
                ack_found = True
                print("ACK_LINE=" + line.decode("utf-8"))
                break
        if ack_found:
            break
        deadline_chunks += 1
finally:
    s.close()

if ack_found:
    print("ACK_OK")
    sys.exit(0)
else:
    print("ACK_MISSING (raw=%r)" % buf)
    sys.exit(2)
PY
)" || {
	echo "$PING_RESULT" >&2
	fail "no ack received for ping id=1 from bundled daemon"
}

echo "$PING_RESULT"
echo "$PING_RESULT" | grep -q "ACK_OK" || fail "ping did not yield ACK_OK"
ok "bundled-daemon socket ping -> ack id=1"

# Tear down the daemon now (trap will also run, but be explicit).
cleanup
trap - EXIT

# ---------------------------------------------------------------------------
# 3. Login-item structure (SOFT / best-effort)
# ---------------------------------------------------------------------------
echo "==> login-item structure checks"

[ -f "$INFO_PLIST" ] || fail "Info.plist missing: $INFO_PLIST"
plutil -lint "$INFO_PLIST" >/dev/null || fail "Info.plist failed plutil -lint"
ok "Info.plist present and parseable"

bundle_id="$(/usr/libexec/PlistBuddy -c "Print :CFBundleIdentifier" "$INFO_PLIST" 2>/dev/null || true)"
[ "$bundle_id" = "$EXPECTED_BUNDLE_ID" ] || fail "CFBundleIdentifier is '$bundle_id', expected '$EXPECTED_BUNDLE_ID'"
ok "CFBundleIdentifier = $EXPECTED_BUNDLE_ID"

icon_file="$(/usr/libexec/PlistBuddy -c "Print :CFBundleIconFile" "$INFO_PLIST" 2>/dev/null || true)"
[ "$icon_file" = "$EXPECTED_ICON_FILE" ] || fail "CFBundleIconFile is '$icon_file', expected '$EXPECTED_ICON_FILE'"
ok "CFBundleIconFile = $EXPECTED_ICON_FILE"

[ -f "$APP_ICON" ] || fail "app icon missing: $APP_ICON"
ok "app icon resource present: $APP_ICON"

lsui="$(/usr/libexec/PlistBuddy -c "Print :LSUIElement" "$INFO_PLIST" 2>/dev/null || true)"
[ "$lsui" = "true" ] || fail "LSUIElement is '$lsui', expected 'true'"
ok "LSUIElement = true (menu-bar only)"

[ -x "$SWIFT_BIN" ] || fail "front-end executable missing or not executable: $SWIFT_BIN"
ok "front-end executable present: $SWIFT_BIN"

if codesign --verify --strict "$SWIFT_BIN" 2>/dev/null; then
	ok "front-end executable is signed"
else
	# Nested executables are covered by the bundle signature; this is advisory.
	note "front-end executable not independently verifiable (covered by bundle signature)"
fi

# True SMAppService.mainApp register/unregister requires the GUI app to run as
# itself within a login session; that is not reliable headless. We validate the
# bundle is *structured* for a login item and treat runtime register/unregister
# as covered by the in-app toggle (see manual checklist).
note "login-item: bundle structure OK (runtime register/unregister validated via in-app toggle -- see manual checklist)"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo
echo "============================================================"
echo "smoke-app: ALL HARD CHECKS PASSED"
echo "  - codesign verify: PASS"
echo "  - bundled-daemon socket ping/ack: PASS"
echo "  - login-item structure: PASS (runtime toggle: manual checklist)"
echo "============================================================"
