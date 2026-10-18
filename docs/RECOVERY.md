# Recovery and disaster recovery

This tool controls two reversible channels:

| channel | what it writes | what it never writes |
|---|---|---|
| ACPI-WMI mailbox (`/dev/acer_gaming_wmi`) | LED groups, colours, fan behaviour, OC level, misc settings | EC/SPI firmware, BIOS, NVRAM |
| vendor HID `0d62:9a1b` | 9-byte feature reports and 65-byte output reports | firmware, descriptors, USB configuration |

There is no firmware flashing path in this repository. The worst case for a
lighting command is a stuck light; the worst case for an overclock command is an
unstable system until the EC/BIOS is reset. Keep the USB keyboard working and
read this page before raising power limits.

## 1. The safe state

```sh
predator-sense panic
```

This turns the keyboard and chassis lights off, sets every supported fan back to
automatic, and writes firmware OC level 0 for CPU and GPU. It is allowed even
when normal writes are disabled. Use it first if anything looks wrong.

The known-good profile is:

```sh
predator-sense profile apply default
```

The built-in default is static cyan at 50% brightness, ring off, heat-pipe off,
fans automatic, OC normal. You can inspect it with:

```sh
predator-sense profile show default
```

## 2. Snapshots

Every profile apply (unless `--no-snapshot`) writes a JSON snapshot first:

```
~/.local/state/predator-sense/snapshots/YYYYMMDD-HHMMSS.json
~/.local/state/predator-sense/snapshots/latest.json
```

Capture one explicitly before an experiment:

```sh
predator-sense snapshot
predator-sense snapshot --profile my-profile
```

The snapshot records raw WMI responses for sysinfo, LED groups, keyboard
backlight, misc settings, fan behaviour, HID getters, RAPL limits and the
relevant sysfs values. There is no universal "read every EC register" API, so
the snapshot is a best-effort baseline, not a byte-for-byte EC dump.

Restore:

```sh
predator-sense restore                 # safe state: lights off, fans auto, OC normal
predator-sense restore --apply-profile # re-apply the profile stored in the snapshot
```

## 3. If the keyboard or GUI misbehaves

1. Run `predator-sense panic` from a terminal (or the Recovery tab).
2. Use an external USB keyboard if the internal one stops responding.
3. Power off, unplug AC and any second adapter, then hold the power button for
   15 seconds. This is the EC reset on this chassis.
4. If the machine has a pinhole reset, use it with the AC adapter disconnected.
5. Boot again and check `predator-sense doctor`.

The TURBO indicator is owned by the EC. It can stay lit even when the WMI
surface is idle; that is normal and not a failure of this tool.

## 4. Removing the kernel module and udev rule

Immediate:

```sh
sudo rmmod acer_gaming_probe
```

Persistent (the installed module was added with DKMS):

```sh
sudo dkms remove -m acer_gaming_probe -v 1.0 --all
sudo rm -f /etc/modules-load.d/acer-gaming-probe.conf
sudo rm -f /etc/udev/rules.d/99-predator-sense.rules
sudo udevadm control --reload
```

If you cannot boot because a boot-time profile or module is misbehaving, add one
of these to the kernel command line in GRUB:

```
module_blacklist=acer_gaming_probe
systemd.unit=multi-user.target
```

Then remove the boot service:

```sh
sudo systemctl disable --now predator-sense-apply.service
```

## 5. Overclock recovery

The Windows OC page uses Intel XTU profiles and NVAPI. This Linux implementation
uses three separate layers:

1. **Firmware OC level** (EC): `predator-sense oc cpu 0`, `predator-sense oc gpu 0`.
2. **RAPL PL1/PL2**: `predator-sense oc power pl1 <W>`, `predator-sense oc power pl2 <W>`.
3. **NVIDIA power/clocks**: `predator-sense oc nvidia reset`.

If the system becomes unstable after raising limits:

1. Boot on AC power (OC preflight requires AC and at least 40% battery).
2. Run `predator-sense panic`; it writes OC level 0 and fans automatic.
3. Run `predator-sense oc power pl1 45`, `oc power pl2 56` (or your stock values
   from the snapshot), and `predator-sense oc nvidia reset`.
4. Remove the boot service if it applies an aggressive profile.
5. Enter the BIOS and load setup defaults (F9 on this Acer firmware), then save
   and exit. A full EC reset (step 3 above) also clears volatile OC state.

The firmware OC level is an EC setting; if it survives a warm reboot, the BIOS
defaults or EC reset is the correct recovery. Do not repeatedly raise it while
the machine is unstable.

## 6. Permissions

The HID device is root-owned by default. The shipped udev rule is:

```
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="0d62", ATTRS{idProduct}=="9a1b", MODE="0660", GROUP="predator-sense", TAG+="uaccess"
KERNEL=="acer_gaming_wmi", MODE="0660", GROUP="predator-sense"
```

If `predator-sense doctor` reports `permission denied`:

```sh
sudo cp packaging/99-predator-sense.rules /etc/udev/rules.d/
sudo udevadm control --reload
sudo udevadm trigger --subsystem-match=hidraw
sudo udevadm trigger --name-match=/dev/acer_gaming_wmi
sudo usermod -aG predator-sense "$USER"   # then log out and back in
```

The dedicated `predator-sense` group only grants access to this keyboard and
the WMI bridge; it does not grant access to every input device. The `TAG+="uaccess"` fallback only works on a normal local seat.

The probe module creates `/dev/acer_gaming_wmi` with a permissive mode; the
shipped udev rule narrows it to the `predator-sense` group. Members of that
group can send WMI method ids to the EC, so treat group membership as hardware
control access.

## 7. Backups

Back up both directories together:

```sh
predator-sense profile export ~/predator-sense-profiles.json
tar -czf ~/predator-sense-state.tgz \
  ~/.config/predator-sense \
  ~/.local/state/predator-sense
```

Restore profiles:

```sh
predator-sense profile import ~/predator-sense-profiles.json
```

The repository itself (`predator-sense/`) is the source of truth for the kernel
module, the protocol implementation and the notes. `work/` contains downloaded
Windows artifacts and is intentionally ignored by git.

## 8. Quick command table

| situation | command |
|---|---|
| lights stuck | `predator-sense panic` |
| want the normal look | `predator-sense profile apply default` |
| before an experiment | `predator-sense snapshot` |
| restore the last snapshot | `predator-sense restore --apply-profile` |
| check devices/permissions | `predator-sense doctor` |
| read EC sensors | `predator-sense monitor` |
| remove the kernel module now | `sudo rmmod acer_gaming_probe` |
| remove everything installed | `sudo packaging/uninstall.sh` |

If a command returns an error, it is reported before any partial write is
attempted where possible. If you are unsure, stop, run `doctor`, and use
`--dry-run` to see what would be written.
