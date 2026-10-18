#!/usr/bin/env bash
# Install the Rust control panel, udev rule, desktop entry and DKMS module.
# Review the script before running it; it uses sudo and touches /etc.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="${PREFIX:-/usr/local}"
SUDO="${SUDO:-sudo}"

echo "==> removing legacy acer-lighting install names"
$SUDO rm -f "$PREFIX/bin/acer-lighting" "$PREFIX/bin/acer-lighting-gui"
$SUDO rm -f /etc/udev/rules.d/99-acer-lighting.rules
$SUDO rm -f /usr/share/applications/acer-lighting.desktop
$SUDO rm -f /etc/systemd/system/acer-lighting-apply.service

if [[ "${RELEASE:-1}" == "1" ]]; then
    echo "==> building release binaries"
    cargo build --release --manifest-path "$ROOT/Cargo.toml"
    BIN="$ROOT/target/release"
else
    echo "==> building debug binaries (RELEASE=0)"
    cargo build --manifest-path "$ROOT/Cargo.toml"
    BIN="$ROOT/target/debug"
fi

echo "==> installing binaries to $PREFIX/bin"
$SUDO install -Dm755 "$BIN/predator-sense" "$PREFIX/bin/predator-sense"
$SUDO install -Dm755 "$BIN/predator-sense-gui" "$PREFIX/bin/predator-sense-gui"

echo "==> creating predator-sense group"
if ! getent group predator-sense >/dev/null 2>&1; then
    $SUDO groupadd --system predator-sense
fi
TARGET_USER="${SUDO_USER:-$(id -un)}"
if [ "$TARGET_USER" != "root" ]; then
    $SUDO usermod -aG predator-sense "$TARGET_USER" || true
fi

echo "==> installing udev rule"
$SUDO install -Dm644 "$ROOT/packaging/99-predator-sense.rules" /etc/udev/rules.d/99-predator-sense.rules
$SUDO udevadm control --reload
$SUDO udevadm trigger --subsystem-match=hidraw || true
$SUDO udevadm trigger --name-match=/dev/acer_gaming_wmi || true

echo "==> installing documentation"
$SUDO install -Dm644 "$ROOT/docs/RECOVERY.md" /usr/local/share/doc/predator-sense/RECOVERY.md
$SUDO install -Dm644 "$ROOT/docs/PARITY.md" /usr/local/share/doc/predator-sense/PARITY.md

echo "==> installing desktop entry"
$SUDO install -Dm644 "$ROOT/packaging/predator-sense.desktop" /usr/share/applications/predator-sense.desktop
$SUDO update-desktop-database /usr/share/applications >/dev/null 2>&1 || true

echo "==> installing and enabling the boot profile service"
TARGET_HOME="$(getent passwd "$TARGET_USER" | cut -d: -f6)"
sed "s|__HOME__|$TARGET_HOME|" \
    "$ROOT/systemd/predator-sense-apply.service" \
    | $SUDO tee /etc/systemd/system/predator-sense-apply.service >/dev/null
$SUDO systemctl daemon-reload
$SUDO systemctl disable --now acer-ring-off.service 2>/dev/null || true
$SUDO systemctl disable --now acer-lighting-apply.service 2>/dev/null || true
$SUDO systemctl enable predator-sense-apply.service

if command -v dkms >/dev/null 2>&1; then
    echo "==> installing DKMS module"
    if ! dkms status 2>/dev/null | grep -q '^acer_gaming_probe/1.0'; then
        $SUDO rm -rf /usr/src/acer_gaming_probe-1.0
        $SUDO cp -r "$ROOT/kmod" /usr/src/acer_gaming_probe-1.0
        $SUDO dkms add -m acer_gaming_probe -v 1.0 || true
    fi
    $SUDO dkms install -m acer_gaming_probe -v 1.0 -k "$(uname -r)" || true
    $SUDO modprobe acer_gaming_probe || true
else
    echo "==> dkms not found; building the module once"
    (cd "$ROOT/kmod" && make)
    $SUDO insmod "$ROOT/kmod/acer_gaming_probe.ko" || true
fi

echo
echo "Installed."
echo "  GUI:  predator-sense-gui"
echo "  CLI:  predator-sense doctor"
echo
echo "Access is granted to the predator-sense group (and seat uaccess)."
echo "If the keyboard is not detected, log out and back in, then run:"
echo "  predator-sense doctor"
echo
echo "The saved default profile is applied at boot by predator-sense-apply.service."
echo "Disable it with: sudo systemctl disable --now predator-sense-apply.service"
echo
echo "Read docs/RECOVERY.md before using overclocking or changing boot services."
