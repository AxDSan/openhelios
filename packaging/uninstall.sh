#!/usr/bin/env bash
# Remove the installed OpenHelios binaries, udev rule, service and DKMS module.
# User profiles and snapshots are intentionally preserved.
set -euo pipefail

SUDO="${SUDO:-sudo}"

echo "==> stopping boot service if enabled"
for service in openhelios-apply predator-sense-apply acer-lighting-apply; do
    $SUDO systemctl disable --now "$service.service" 2>/dev/null || true
done

echo "==> removing installed files"
for bin in openhelios openhelios-gui predator-sense predator-sense-gui acer-lighting acer-lighting-gui; do
    $SUDO rm -f "/usr/local/bin/$bin"
done
for rule in 99-openhelios.rules 99-predator-sense.rules 99-acer-lighting.rules; do
    $SUDO rm -f "/etc/udev/rules.d/$rule"
done
for desktop in openhelios.desktop predator-sense.desktop acer-lighting.desktop; do
    $SUDO rm -f "/usr/share/applications/$desktop"
done
for service in openhelios-apply predator-sense-apply acer-lighting-apply; do
    $SUDO rm -f "/etc/systemd/system/$service.service"
done
$SUDO rm -rf /usr/local/share/doc/openhelios /usr/local/share/doc/predator-sense
$SUDO udevadm control --reload || true
$SUDO systemctl daemon-reload || true

echo "==> removing DKMS module"
$SUDO rmmod acer_gaming_probe 2>/dev/null || true
$SUDO dkms remove -m acer_gaming_probe -v 1.0 --all 2>/dev/null || true
$SUDO rm -rf /usr/src/acer_gaming_probe-1.0

echo
echo "Preserved user data:"
echo "  ~/.config/openhelios/profiles"
echo "  ~/.local/state/openhelios/snapshots"
echo "  (legacy ~/.config/predator-sense and ~/.config/acer-lighting are untouched)"
echo
echo "A reboot (or EC reset) returns the EC to its firmware defaults."
