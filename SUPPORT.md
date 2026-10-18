# Support

## Before opening an issue

1. Read [`README.md`](README.md), [`docs/PROTOCOL.md`](docs/PROTOCOL.md),
   [`docs/PARITY.md`](docs/PARITY.md) and [`docs/RECOVERY.md`](docs/RECOVERY.md).
2. Search existing issues.
3. Run:

   ```sh
   openhelios doctor
   openhelios monitor
   ```

4. For a lighting/fan problem, run the failing command with `--dry-run` first
   and include the log.

## What to include

- laptop model, board name, BIOS version;
- distribution and kernel (`uname -r`);
- desktop/session (X11 or Wayland) and whether the GUI or CLI was used;
- exact command and its output;
- `openhelios doctor` output;
- what changed physically (which light/fan, expected vs actual).

Remove serial numbers, MAC addresses, IP addresses and any personal data.

## Where to ask

- **Bug** — use the bug report template.
- **Hardware report** — use the hardware report template; this is the fastest
  way to add a new model.
- **Feature** — use the feature request template.
- **Security** — follow [`SECURITY.md`](SECURITY.md); do not open a public
  issue.

## Scope

The project supports the hardware-control surface (lighting, fans, sensors,
MagForce, overclocking). Windows-only modules (App Center, GameSync, sound
profiles, BIOS logo, mobile pairing) are out of scope unless they expose a
documented hardware interface. See [`docs/PARITY.md`](docs/PARITY.md).

Hardware damage and recovery are covered by
[`docs/RECOVERY.md`](docs/RECOVERY.md). The maintainers cannot guarantee that
an overclock is safe for your silicon.
