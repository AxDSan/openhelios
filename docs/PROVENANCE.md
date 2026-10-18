# Research provenance and legal notes

OpenHelios is an independent Linux implementation for the Acer Predator
Helios 700 (PH717-71, board `Levante_CFS`). It is not affiliated with, endorsed
by, or supported by Acer. Acer, Predator, Helios, PredatorSense and related
marks belong to their respective owners.

This document records the research provenance behind the protocol constants and
the limits of what the project claims. It is **not legal advice**.

## What was examined

The implementation was derived from functional interface evidence:

- public ACPI tables on the target laptop (`_WDG`, the `AcerGamingFunction`
  device, and the `WMBH` method in SSDT7);
- USB HID report descriptors and black-box feature/output reports from the
  vendor keyboard (`0d62:9a1b`);
- WMI method and selector probing, with snapshots and reversible calls;
- the publicly distributed **PredatorSense Microsoft Store package** installed
  on the target machine. That package is a Desktop Bridge **.NET** application;
  its managed metadata and assemblies were inspected with standard tooling to
  confirm method IDs, selector semantics, payload shapes and UI-visible
  behaviour.

No firmware image, EC/SPI image, BIOS or NVRAM contents were extracted or
written. No vendor source tree was obtained for redistribution or copied into
the repository. Decompiler output was used as analysis notes and is not
committed. No DRM,
authentication, licence check or encryption was bypassed to inspect the managed
application.

Acer's encrypted `.xtu` profiles were **not decrypted and are not used**. The
project reproduces the same class of CPU/GPU limits through public interfaces:
RAPL/powercap for PL1/PL2, sysfs `intel_pstate` for governor/EPP/frequency, and
the NVIDIA driver (`nvidia-smi`) for power limits and locked clocks.

## What is in this repository

The repository and releases contain:

- original Rust source for the core, CLI and GUI;
- the original GPL kernel module `acer_gaming_probe`;
- factual protocol notes;
- screenshots of OpenHelios itself;
- original legacy Python tooling under `legacy/`.

They do **not** contain vendor binaries, decompiled vendor source, extracted
icons, fonts, images, firmware, `.appxbundle`/`.appx` files, `.xtu` profiles or
other proprietary material. A local, git-ignored `work/` scratch directory may
hold the downloaded package during analysis; it is never committed or
released. The keyboard map is a functional key-position map
used for interoperability; it is not a copied expressive layout asset.

## Access, agreements and copying

| Question | This project |
|---|---|
| Was the Windows software lawfully obtained? | The analysis used the publicly distributed Microsoft Store package installed on the target machine. The package is not redistributed by this project. |
| Were terms accepted that restrict reverse engineering? | The project makes no claim that vendor terms permit or prohibit the analysis. It does not rely on a vendor licence to redistribute anything, and it distributes no vendor material. Users and contributors must assess their own contractual obligations. |
| Was encryption, authentication or another access control bypassed? | No. The application is a Desktop Bridge .NET package; managed metadata was inspected with standard tooling. Encrypted `.xtu` profiles were not decrypted or used. |
| Did proprietary code or expressive assets enter the repository or releases? | No. Only original code, factual interface notes and screenshots of OpenHelios are tracked. See the check below. |

This is a factual account, not a legal conclusion. Reverse-engineering
permissions differ by jurisdiction and by the terms attached to a particular
copy. If you need legal advice, consult a lawyer qualified in your
jurisdiction.

## Clean-room accuracy

**This is not a certified clean room.** The same maintainer performed the
interface analysis and wrote the implementation. The project does not claim the
kind of separation where one team writes interface documentation and a
separate team implements without seeing the original material.

What is claimed is narrower and verifiable:

- the repository contains no vendor code or expressive assets;
- the Rust implementation is original;
- protocol constants and payloads are functional interface facts documented in
  [`PROTOCOL.md`](PROTOCOL.md);
- contributors must not submit decompiled source, firmware, vendor binaries or
  copied assets.

If a formal clean-room audit is required for a contribution or distribution,
this repository does not provide one.

## Repository check

The following should return no vendor binaries or decompiled source:

```sh
git ls-files | grep -Ei '\.(exe|dll|appx|appxbundle|msi|xtu|cs)$'
```

The check was also run across all git history:

```sh
git log --all --name-only --pretty=format: | \
  sort -u | grep -Ei '\.(exe|dll|cs|appx|appxbundle|msi|xtu)$'
```

Both returned no vendor artifacts. The `legacy/` Python files are original
code from the earlier prototype.

## Reporting a provenance problem

If you believe material in the repository or a release is misattributed or
should not be present, open a private security advisory or contact the
maintainer through the repository. Do not add vendor material to an issue.
