# Protocol reference

This is a clean-room description of the two hardware channels used by
PredatorSense Linux on the Acer Predator PH717-71 (`Levante_CFS`). It records
the **facts** needed to implement the controls; it does not contain Acer
application code or firmware.

The command values were recovered by observing the shipped Windows application
and its service on this hardware, then verified by reading the device back and
by watching the physical lights.

## 1. ACPI-WMI gaming-function mailbox

The firmware exposes a WMI device (`_HID PNP0C14`, `_UID "APGe"`) whose
`_WDG` maps the `AcerGamingFunction` GUID to the ACPI method `WMBH`:

```
7A4DDFE7-5B5D-40B4-8595-4408E0CC7F56  ->  WMBH
```

`WMBH(Arg1, Arg2)` is a dispatcher: `Arg1` is a method id, `Arg2` is an input
buffer. The kernel module `acer_gaming_probe.ko` calls the exported
`wmi_evaluate_method()` and exposes:

```
write(): byte 0 = method id, bytes 1..N = payload
read() : raw WMI response of the last call
```

### Method ids

| id | name | payload / response |
|---:|---|---|
| 2 | `SetGamingLED` | u64 selector |
| 4 | `GetGamingLED` | — / u64 |
| 5 | `GetGamingSysInfo` | u64 selector / u64 |
| 6 | `SetGamingStaticLED` | `[zone, R, G, B]` |
| 10 | `SetGamingLEDGroupBehavior` | u64 |
| 11 | `GetGamingLEDGroupBehavior` | u64 selector / u64 |
| 12 | `SetGamingLEDGroupColor` | u64 |
| 13 | `GetGamingLEDGroupColor` | u64 selector / u64 |
| 14 | `SetGamingFanGroupBehavior` | u64 |
| 15 | `GetGamingFanGroupBehavior` | u64 selector / u64 |
| 16 | `SetGamingFanGroupSpeed` | u64 |
| 17 | `GetGamingFanGroupSpeed` | u64 selector / u64 |
| 18 | `SetGamingFanTable` | u64 |
| 19 | `GetGamingFanTable` | u64 selector / u64 |
| 20 | `SetGamingKBBacklight` | 16-byte effect struct **or** u64 |
| 21 | `GetGamingKBBacklight` | — / 16 bytes |
| 22 | `SetGamingMiscSetting` | u64 |
| 23 | `GetGamingMiscSetting` | u64 selector / u64 |

Methods 2, 6 and 20 are accepted on the PH717-71 but are **visibly inert** for
the keyboard/ring/heat-pipe: this unit's lights are on the vendor HID channel
and WMI LED groups. The tool still implements the payloads for other models.

### LED groups (method 10)

Payload: `group_id | (1 << on_bit)` to turn on, `group_id` alone to turn off.

| group | `group_id` | on bit |
|---|---:|---:|
| CPU fan | `0x20` | 26 |
| GPU fan / heat-pipe | `0x40` | 28 |
| Touchpad ring | `0x80` | 30 |
| Light 1 / logo | `0x100` | 40 |
| Other light | `0x01` | 16 |

Example: ring on = `0x0000000040000080`, ring off = `0x0000000000000080`.

### LED colours (method 12)

Payload: `index | R<<8 | G<<16 | B<<24`, optionally `brightness<<32` and
`1<<40` (enable).

| index | light |
|---:|---|
| 6 | CPU fan |
| 7 | GPU fan / heat-pipe |
| 8 | touchpad ring |
| 9 | light 1 / logo |
| 1 | other light |

The PH717-71 accepts ring colour writes but the ring is a fixed-blue on/off
light, not an RGB zone.

### Keyboard-backlight struct (method 20, 16 bytes)

| byte | meaning |
|---:|---|
| 0 | mode / effect |
| 1 | speed |
| 2 | brightness |
| 3 | `0x08` for wave, else `0` |
| 4 | direction |
| 5,6,7 | R, G, B |
| 9 | enable (`1`) |

PredatorSense also uses a **u64 form** of method 20 for the touchpad ring:
`0 | (brightness << 16)`.

### Fan groups (methods 14/16)

Payloads recovered from the Windows fan routine:

| group | auto behaviour | manual behaviour | speed selector |
|---|---:|---:|---:|
| CPU | `0x10001` | `0x30001` | 1 |
| System 1 | `0x40002` | `0xC0002` | 2 |
| System 2 | `0x10004` | `0x30004` | 3 |
| GPU 1 | `0x40008` | `0xC0008` | 4 |
| GPU 2 | `0x10010` | `0x30010` | 5 |

Manual speed: `speed_selector | (percent << 8)`.
The PH717-71 uses CPU and GPU groups; system groups report zero.

### Health / sensor indices (method 5)

PredatorSense calls `GetGamingSysInfo(1 | (index << 8))`. The low byte is `1`;
the response is a u64 where `(value & 0xff) == 0` means success and
`(value >> 8) & 0xffff` is the reading.

| index | reading | observed PH717-71 |
|---:|---|---:|
| 1 | CPU temperature | 50 °C |
| 2 | CPU fan speed | 1894 rpm |
| 3 | system temperature | 52 °C |
| 4 | system fan speed | 0 |
| 5 | system fan speed external | 0 |
| 6 | GPU 1 fan speed | 1681 rpm |
| 7 | system 2 temperature | 0 |
| 8 | system 2 fan speed | 0 |
| 9 | GPU 2 fan speed | 0 |
| 10 | GPU 1 temperature | 47 °C |
| 11 | GPU 2 temperature | 0 |
| 12 | CPU fan rate | 0 |
| 13 | system fan rate | 0 |
| 14 | system 2 fan rate | 0 |
| 15 | system fan rate external | 0 |
| 16 | GPU 1 fan rate | 0 |
| 17 | GPU 2 fan rate | 0 |

### Firmware overclock (method 22)

`SetGamingMiscSetting` payloads:

| function | payload |
|---|---|
| CPU level 0 / 1 / 2 | `7` / `263` / `519` |
| GPU level 0 / 1 / 2 | `5` / `261` / `517` |
| Max-Q mode 1, level 0/1/2 | `4101` / `4357` / `4613` |
| Max-Q mode 2, level 0/1/2 | `8197` / `8453` / `8709` |
| GPU mode `m` | `2 \| (m << 8)` |
| POST sound on/off | `6 \| (state << 8)` |

## 2. Vendor HID keyboard (`0d62:9a1b`)

The keyboard is a Holtek/Darfon composite device. The `0xFF01` interface
(`input3`) exposes:

- 64-byte input report;
- 64-byte output report;
- 8-byte feature report.

The Linux tool writes the same reports as the Windows service.

### Feature command framing

The Windows agent sends a **9-byte feature report**:

```
[0] report id (0)
[1..8] command bytes
```

The last command byte is a checksum:

```
cmd[7] = ~(cmd[0] + cmd[1] + ... + cmd[6])
```

Example getter: `00 88 00 00 00 00 00 00 77` returns a 9-byte reply; the
8 bytes after the report id are the response. On the PH717-71 a healthy reply is
`88 00 33 05 32 08 01 00` (byte 4 tracks the current brightness).

### Static per-key colours

1. Feature command `12 00 00 08 00 00 00 <chk>`.
2. Sleep ~100 ms.
3. Write the 512-byte colour buffer as eight 65-byte output reports:
   `[0] + 64 bytes`. The buffer is 128 slots of `[0, R, G, B]`; slot 126 is the
   touchpad entry.
4. Sleep ~100 ms.
5. Feature command `08 02 33 05 <brightness> 08 01 <chk>`.

The key tag is the slot index. The `en-us` layout has 103 keys with tags
`0..125`; tag 126 is the touchpad.

### Effects

For a non-static effect:

1. `08 02 00 00 00 00 00 <chk>`
2. `14 00 00 R G B 00 <chk>`
3. `08 00 <effect> <abs(speed-10)> <brightness> <1|8> <direction> <chk>`

`1` in byte 5 is the normal tail; `8` is used for effect 9 (wave).
The effect ids used by the Windows UI are: 2 static, 5 breathing, 11 neon,
9 wave, 4 ripple, 12 rainbow, 13 snake, 3 zoom, 8 spot, 6 star, 10 wave cold,
and 17–22 for the remaining patterns.

Brightness-only updates use the `(51, 5, 1)` triple.

### MagForce

| function | command |
|---|---|
| switch / linear / joystick | `31 <switch> <linear> <joystick> 00 00 00 <chk>` |
| actuation point | `32 <value> 00 00 00 00 00 <chk>` |
| game keys (header) | `34 00 00 19 00 00 00 <chk>` + 25-byte output report |
| analog curve | `35 <mode> 00 00 00 00 00 <chk>` |
| replace mode | `30 01 <30s> <status> 00 00 00 <chk>` |

Getters: `B0` (30-second backlight), `B1` (key setting), `B2` (actuation),
`B4` (game keys), `B5` (analog curve).

Game-key payload: a 25-byte map sent as a 65-byte output report; the four
selected slots are set to the HID usages `0x1a` (W), `0x04` (A), `0x16` (S),
`0x07` (D).

## 3. Verified and inert

Verified on the PH717-71:

- per-key static RGB, effects and brightness (HID);
- MagForce actuation read/write and setting commands (HID);
- ring and heat-pipe on/off (WMI 10);
- fan auto/manual (WMI 14/16);
- EC CPU/system/GPU temperatures and fan RPM (WMI 5 health);
- firmware OC levels (WMI 22, level 0 verified; higher levels gated by
  `--allow-oc`).

Accepted but visibly inert on this unit:

- WMI method 2 (`SetGamingLED`) for the TURBO indicator;
- WMI method 6 static zones;
- WMI method 20 keyboard-backlight struct;
- ring colour via method 12.

These are implemented because other models use them, but the tool marks them as
unverified/inert where appropriate.

## 4. Safety

No firmware, EC flash, SPI, BIOS or NVRAM is written. The kernel module only
calls the exported WMI method. The HID writes are feature/output reports. The
tool gates writes with `--read-only`, `--dry-run` and `--allow-oc`, snapshots
before hardware writes, and provides `panic`/`restore`. See
[`RECOVERY.md`](RECOVERY.md).

## 5. Evidence and clean-room statement

The protocol was recovered from the public ACPI tables, the USB HID descriptors,
the shipped Windows application's observable behaviour, and static analysis of
its service. This repository contains only original code and factual protocol
descriptions. It does not include or redistribute Acer firmware, decompiled
application source, or other proprietary material. Contributions must follow
the same rule.
