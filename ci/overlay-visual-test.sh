#!/usr/bin/env bash
# Runs inside an Ubuntu container.
# Mounts expected:
#   /usr/local/bin/overlay-adapter  (host-built binary, :ro)
#   /screenshots/                   (host screenshots dir, writable)
# Env expected:
#   DISPLAY                         (host X socket forwarded)
#   XDG_RUNTIME_DIR=/tmp
set -euo pipefail

echo "=== installing runtime deps ==="
apt-get update -qq
DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends \
    libxcb1 libxkbcommon0 libxkbcommon-x11-0 \
    libxau6 libxdmcp6 libxcb-xkb1 \
    mesa-vulkan-drivers vulkan-tools \
    fonts-dejavu-core fonts-noto-core \
    python3 \
    scrot x11-utils x11-apps imagemagick \
    xauth 2>&1 | grep -E '^(E:|Setting up|dpkg:)' || true

echo "=== checking Vulkan ==="
LVP_ICD=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -1)
echo "lavapipe ICD: $LVP_ICD"
VK_ICD_FILENAMES="$LVP_ICD" vulkaninfo --summary 2>&1 | grep -E 'GPU|deviceName' || true

SOCK="${XDG_RUNTIME_DIR:-/tmp}/avro-overlay.sock"
rm -f "$SOCK"

echo "=== starting overlay-adapter (pid will follow) ==="
DISPLAY="$DISPLAY" \
XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}" \
VK_ICD_FILENAMES="$LVP_ICD" \
    /usr/local/bin/overlay-adapter &
OVERLAY_PID=$!
echo "overlay-adapter pid=$OVERLAY_PID"

echo "=== starting fake IPC server ==="
# Creates the socket, waits for overlay-adapter to connect, sends preedit,
# waits for the window to render, then exits.
python3 - <<PYEOF
import socket, os, sys, time, json

path = os.environ.get('XDG_RUNTIME_DIR', '/tmp') + '/avro-overlay.sock'
if os.path.exists(path):
    os.unlink(path)

srv = socket.socket(socket.AF_UNIX)
srv.bind(path)
srv.listen(1)
srv.settimeout(8)

print("socket created at", path, "— waiting for overlay-adapter to connect...", flush=True)
try:
    conn, _ = srv.accept()
except socket.timeout:
    print("FAIL: overlay-adapter did not connect within 8 s", flush=True)
    sys.exit(1)

print("overlay-adapter connected — sending fake preedit", flush=True)
msg = json.dumps({
    'preedit': 'আমার সোনার',
    'suggestions': ['আমার', 'আমি', 'আমাদের', 'আমাদের'],
    'cursor': {'x': 300, 'y': 400, 'w': 2, 'h': 20}
}) + '\n'
conn.send(msg.encode())

print("message sent — waiting 5 s for render...", flush=True)
time.sleep(5)
print("done", flush=True)
PYEOF

echo "=== X11 windows ==="
xwininfo -root -tree 2>/dev/null | grep -E '^\s+(0x|"[^"]+".*[0-9]+x[0-9]+)' | head -30 || true

echo "=== taking screenshots ==="
TIMESTAMP=$(date +%s)

# Full root
scrot "/screenshots/full-${TIMESTAMP}.png" && echo "full screenshot saved" || echo "scrot failed"

# Capture overlay window directly by ID (bypasses compositor alpha compositing)
OVERLAY_WID=$(xwininfo -root -tree 2>/dev/null \
    | grep '320x60' | awk '{print $1}' | head -1)
echo "overlay window id: $OVERLAY_WID"
if [ -n "$OVERLAY_WID" ]; then
    xwd -id "$OVERLAY_WID" -silent -out /tmp/overlay.xwd 2>/dev/null && \
        convert /tmp/overlay.xwd "/screenshots/overlay-xwd-${TIMESTAMP}.png" && \
        echo "xwd capture saved" || echo "xwd/convert failed"
fi

kill "$OVERLAY_PID" 2>/dev/null || true
wait "$OVERLAY_PID" 2>/dev/null || true
echo "=== test complete ==="
