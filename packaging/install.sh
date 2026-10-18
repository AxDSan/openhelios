#!/usr/bin/env bash
# Install OpenHelios: Rust binaries, udev rule, desktop entry and DKMS module.
# Review the script before running it; it uses sudo and touches /etc.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="${PREFIX:-/usr/local}"
SUDO="${SUDO:-sudo}"

echo "==> removing legacy install names"
for bin in acer-lighting acer-lighting-gui predator-sense predator-sense-gui; do
    $SUDO rm -f "$PREFIX/bin/$bin"
done
for rule in 99-acer-lighting.rules 99-predator-sense.rules; do
    $SUDO rm -f "/etc/udev/rules.d/$rule"
done
for desktop in acer-lighting.desktop predator-sense.desktop; do
    $SUDO rm -f "/usr/share/applications/$desktop"
done
for service in acer-lighting-apply predator-sense-apply; do
    $SUDO systemctl disable --now "$service.service" 2>/dev/null || true
    $SUDO rm -f "/etc/systemd/system/$service.service"
done
$SUDO systemctl disable --now acer-ring-off.service 2>/dev/null || true
$SUDO rm -rf /usr/local/share/doc/predator-sense

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
$SUDO install -Dm755 "$BIN/openhelios" "$PREFIX/bin/openhelios"
$SUDO install -Dm755 "$BIN/openhelios-gui" "$PREFIX/bin/openhelios-gui"

echo "==> creating openhelios group"
if ! getent group openhelios >/dev/null 2>&1; then
    $SUDO groupadd --system openhelios
fi
TARGET_USER="${SUDO_USER:-$(id -un)}"
if [ "$TARGET_USER" != "root" ]; then
    $SUDO usermod -aG openhelios "$TARGET_USER" || true
fi

echo "==> installing udev rule"
$SUDO install -Dm644 "$ROOT/packaging/99-openhelios.rules" /etc/udev/rules.d/99-openhelios.rules
$SUDO udevadm control --reload
$SUDO udevadm trigger --subsystem-match=hidraw || true
$SUDO udevadm trigger --name-match=/dev/acer_gaming_wmi || true

echo "==> installing documentation"
$SUDO install -Dm644 "$ROOT/docs/RECOVERY.md" /usr/local/share/doc/openhelios/RECOVERY.md
$SUDO install -Dm644 "$ROOT/docs/PARITY.md" /usr/local/share/doc/openhelios/PARITY.md
$SUDO install -Dm644 "$ROOT/docs/PROVENANCE.md" /usr/local/share/doc/openhelios/PROVENANCE.md

echo "==> installing desktop entry"
$SUDO install -Dm644 "$ROOT/packaging/openhelios.desktop" /usr/share/applications/openhelios.desktop
$SUDO update-desktop-database /usr/share/applications >/dev/null 2>&1 || true

echo "==> installing GUI autostart entry"
TARGET_HOME="$(getent passwd "$TARGET_USER" | cut -d: -f6)"
TARGET_GROUP="$(id -gn "$TARGET_USER")"
$SUDO install -d -o "$TARGET_USER" -g "$TARGET_GROUP" "$TARGET_HOME/.config/autostart"
sed "s|__PREFIX__|$PREFIX|" \
    "$ROOT/packaging/openhelios-autostart.desktop" \
    | $SUDO tee "$TARGET_HOME/.config/autostart/openhelios.desktop" >/dev/null
$SUDO chown "$TARGET_USER:$TARGET_GROUP" "$TARGET_HOME/.config/autostart/openhelios.desktop"

echo "==> installing and enabling the boot profile service"
sed "s|__HOME__|$TARGET_HOME|" \
    "$ROOT/systemd/openhelios-apply.service" \
    | $SUDO tee /etc/systemd/system/openhelios-apply.service >/dev/null
$SUDO systemctl daemon-reload
$SUDO systemctl enable openhelios-apply.service

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
echo "  GUI:  openhelios-gui"
echo "  CLI:  openhelios doctor"
echo
echo "Access is granted to the openhelios group (and seat uaccess)."
echo "If the keyboard or WMI node is not detected, log out and back in, then run:"
echo "  openhelios doctor"
echo
echo "The saved default profile is applied at boot by openhelios-apply.service."
echo "Disable it with: sudo systemctl disable --now openhelios-apply.service"
echo
echo "Read docs/RECOVERY.md before using overclocking or changing boot services."
