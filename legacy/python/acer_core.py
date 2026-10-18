"""
acer_core.py - protocol layer for the Acer "gaming function" WMI bridge.

The kernel module `kmod/acer_gaming_probe.c` exposes the ACPI method `WMBH`
(GUID 7A4DDFE7-5B5D-40B4-8595-4408E0CC7F56) as /dev/acer_gaming_wmi:

    write(): byte 0 = method id, bytes 1..N = raw input buffer
    read() : raw WMI response of the last call

This module turns that into a small, exception-raising API used by both the CLI
(acer_gaming.py) and the GUI (acer_gui.py).
"""

import os

DEV = "/dev/acer_gaming_wmi"

# --- method ids (Arg1 of WMBH) ---------------------------------------------
M_SET_GAMING_LED = 2      # u64
M_GET_GAMING_LED = 4
M_GET_SYS_INFO = 5        # u64, byte 0 = selector
M_SET_STATIC_LED = 6      # 4 bytes: [zone, R, G, B]
M_SET_LED_GROUP_BEHAVIOR = 10   # u64: group_id | (on ? on_bit : 0)
M_GET_LED_GROUP_BEHAVIOR = 11
M_SET_LED_GROUP_COLOR = 12      # u64: index | R<<8 | G<<16 | B<<24
M_GET_LED_GROUP_COLOR = 13
M_SET_FAN_BEHAVIOR = 14
M_SET_KBBL = 20           # 16-byte effect struct (also a u64 form, see set_ring_brightness)
M_GET_KBBL = 21
M_SET_MISC = 22
M_GET_MISC = 23

METHOD_NAMES = {
    M_SET_GAMING_LED: "SetGamingLED",
    M_GET_GAMING_LED: "GetGamingLED",
    M_GET_SYS_INFO: "GetGamingSysInfo",
    M_SET_STATIC_LED: "SetGamingStaticLED",
    M_SET_LED_GROUP_BEHAVIOR: "SetGamingLEDGroupBehavior",
    M_GET_LED_GROUP_BEHAVIOR: "GetGamingLEDGroupBehavior",
    M_SET_LED_GROUP_COLOR: "SetGamingLEDGroupColor",
    M_GET_LED_GROUP_COLOR: "GetGamingLEDGroupColor",
    M_SET_FAN_BEHAVIOR: "SetGamingFanBehavior",
    M_SET_KBBL: "SetGamingKBBacklight",
    M_GET_KBBL: "GetGamingKBBacklight",
    M_SET_MISC: "SetGamingMiscSetting",
    M_GET_MISC: "GetGamingMiscSetting",
}

# --- LED groups (method 10) ------------------------------------------------
# Recovered by decompiling PredatorSense 3.0.3196 (PredatorSense.CommonFunction).
# Each entry is (group_id, on_bit_position).  The u64 payload is
#     group_id | (1 << on_bit) when turning the light on, group_id alone for off.
LED_GROUPS = {
    "ring":     (0x80, 30),   # touchpad "mousetrack" ring   - VERIFIED on PH717-71
    "heatpipe": (0x40, 28),   # heat-pipe / HyperDrift
    "gpufan":   (0x40, 28),   # same group as heatpipe
    "cpufan":   (0x20, 26),
    "light1":   (0x100, 40),
    "lightn":   (0x01, 16),
}

# --- LED group colour indices (method 12) ----------------------------------
LED_COLOR_INDEX = {"cpufan": 6, "gpufan": 7, "ring": 8}

EFFECTS = ["static", "breath", "neon", "wave", "shifting", "zoom"]
EFFECT_ID = {name: i for i, name in enumerate(EFFECTS)}
DIRECTIONS = {"right-to-left": 1, "left-to-right": 2}


class AcerLightingError(RuntimeError):
    pass


def device_available() -> bool:
    return os.path.exists(DEV)


def call(method_id: int, payload: bytes = b"") -> bytes:
    """Send [method_id] + payload to the WMI dispatcher, return raw response."""
    if not os.path.exists(DEV):
        raise AcerLightingError(
            f"{DEV} not found - load the driver first:\n"
            f"    sudo insmod kmod/acer_gaming_probe.ko"
        )
    try:
        fd = os.open(DEV, os.O_RDWR)
    except PermissionError as exc:
        raise AcerLightingError(f"permission denied opening {DEV}: {exc}")
    try:
        try:
            os.write(fd, bytes([method_id & 0xFF]) + bytes(payload))
        except OSError as exc:
            raise AcerLightingError(f"WMI call (method {method_id}) failed: {exc}")
        try:
            return os.read(fd, 1024)
        except OSError:
            return b""
    finally:
        os.close(fd)


def u64le(value: int) -> bytes:
    return (int(value) & 0xFFFFFFFFFFFFFFFF).to_bytes(8, "little")


def u64be(data: bytes) -> int:
    return int.from_bytes(data[:8].ljust(8, b"\0"), "little")


# --- high level helpers ----------------------------------------------------

def get_sysinfo(selector: int = 0) -> bytes:
    return call(M_GET_SYS_INFO, u64le(selector))


def get_gaming_led() -> bytes:
    return call(M_GET_GAMING_LED)


def set_gaming_led(value: int) -> bytes:
    return call(M_SET_GAMING_LED, u64le(value))


def get_kbbl() -> bytes:
    return call(M_GET_KBBL)


def kbbl_packet(mode, speed, bright, direction, r, g, b, enable=1) -> bytes:
    """Build the 16-byte keyboard-backlight effect packet.

    [mode, speed, brightness, special, direction, R, G, B, 0, EN, 0,0,0,0,0,0]
    """
    p = bytearray(16)
    p[0] = int(mode) & 0xFF
    p[1] = int(speed) & 0xFF
    p[2] = max(0, min(100, int(bright)))
    p[3] = 0x08 if int(mode) == EFFECT_ID["wave"] else 0x00
    p[4] = int(direction) & 0xFF
    p[5] = int(r) & 0xFF
    p[6] = int(g) & 0xFF
    p[7] = int(b) & 0xFF
    p[9] = 1 if enable else 0
    return bytes(p)


def set_kbbl(mode, speed, bright, direction, r, g, b, enable=1) -> bytes:
    return call(M_SET_KBBL, kbbl_packet(mode, speed, bright, direction, r, g, b, enable))


def set_led_group(group: str, on: bool) -> bytes:
    """Turn an LED group on or off (method 10).

    `group` is a key of LED_GROUPS, e.g. "ring" for the touchpad ring.
    """
    try:
        gid, bit = LED_GROUPS[group]
    except KeyError:
        raise AcerLightingError(
            f"unknown LED group {group!r}; known: {', '.join(sorted(LED_GROUPS))}")
    return call(M_SET_LED_GROUP_BEHAVIOR, u64le(gid | ((1 << bit) if on else 0)))


def set_led_group_color(group: str, r: int, g: int, b: int) -> bytes:
    """Set an LED group's colour (method 12).

    Note: on the PH717-71 the touchpad ring ignores this and stays its native
    blue - it is an on/off light.  Kept because the call is accepted and other
    models in the same PredatorSense code path do honour it.
    """
    try:
        idx = LED_COLOR_INDEX[group]
    except KeyError:
        raise AcerLightingError(
            f"no colour index for {group!r}; known: {', '.join(sorted(LED_COLOR_INDEX))}")
    v = idx | ((r & 0xFF) << 8) | ((g & 0xFF) << 16) | ((b & 0xFF) << 24)
    return call(M_SET_LED_GROUP_COLOR, u64le(v))


def set_ring_brightness(brightness: int) -> bytes:
    """Touchpad-ring brightness - method 20 in its u64 form (brightness << 16).

    This is *not* the 16-byte keyboard effect struct; PredatorSense's
    set_touchpad_brightness() uses this shorter form.
    """
    return call(M_SET_KBBL, u64le((max(0, min(100, int(brightness))) & 0xFF) << 16))


def set_static_zone(zone, r, g, b) -> bytes:
    return call(M_SET_STATIC_LED, bytes([zone & 0xFF, r & 0xFF, g & 0xFF, b & 0xFF]))


# --- formatting ------------------------------------------------------------

def hexs(data: bytes) -> str:
    return " ".join(f"{b:02X}" for b in data) if data else "(empty)"
