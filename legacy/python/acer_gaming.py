#!/usr/bin/env python3
"""
acer_gaming.py - CLI for the Acer "gaming function" WMI method on Predator/Nitro
laptops, via the acer_gaming_probe kernel module (/dev/acer_gaming_wmi).

See README.md for the full reverse-engineering background.  The GUI equivalent
is acer_gui.py.

Usage
-----
  acer_gaming.py raw <method_id> [hex-byte ...]
  acer_gaming.py sysinfo [selector]
  acer_gaming.py getled
  acer_gaming.py led <u64>
  acer_gaming.py kbbl-set --mode M --speed S --bright B --dir D [--r R --g G --b B]
  acer_gaming.py kbbl-get
  acer_gaming.py static <zone> <r> <g> <b>
  acer_gaming.py probe-static [--zone N]
  acer_gaming.py probe-brightness
  acer_gaming.py probe-led
"""

import argparse
import sys
import time

import acer_core as core


def parse_hex_args(items):
    out = bytearray()
    for it in items:
        it = it.replace("0x", "").replace(",", "")
        if len(it) % 2:
            it = "0" + it
        out += bytes.fromhex(it)
    return bytes(out)


def print_resp(data: bytes) -> None:
    print(f"  response ({len(data)} bytes): {core.hexs(data)}")


def guarded(fn, *a, **kw):
    try:
        return fn(*a, **kw)
    except core.AcerLightingError as exc:
        sys.exit(str(exc))


# --------------------------------------------------------------------------

def cmd_raw(a):
    payload = parse_hex_args(a.bytes) if a.bytes else b""
    print(f"method {a.method_id} "
          f"({core.METHOD_NAMES.get(a.method_id, '?')}) payload: {core.hexs(payload)}")
    print_resp(guarded(core.call, a.method_id, payload))


def cmd_sysinfo(a):
    print(f"GetGamingSysInfo selector={a.selector}")
    print_resp(guarded(core.get_sysinfo, a.selector))


def cmd_getled(a):
    print("GetGamingLED")
    print_resp(guarded(core.get_gaming_led))


def cmd_led(a):
    val = int(a.value, 0)
    print(f"SetGamingLED u64=0x{val:016X}")
    print_resp(guarded(core.set_gaming_led, val))


def cmd_ring(a):
    on = a.state == "on"
    print(f"Touchpad ring -> {a.state} "
          f"(SetGamingLEDGroupBehavior, group 0x80, bit 30)")
    print_resp(guarded(core.set_led_group, "ring", on))
    if a.bright is not None:
        print(f"  ring brightness -> {a.bright}")
        print_resp(guarded(core.set_ring_brightness, a.bright))


def cmd_heatpipe(a):
    on = a.state == "on"
    print(f"Heat-pipe lighting -> {a.state} "
          f"(SetGamingLEDGroupBehavior, group 0x40, bit 28)")
    print_resp(guarded(core.set_led_group, "heatpipe", on))


def cmd_ledgroup(a):
    print(f"SetGamingLEDGroupBehavior group={a.group} state={a.state}")
    print_resp(guarded(core.set_led_group, a.group, a.state == "on"))


def cmd_ledgroup_color(a):
    print(f"SetGamingLEDGroupColor group={a.group} rgb=({a.r},{a.g},{a.b})")
    print_resp(guarded(core.set_led_group_color, a.group, a.r, a.g, a.b))


def cmd_kbbl_set(a):
    mode = core.EFFECT_ID[a.mode] if a.mode in core.EFFECT_ID else int(a.mode)
    print("SetGamingKBBacklight -> "
          f"{core.hexs(core.kbbl_packet(mode, a.speed, a.bright, a.direction, a.r, a.g, a.b))}")
    print_resp(guarded(core.set_kbbl, mode, a.speed, a.bright, a.direction,
                       a.r, a.g, a.b))


def cmd_kbbl_get(a):
    print("GetGamingKBBacklight")
    data = guarded(core.get_kbbl)
    print_resp(data)
    if data:
        eid = data[0]
        print(f"  effect id = {eid} "
              f"({core.EFFECTS[eid] if eid < len(core.EFFECTS) else '?'})")


def cmd_static(a):
    print(f"SetGamingStaticLED zone={a.zone} rgb=({a.r},{a.g},{a.b})")
    print_resp(guarded(core.set_static_zone, a.zone, a.r, a.g, a.b))


def cmd_probe_static(a):
    zones = [a.zone] if a.zone else [1, 2, 3, 4]
    print("Cycling static-LED zones with bright white. WATCH THE LAPTOP.")
    for z in zones:
        print(f"\n>>> zone {z} = white")
        print_resp(guarded(core.set_static_zone, z, 255, 255, 255))
        time.sleep(3)
        print(f"<<< zone {z} = off")
        print_resp(guarded(core.set_static_zone, z, 0, 0, 0))
        time.sleep(1)


def cmd_probe_brightness(a):
    print("Running brightness sweep. WATCH THE LAPTOP.\n")
    for b in (100, 60, 30, 10, 0):
        print(f">>> keyboard brightness {b}%")
        print_resp(guarded(core.set_kbbl, core.EFFECT_ID["static"], 0, b, 1,
                           255, 255, 255))
        time.sleep(2)
    print("\nRestoring 100%.")
    guarded(core.set_kbbl, core.EFFECT_ID["static"], 0, 100, 1, 255, 255, 255)


def cmd_probe_led(a):
    print("Sweeping SetGamingLED values. WATCH THE LAPTOP.")
    for val in (0x00, 0x01, 0x02, 0x04, 0x08, 0x00000F0000000008):
        print(f"\n>>> SetGamingLED 0x{val:016X}")
        print_resp(guarded(core.set_gaming_led, val))
        time.sleep(2)


# --------------------------------------------------------------------------

def build_parser():
    p = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    s = sub.add_parser("raw", help="send an arbitrary method id + payload")
    s.add_argument("method_id", type=lambda x: int(x, 0))
    s.add_argument("bytes", nargs="*", help="payload bytes, e.g. 41 42 ff")
    s.set_defaults(func=cmd_raw)

    s = sub.add_parser("sysinfo", help="GetGamingSysInfo (method 5)")
    s.add_argument("selector", type=lambda x: int(x, 0), nargs="?", default=0)
    s.set_defaults(func=cmd_sysinfo)

    s = sub.add_parser("getled", help="GetGamingLED (method 4)")
    s.set_defaults(func=cmd_getled)

    s = sub.add_parser("led", help="SetGamingLED (method 2), a u64")
    s.add_argument("value", help="u64 value, e.g. 0x00000F0000000008")
    s.set_defaults(func=cmd_led)

    s = sub.add_parser("kbbl-set", help="SetGamingKBBacklight (method 20)")
    s.add_argument("--mode", default="static", help="static|breath|neon|wave|shifting|zoom")
    s.add_argument("--speed", type=int, default=0)
    s.add_argument("--bright", type=int, default=100)
    s.add_argument("--dir", "--direction", dest="direction", type=int, default=1)
    s.add_argument("--r", type=int, default=255)
    s.add_argument("--g", type=int, default=255)
    s.add_argument("--b", type=int, default=255)
    s.set_defaults(func=cmd_kbbl_set)

    s = sub.add_parser("kbbl-get", help="GetGamingKBBacklight (method 21)")
    s.set_defaults(func=cmd_kbbl_get)

    s = sub.add_parser("static", help="SetGamingStaticLED (method 6)")
    s.add_argument("zone", type=int)
    s.add_argument("r", type=int)
    s.add_argument("g", type=int)
    s.add_argument("b", type=int)
    s.set_defaults(func=cmd_static)

    s = sub.add_parser("ring", help="touchpad ring on/off (method 10) - VERIFIED")
    s.add_argument("state", choices=("on", "off"))
    s.add_argument("--bright", type=int, default=None,
                   help="also set ring brightness 0-100 (method 20, u64 form)")
    s.set_defaults(func=cmd_ring)

    s = sub.add_parser("heatpipe", help="heat-pipe lighting on/off (method 10)")
    s.add_argument("state", choices=("on", "off"))
    s.set_defaults(func=cmd_heatpipe)

    s = sub.add_parser("ledgroup", help="any LED group on/off (method 10)")
    s.add_argument("group", choices=sorted(core.LED_GROUPS))
    s.add_argument("state", choices=("on", "off"))
    s.set_defaults(func=cmd_ledgroup)

    s = sub.add_parser("ledgroup-color", help="LED group colour (method 12)")
    s.add_argument("group", choices=sorted(core.LED_COLOR_INDEX))
    s.add_argument("r", type=int)
    s.add_argument("g", type=int)
    s.add_argument("b", type=int)
    s.set_defaults(func=cmd_ledgroup_color)

    s = sub.add_parser("probe-static", help="cycle static zones so you can watch")
    s.add_argument("--zone", type=int, default=0)
    s.set_defaults(func=cmd_probe_static)

    s = sub.add_parser("probe-brightness", help="keyboard brightness sweep")
    s.set_defaults(func=cmd_probe_brightness)

    s = sub.add_parser("probe-led", help="sweep SetGamingLED values")
    s.set_defaults(func=cmd_probe_led)

    return p


if __name__ == "__main__":
    args = build_parser().parse_args()
    args.func(args)
