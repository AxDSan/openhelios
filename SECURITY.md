# Security policy

## Supported versions

| version | supported |
|---|---|
| 0.1.x | yes |
| older | no |

## Reporting a vulnerability

Please **do not open a public issue** for a security problem.

Use GitHub's private vulnerability reporting:
[Security Advisories](https://github.com/AxDSan/predator-sense-linux/security/advisories/new).

Include:

- affected version/commit and distribution/kernel;
- the smallest reproduction you can share;
- impact (privilege boundary, memory safety, data exposure, denial of service);
- whether the kernel module or only the userspace tool is involved.

You will get an acknowledgement as soon as possible. If the report is valid,
a fix and advisory are prepared before public disclosure. Please allow a
reasonable disclosure window.

## Scope

In scope:

- the kernel module (`kmod/acer_gaming_probe.c`), including validation of
  userspace input and WMI buffer handling;
- the udev rule, group ownership and device permissions;
- the Rust crates, especially `unsafe` code and HID/WMI input handling;
- privilege boundaries between the GUI/CLI and `/dev/acer_gaming_wmi`,
  `/dev/hidraw*`, RAPL and `nvidia-smi`;
- CI and release supply-chain issues.

Out of scope:

- vulnerabilities in the Linux kernel, `acer-wmi`, systemd, the NVIDIA driver
  or firmware;
- physical access attacks;
- hardware damage caused by overclocking or fan settings (use
  [`docs/RECOVERY.md`](docs/RECOVERY.md));
- reports that require disabling the documented safety gates.

## Trust boundaries and known tradeoffs

- **Dedicated group.** The installer creates `predator-sense` and grants it
  access to the vendor HID interface and the WMI device. This is intentionally
  narrower than the `input` group: it does not grant read access to every
  keyboard. Members can still send arbitrary frames to this one keyboard and
  arbitrary WMI method ids to the EC. Treat group membership as equivalent to
  hardware-control access.
- **Kernel module.** `/dev/acer_gaming_wmi` is a research bridge. It forwards
  `[method_id] + payload` to the WMI method; the udev rule restricts the node to
  the dedicated group. Do not widen the permissions without understanding the
  EC surface.
- **Overclocking.** `--allow-oc` is a deliberate second gate. The tool checks
  AC power and battery level before EC OC writes and snapshots before hardware
  writes, but it cannot protect against an unstable overclock. `panic` and the
  recovery guide are the mitigation.
- **Firmware.** The tool never writes EC/SPI firmware, BIOS or NVRAM. A report
  that claims otherwise should include the exact command and a trace.
- **Root.** RAPL and `nvidia-smi` writes require root. Running the whole GUI as
  root is not recommended; prefer the CLI with `sudo` for those specific
  commands.
- **No secrets.** Please remove serial numbers, MAC addresses, IP addresses and
  any personal data from logs before posting them.

## Hardware safety

Hardware safety issues (a light stuck on, fans at a fixed duty, an unstable OC)
are handled by [`docs/RECOVERY.md`](docs/RECOVERY.md), not by the security
process. If you believe a command can permanently damage hardware, report it
privately with the exact sequence and evidence.

## Safe harbor

We will not pursue legal action or report good-faith security research that
follows this policy, avoids privacy violations and data destruction, and gives
us a reasonable chance to fix the issue before disclosure.
