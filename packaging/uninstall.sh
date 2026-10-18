#!/usr/bin/env bash
# Remove the installed binaries, udev rule, service and DKMS module.
# User profiles and snapshots are intentionally preserved.
set -euo pipefail

SUDO="${SUDO:-sudo}"

echo "==> stopping boot service if enabled"
$SUDO systemctl disable --now predator-sense-apply.service 2>/dev/null || true

echo "==> removing installed files"
$SUDO rm -f /usr/local/bin/predator-sense /usr/local/bin/predator-sense-gui
$SUDO rm -f /usr/local/bin/acer-lighting /usr/local/bin/acer-lighting-gui
$SUDO rm -f /etc/udev/rules.d/99-predator-sense.rules
$SUDO rm -f /etc/udev/rules.d/99-acer-lighting.rules
$SUDO rm -f /usr/share/applications/predator-sense.desktop
$SUDO rm -f /usr/share/applications/acer-lighting.desktop
$SUDO rm -f /etc/systemd/system/predator-sense-apply.service
$SUDO rm -f /etc/systemd/system/acer-lighting-apply.service
$SUDO udevadm control --reload || true
$SUDO systemctl daemon-reload || true

echo "==> removing DKMS module"
$SUDO rmmod acer_gaming_probe 2>/dev/null || true
$SUDO dkms remove -m acer_gaming_probe -v 1.0 --all 2>/dev/null || true
$SUDO rm -rf /usr/src/acer_gaming_probe-1.0

echo
echo "Preserved user data:"
echo "  ~/.config/predator-sense/profiles"
echo "  ~/.local/state/predator-sense/snapshots"
echo
echo "A reboot (or EC reset) returns the EC to its firmware defaults."
