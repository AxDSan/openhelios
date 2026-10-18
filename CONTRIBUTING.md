# Contributing

Thanks for helping make Predator Helios hardware work on Linux with OpenHelios.

## Ground rules

- Be respectful. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
- This is an **unofficial** project. Do not claim affiliation with Acer.
- **Provenance:** do not contribute Acer firmware, decompiled application
  source, vendor binaries, extracted assets, `.xtu` files, or other proprietary
  material. Protocol facts, observations and original code are welcome. If a
  change depends on new hardware behaviour, describe the observation and add
  it to [`docs/PROTOCOL.md`](docs/PROTOCOL.md). Disclose whether you inspected
  a vendor binary; do not paste its code or assets. This project is not a
  certified clean room and does not claim to be one. See
  [`docs/PROVENANCE.md`](docs/PROVENANCE.md).
- **License:** contributions are accepted under GPL-2.0-or-later. Sign off every
  commit with the Developer Certificate of Origin:

  ```sh
  git commit -s
  ```

  The sign-off certifies you wrote the change or have the right to submit it.
- **Security:** do not open a public issue for a vulnerability. Use
  [SECURITY.md](SECURITY.md).

## Development setup

```sh
git clone https://github.com/AxDSan/openhelios.git
cd openhelios
cargo build
cargo test
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

Hardware access needs the kernel module and the udev rule:

```sh
RELEASE=0 ./packaging/install.sh   # fast debug install
openhelios doctor
```

If you only need the module:

```sh
cd kmod && make && sudo insmod acer_gaming_probe.ko
```

## Style

- Rust: `rustfmt` defaults; 4 spaces; no `unsafe` unless a syscall/ioctl
  requires it, with a comment explaining the invariant.
- Errors: return `Result`; no `unwrap()`/`expect()` on hardware paths. User
  errors should explain the next action.
- Protocol code: keep wire values in `openhelios-core/src/protocol.rs`; do not
  duplicate them in the GUI or CLI.
- Safety: every hardware write must be gated by `SafetyPolicy`; direct HID
  writes in the GUI/CLI must snapshot first.
- Docs: update `README.md`, `docs/PROTOCOL.md`, `docs/PARITY.md` or
  `docs/RECOVERY.md` when behaviour changes.

## Hardware testing

Before a PR that touches hardware:

1. Run `openhelios snapshot`.
2. Use AC power. Do not test overclock writes on battery.
3. Prefer `--dry-run` first.
4. Verify the physical result (lights/fans) and the getter/readback.
5. End with `openhelios panic` or `openhelios profile apply default`.
6. Attach the relevant `openhelios doctor` and `openhelios monitor`
   output, with serial numbers/MAC addresses removed.

Do not add speculative writes for an unknown method or payload. If a value is
accepted but visibly inert, record that in `docs/PARITY.md` rather than
pretending it works.

## Commit style

Use Conventional Commits with an imperative subject:

```
feat(lighting): add per-key drag selection
fix(hid): preserve checksum when the command is retried
docs(protocol): document health index 12
```

Keep commits focused. One topic per PR. Include the hardware tested in the PR
description.

## Pull request checklist

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Hardware tested (or explicitly marked not hardware-tested)
- [ ] `openhelios doctor` output attached for hardware changes
- [ ] `docs/PROTOCOL.md`/`PARITY.md` updated if the protocol changed
- [ ] No proprietary code, firmware or decompiled source
- [ ] Commits signed off (`git commit -s`)

## Adding a new model

Please include:

- model and board name (`/sys/class/dmi/id/product_name`, `board_name`);
- BIOS version and kernel;
- `openhelios doctor` output;
- `lsusb` lines for the vendor HID device(s);
- what works and what is inert (lighting groups, fans, OC, MagForce);
- whether the model uses the `0d62:9a1b` HID family or another controller.

Open a hardware report issue before a large PR so the protocol can be compared.

## Release process

1. Update `CHANGELOG.md` and the version in `Cargo.toml`.
2. Run the full check set and a release build.
3. Commit, sign off, tag `vX.Y.Z`, and push the tag.
4. Create a GitHub release with the changelog and verified-hardware notes.
