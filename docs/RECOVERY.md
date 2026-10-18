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
openhelios panic
```

This turns the keyboard and chassis lights off, sets every supported fan back to
automatic, and writes firmware OC level 0 for CPU and GPU. It is allowed even
when normal writes are disabled. Use it first if anything looks wrong.

The known-good profile is:

```sh
openhelios profile apply default
```

The built-in default is static cyan at 50% brightness, ring off, heat-pipe off,
fans automatic, OC normal. You can inspect it with:

```sh
openhelios profile show default
```

## 2. Snapshots

Every profile apply (unless `--no-snapshot`) writes a JSON snapshot first:

```
~/.local/state/openhelios/snapshots/YYYYMMDD-HHMMSS.json
~/.local/state/openhelios/snapshots/latest.json
```

Capture one explicitly before an experiment:

```sh
openhelios snapshot
openhelios snapshot --profile my-profile
```

The snapshot records raw WMI responses for sysinfo, LED groups, keyboard
backlight, misc settings, fan behaviour, HID getters, RAPL limits and the
relevant sysfs values. There is no universal "read every EC register" API, so
the snapshot is a best-effort baseline, not a byte-for-byte EC dump.

Restore:

```sh
openhelios restore                 # safe state: lights off, fans auto, OC normal
openhelios restore --apply-profile # re-apply the profile stored in the snapshot
```

## 3. If the keyboard or GUI misbehaves

1. Run `openhelios panic` from a terminal (or the Recovery tab).
2. Use an external USB keyboard if the internal one stops responding.
3. Power off, unplug AC and any second adapter, then hold the power button for
   15 seconds. This is the EC reset on this chassis.
4. If the machine has a pinhole reset, use it with the AC adapter disconnected.
5. Boot again and check `openhelios doctor`.

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
sudo rm -f /etc/udev/rules.d/99-openhelios.rules
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
sudo systemctl disable --now openhelios-apply.service
```

## 5. Overclock recovery

The Windows OC page uses Intel XTU profiles and NVAPI. This Linux implementation
uses three separate layers:

1. **Firmware OC level** (EC): `openhelios oc cpu 0`, `openhelios oc gpu 0`.
2. **RAPL PL1/PL2**: `openhelios oc power pl1 <W>`, `openhelios oc power pl2 <W>`.
3. **NVIDIA power/clocks**: `openhelios oc nvidia reset`.

If the system becomes unstable after raising limits:

1. Boot on AC power (OC preflight requires AC and at least 40% battery).
2. Run `openhelios panic`; it writes OC level 0 and fans automatic.
3. Run `openhelios oc power pl1 45`, `oc power pl2 56` (or your stock values
   from the snapshot), and `openhelios oc nvidia reset`.
4. Remove the boot service if it applies an aggressive profile.
5. Enter the BIOS and load setup defaults (F9 on this Acer firmware), then save
   and exit. A full EC reset (step 3 above) also clears volatile OC state.

The firmware OC level is an EC setting; if it survives a warm reboot, the BIOS
defaults or EC reset is the correct recovery. Do not repeatedly raise it while
the machine is unstable.

## 6. Permissions

The HID device is root-owned by default. The shipped udev rule is:

```
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="0d62", ATTRS{idProduct}=="9a1b", MODE="0660", GROUP="openhelios", TAG+="uaccess"
KERNEL=="acer_gaming_wmi", MODE="0660", GROUP="openhelios", TAG+="seat", TAG+="uaccess", ENV{ID_FOR_SEAT}="acer-gaming-wmi", ENV{ID_SEAT}="seat0"
```

If `openhelios doctor` reports `permission denied`:

```sh
sudo cp packaging/99-openhelios.rules /etc/udev/rules.d/
sudo udevadm control --reload
sudo udevadm trigger --subsystem-match=hidraw
sudo udevadm trigger --name-match=/dev/acer_gaming_wmi
sudo usermod -aG openhelios "$USER"   # then log out and back in
```

The dedicated `openhelios` group only grants access to this keyboard and
the WMI bridge; it does not grant access to every input device. Both rules
carry `TAG+="uaccess"`, so the active seat user gets an ACL immediately. The
WMI node is virtual and has no udev seat path; its rule sets `ID_FOR_SEAT` and
`ID_SEAT` explicitly so the ACL is applied. Group membership still works after
a re-login.

The probe module creates `/dev/acer_gaming_wmi` with a permissive mode; the
shipped udev rule narrows it to the `openhelios` group. Members of that
group can send WMI method ids to the EC, so treat group membership as hardware
control access.

## 7. Backups

Back up both directories together:

```sh
openhelios profile export ~/openhelios-profiles.json
tar -czf ~/openhelios-state.tgz \
  ~/.config/openhelios \
  ~/.local/state/openhelios
```

Restore profiles:

```sh
openhelios profile import ~/openhelios-profiles.json
```

The repository itself is the source of truth for the kernel module, the
protocol implementation and the notes. A local `work/` scratch directory may
contain the downloaded Store package used for analysis; it is ignored by git
and must never be committed or included in a release. See
[`PROVENANCE.md`](PROVENANCE.md).

## 8. Quick command table

| situation | command |
|---|---|
| lights stuck | `openhelios panic` |
| want the normal look | `openhelios profile apply default` |
| before an experiment | `openhelios snapshot` |
| restore the last snapshot | `openhelios restore --apply-profile` |
| check devices/permissions | `openhelios doctor` |
| read EC sensors | `openhelios monitor` |
| remove the kernel module now | `sudo rmmod acer_gaming_probe` |
| remove everything installed | `sudo packaging/uninstall.sh` |

If a command returns an error, it is reported before any partial write is
attempted where possible. If you are unsure, stop, run `doctor`, and use
`--dry-run` to see what would be written.
