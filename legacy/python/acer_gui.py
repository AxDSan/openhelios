#!/usr/bin/env python3
"""
acer_gui.py - PyQt5 control panel for the Acer Predator Helios 700 lighting.

Talks to the Acer "gaming function" WMI method through the
/dev/acer_gaming_wmi bridge provided by kmod/acer_gaming_probe.c.

Run:      python3 acer_gui.py
Headless: QT_QPA_PLATFORM=offscreen python3 acer_gui.py --self-test
"""

import sys

from PyQt5.QtCore import Qt, QTimer, pyqtSignal
from PyQt5.QtGui import QColor, QFont
from PyQt5.QtWidgets import (
    QApplication, QMainWindow, QWidget, QTabWidget, QVBoxLayout, QHBoxLayout,
    QFormLayout, QGridLayout, QLabel, QPushButton, QSlider, QSpinBox, QComboBox,
    QCheckBox, QGroupBox, QColorDialog, QTextEdit, QListWidget, QLineEdit,
    QMessageBox, QFrame,
)

import acer_core as core


# --------------------------------------------------------------------------
# small widgets
# --------------------------------------------------------------------------

class ColorButton(QPushButton):
    """A push-button that shows the selected colour and opens a colour picker."""

    colorChanged = pyqtSignal(QColor)

    def __init__(self, color="#ffffff", parent=None):
        super().__init__(parent)
        self._color = QColor(color)
        self.setFixedWidth(120)
        self.setFixedHeight(28)
        self.clicked.connect(self._pick)
        self._refresh()

    def color(self) -> QColor:
        return QColor(self._color)

    def setColor(self, color):
        self._color = QColor(color)
        self._refresh()

    def _refresh(self):
        c = self._color
        # pick readable text colour
        lum = 0.299 * c.red() + 0.587 * c.green() + 0.114 * c.blue()
        fg = "#000000" if lum > 140 else "#ffffff"
        self.setStyleSheet(
            f"background:{c.name()};color:{fg};border:1px solid #555;"
            f"border-radius:4px;font-weight:bold;"
        )
        self.setText(c.name().upper())

    def _pick(self):
        c = QColorDialog.getColor(self._color, self, "Choose colour")
        if c.isValid():
            self._color = c
            self._refresh()
            self.colorChanged.emit(c)


class LabeledSlider(QWidget):
    """Horizontal slider + live value label."""

    valueChanged = pyqtSignal(int)
    released = pyqtSignal(int)

    def __init__(self, lo, hi, value, suffix="", parent=None):
        super().__init__(parent)
        lay = QHBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        self.slider = QSlider(Qt.Horizontal)
        self.slider.setRange(lo, hi)
        self.slider.setValue(value)
        self.label = QLabel(f"{value}{suffix}")
        self.label.setFixedWidth(48)
        self.label.setAlignment(Qt.AlignRight | Qt.AlignVCenter)
        self.suffix = suffix
        lay.addWidget(self.slider, 1)
        lay.addWidget(self.label)

        self.slider.valueChanged.connect(self._on_change)
        self.slider.sliderReleased.connect(
            lambda: self.released.emit(self.slider.value()))
        self.slider.valueChanged.connect(self.valueChanged.emit)

    def _on_change(self, v):
        self.label.setText(f"{v}{self.suffix}")

    def value(self) -> int:
        return self.slider.value()

    def setValue(self, v):
        self.slider.setValue(v)


class ResponseView(QTextEdit):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setReadOnly(True)
        self.setFont(QFont("monospace", 9))
        self.setFixedHeight(70)

    def show_bytes(self, data: bytes, prefix=""):
        if not data:
            self.setPlainText(f"{prefix}(empty response)")
            return
        ascii_part = "".join(chr(b) if 32 <= b < 127 else "." for b in data)
        self.setPlainText(
            f"{prefix}{len(data)} bytes\n"
            f"hex:   {core.hexs(data)}\n"
            f"ascii: {ascii_part}"
        )


# --------------------------------------------------------------------------
# probe sequencer
# --------------------------------------------------------------------------

class Sequencer:
    """Runs a list of (delay_ms, callable) steps on the Qt event loop."""

    def __init__(self, log):
        self._log = log
        self._steps = []
        self._i = 0

    def start(self, steps, title="probe"):
        self._steps = list(steps)
        self._i = 0
        self._log(f"=== {title} ===")
        QTimer.singleShot(0, self._next)

    def _next(self):
        if self._i >= len(self._steps):
            self._log("=== done ===")
            return
        delay, fn = self._steps[self._i]
        self._i += 1
        try:
            fn()
        except core.AcerLightingError as exc:
            self._log(f"  !! {exc}")
        QTimer.singleShot(delay, self._next)


# --------------------------------------------------------------------------
# main window
# --------------------------------------------------------------------------

class MainWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        self.setWindowTitle("Acer Predator Helios 700 — Lighting Control")
        self.resize(760, 620)

        self.kb_color = QColor(0, 255, 255)
        self.zone_colors = [QColor(255, 0, 0), QColor(0, 255, 0),
                            QColor(0, 128, 255), QColor(255, 255, 0)]
        self.seq = Sequencer(self.probe_log)

        tabs = QTabWidget()
        tabs.addTab(self._tab_quick(), "▶ Quick / Ring")
        tabs.addTab(self._tab_keyboard(), "Keyboard")
        tabs.addTab(self._tab_zones(), "Zones")
        tabs.addTab(self._tab_gaming_led(), "Gaming LED")
        tabs.addTab(self._tab_sysinfo(), "System info")
        tabs.addTab(self._tab_probes(), "Probes")
        tabs.addTab(self._tab_raw(), "Raw console")
        self.setCentralWidget(tabs)

        # status bar
        self.status_label = QLabel()
        self.statusBar().addWidget(self.status_label, 1)
        btn_reload = QPushButton("Reload driver")
        btn_reload.clicked.connect(self.reload_driver)
        self.statusBar().addPermanentWidget(btn_reload)
        btn_status = QPushButton("Re-check")
        btn_status.clicked.connect(self.refresh_status)
        self.statusBar().addPermanentWidget(btn_status)
        self.refresh_status()

    # ---------------- status ------------------------------------------------

    def refresh_status(self):
        if core.device_available():
            self.status_label.setText(f"● ready — {core.DEV}")
        else:
            self.status_label.setText(
                f"● driver not loaded — run: sudo insmod kmod/acer_gaming_probe.ko"
            )

    def reload_driver(self):
        import subprocess
        from pathlib import Path
        ko = Path(__file__).resolve().parent / "kmod" / "acer_gaming_probe.ko"
        if not ko.exists():
            QMessageBox.warning(self, "Reload driver",
                                f"{ko} not built.\n\nRun:  make -C kmod")
            return
        try:
            subprocess.run(["sudo", "-n", "rmmod", "acer_gaming_probe"],
                           capture_output=True)
            out = subprocess.run(["sudo", "-n", "insmod", str(ko)],
                                 capture_output=True, text=True)
            if out.returncode != 0:
                QMessageBox.warning(self, "Reload driver",
                                    out.stderr.strip() or "insmod failed")
        except OSError as exc:
            QMessageBox.warning(self, "Reload driver", str(exc))
        QTimer.singleShot(300, self.refresh_status)

    # ---------------- helpers ----------------------------------------------

    def _call(self, fn, view=None):
        try:
            data = fn()
        except core.AcerLightingError as exc:
            QMessageBox.warning(self, "Acer lighting", str(exc))
            return None
        if view is not None:
            view.show_bytes(data)
        return data

    # ---------------- Tab: Quick / Ring ------------------------------------

    def _tab_quick(self):
        w = QWidget()
        lay = QVBoxLayout(w)

        intro = QLabel(
            "<b>Touchpad ring — the “mousetrack ring of light”.</b><br>"
            "This firmware exposes no dedicated ring command, so the lever is one of the "
            "candidates below. Move the slider and watch the ring:"
            "<ul>"
            "<li>ring dims with the keyboard → it shares the keyboard backlight brightness</li>"
            "<li>ring is one of the 4 static zones → use the Zones tab</li>"
            "<li>ring never reacts → it is on the USB HID channel (see Probes tab notes)</li>"
            "</ul>"
        )
        intro.setWordWrap(True)
        lay.addWidget(intro)

        box = QGroupBox("Global dim")
        form = QFormLayout(box)

        self.quick_bright = LabeledSlider(0, 100, 100, "%")
        form.addRow("Brightness", self.quick_bright)

        self.quick_color = ColorButton("#ffffff")
        form.addRow("Colour", self.quick_color)

        self.quick_zones = QCheckBox("Also apply to all 4 static zones")
        self.quick_led = QCheckBox("Also drive the “gaming LED” preset")
        form.addRow("", self.quick_zones)
        form.addRow("", self.quick_led)
        lay.addWidget(box)

        row = QHBoxLayout()
        btn_apply = QPushButton("Apply")
        btn_apply.clicked.connect(self.apply_quick)
        btn_off = QPushButton("All off")
        btn_off.clicked.connect(self.all_off)
        btn_restore = QPushButton("Restore keyboard (static, full)")
        btn_restore.clicked.connect(self.restore_keyboard)
        row.addWidget(btn_apply)
        row.addWidget(btn_off)
        row.addWidget(btn_restore)
        row.addStretch(1)
        lay.addLayout(row)

        lay.addStretch(1)
        return w

    def apply_quick(self):
        bright = self.quick_bright.value()
        c = self.quick_color.color()
        self._call(lambda: core.set_kbbl(core.EFFECT_ID["static"], 0, bright, 1,
                                         c.red(), c.green(), c.blue()))
        if self.quick_zones.isChecked():
            for z in (1, 2, 3, 4):
                self._call(lambda z=z: core.set_static_zone(z, c.red(), c.green(), c.blue()))
        if self.quick_led.isChecked():
            val = 0x08 | (0x0F << 40) if bright > 0 else 0x00
            self._call(lambda: core.set_gaming_led(val))

    def all_off(self):
        self._call(lambda: core.set_kbbl(0, 0, 0, 1, 0, 0, 0, enable=0))
        for z in (1, 2, 3, 4):
            self._call(lambda z=z: core.set_static_zone(z, 0, 0, 0))
        self._call(lambda: core.set_gaming_led(0x00))

    def restore_keyboard(self):
        self._call(lambda: core.set_kbbl(core.EFFECT_ID["static"], 0, 100, 1,
                                         255, 255, 255))

    # ---------------- Tab: Keyboard ----------------------------------------

    def _tab_keyboard(self):
        w = QWidget()
        lay = QVBoxLayout(w)
        box = QGroupBox("Keyboard backlight (SetGamingKBBacklight, method 20)")
        form = QFormLayout(box)

        self.kb_mode = QComboBox()
        self.kb_mode.addItems(core.EFFECTS)
        form.addRow("Effect", self.kb_mode)

        self.kb_bright = LabeledSlider(0, 100, 100, "%")
        form.addRow("Brightness", self.kb_bright)

        self.kb_speed = LabeledSlider(0, 9, 4)
        form.addRow("Speed", self.kb_speed)

        self.kb_dir = QComboBox()
        self.kb_dir.addItems(list(core.DIRECTIONS.keys()))
        form.addRow("Direction", self.kb_dir)

        self.kb_color_btn = ColorButton(self.kb_color.name())
        form.addRow("Colour", self.kb_color_btn)
        lay.addWidget(box)

        row = QHBoxLayout()
        b_apply = QPushButton("Apply effect")
        b_apply.clicked.connect(self.apply_keyboard)
        b_read = QPushButton("Read state")
        b_read.clicked.connect(self.read_keyboard)
        row.addWidget(b_apply)
        row.addWidget(b_read)
        row.addStretch(1)
        lay.addLayout(row)

        self.kb_resp = ResponseView()
        lay.addWidget(self.kb_resp)
        lay.addStretch(1)

        # live brightness
        self.kb_bright.released.connect(lambda _: self.apply_keyboard())
        self.kb_color_btn.colorChanged.connect(lambda _: self.apply_keyboard())
        return w

    def apply_keyboard(self):
        mode = core.EFFECT_ID[self.kb_mode.currentText()]
        c = self.kb_color_btn.color()
        self._call(lambda: core.set_kbbl(
            mode, self.kb_speed.value(), self.kb_bright.value(),
            core.DIRECTIONS[self.kb_dir.currentText()],
            c.red(), c.green(), c.blue()), self.kb_resp)

    def read_keyboard(self):
        data = self._call(core.get_kbbl, self.kb_resp)
        if data:
            self.kb_resp.append(
                f"effect id (byte 0) = {data[0]}"
                f"  ({core.EFFECTS[data[0]] if data[0] < len(core.EFFECTS) else '?'})"
            )

    # ---------------- Tab: Zones -------------------------------------------

    def _tab_zones(self):
        w = QWidget()
        lay = QVBoxLayout(w)
        lay.addWidget(QLabel(
            "Static per-zone colour (SetGamingStaticLED, method 6): payload "
            "<code>[zone, R, G, B]</code>, zone = 1..4."
        ))
        box = QGroupBox("Zones")
        grid = QGridLayout(box)
        self.zone_buttons = []
        self.zone_enabled = []
        for i in range(4):
            chk = QCheckBox(f"Zone {i + 1}")
            chk.setChecked(True)
            btn = ColorButton(self.zone_colors[i].name())
            self.zone_enabled.append(chk)
            self.zone_buttons.append(btn)
            grid.addWidget(chk, i, 0)
            grid.addWidget(btn, i, 1)
        lay.addWidget(box)

        row = QHBoxLayout()
        b_apply = QPushButton("Apply zones")
        b_apply.clicked.connect(self.apply_zones)
        b_off = QPushButton("Zones off")
        b_off.clicked.connect(self.zones_off)
        row.addWidget(b_apply)
        row.addWidget(b_off)
        row.addStretch(1)
        lay.addLayout(row)

        self.zone_resp = ResponseView()
        lay.addWidget(self.zone_resp)
        lay.addStretch(1)
        return w

    def apply_zones(self):
        for z in range(4):
            if not self.zone_enabled[z].isChecked():
                continue
            c = self.zone_buttons[z].color()
            self._call(lambda z=z, c=c: core.set_static_zone(z + 1, c.red(), c.green(), c.blue()),
                       self.zone_resp)

    def zones_off(self):
        for z in (1, 2, 3, 4):
            self._call(lambda z=z: core.set_static_zone(z, 0, 0, 0), self.zone_resp)

    # ---------------- Tab: Gaming LED --------------------------------------

    LED_PRESETS = {
        "Off": 0x0000000000000000,
        "Enable (0x08)": 0x0000000000000008,
        "Enable + all zones (0x0F<<40)": 0x00000F0000000008,
        "Enable + zones 1-2 (0x03<<40)": 0x0000030000000008,
    }

    def _tab_gaming_led(self):
        w = QWidget()
        lay = QVBoxLayout(w)
        lay.addWidget(QLabel(
            "SetGamingLED (method 2) takes a packed <code>u64</code>. Observed layout: "
            "<code>byte0 = enable (0x08)</code>, <code>byte5 = zone mask</code>."
        ))
        box = QGroupBox("Gaming LED")
        form = QFormLayout(box)

        self.led_preset = QComboBox()
        self.led_preset.addItems(list(self.LED_PRESETS.keys()))
        form.addRow("Preset", self.led_preset)

        self.led_hex = QLineEdit("0x00000F0000000008")
        form.addRow("Raw u64", self.led_hex)
        lay.addWidget(box)

        row = QHBoxLayout()
        b_apply = QPushButton("Set")
        b_apply.clicked.connect(self.apply_led)
        b_read = QPushButton("GetGamingLED")
        b_read.clicked.connect(self.read_led)
        row.addWidget(b_apply)
        row.addWidget(b_read)
        row.addStretch(1)
        lay.addLayout(row)

        self.led_resp = ResponseView()
        lay.addWidget(self.led_resp)
        lay.addStretch(1)

        self.led_preset.currentTextChanged.connect(
            lambda t: self.led_hex.setText(f"0x{self.LED_PRESETS[t]:016X}"))
        return w

    def apply_led(self):
        try:
            val = int(self.led_hex.text(), 0)
        except ValueError:
            QMessageBox.warning(self, "Gaming LED", "Raw u64 must be a number, e.g. 0x00000F0000000008")
            return
        self._call(lambda: core.set_gaming_led(val), self.led_resp)

    def read_led(self):
        data = self._call(core.get_gaming_led, self.led_resp)
        if data:
            self.led_resp.append(f"value = 0x{core.u64be(data):016X}")

    # ---------------- Tab: System info -------------------------------------

    def _tab_sysinfo(self):
        w = QWidget()
        lay = QVBoxLayout(w)
        lay.addWidget(QLabel("GetGamingSysInfo (method 5). Selector is byte 0 of a u64."))

        self.sys_views = {}
        for sel in (0, 1, 2):
            box = QGroupBox(f"selector {sel}")
            v = QVBoxLayout(box)
            view = ResponseView()
            self.sys_views[sel] = view
            v.addWidget(view)
            lay.addWidget(box)

        row = QHBoxLayout()
        b = QPushButton("Refresh all")
        b.clicked.connect(self.read_sysinfo)
        row.addWidget(b)
        row.addStretch(1)
        lay.addLayout(row)
        lay.addStretch(1)
        QTimer.singleShot(400, self.read_sysinfo)
        return w

    def read_sysinfo(self):
        for sel, view in self.sys_views.items():
            self._call(lambda sel=sel: core.get_sysinfo(sel), view)

    # ---------------- Tab: Probes ------------------------------------------

    def _tab_probes(self):
        w = QWidget()
        lay = QVBoxLayout(w)
        note = QLabel(
            "Run these while watching the laptop. The log records what was sent."
        )
        note.setWordWrap(True)
        lay.addWidget(note)

        row = QHBoxLayout()
        b1 = QPushButton("Probe static zones")
        b1.clicked.connect(self.probe_static_zones)
        b2 = QPushButton("Probe gaming LED")
        b2.clicked.connect(self.probe_gaming_led)
        b3 = QPushButton("Probe brightness sweep")
        b3.clicked.connect(self.probe_brightness)
        for b in (b1, b2, b3):
            row.addWidget(b)
        row.addStretch(1)
        lay.addLayout(row)

        self.probe_log_view = QTextEdit()
        self.probe_log_view.setReadOnly(True)
        self.probe_log_view.setFont(QFont("monospace", 9))
        lay.addWidget(self.probe_log_view, 1)
        return w

    def probe_log(self, text):
        self.probe_log_view.append(text)

    def probe_static_zones(self):
        steps = []
        for z in (1, 2, 3, 4):
            steps.append((0, lambda z=z: (
                self.probe_log(f">>> zone {z} = white"),
                self._call(lambda z=z: core.set_static_zone(z, 255, 255, 255)))))
            steps.append((2500, lambda z=z: (
                self.probe_log(f"<<< zone {z} = off"),
                self._call(lambda z=z: core.set_static_zone(z, 0, 0, 0)))))
        self.seq.start(steps, "static zone cycle")

    def probe_gaming_led(self):
        vals = [0x00, 0x01, 0x02, 0x04, 0x08, 0x00000F0000000008]
        steps = []
        for v in vals:
            steps.append((0, lambda v=v: (
                self.probe_log(f">>> SetGamingLED 0x{v:016X}"),
                self._call(lambda v=v: core.set_gaming_led(v)))))
            steps.append((1800, lambda: None))
        self.seq.start(steps, "gaming LED sweep")

    def probe_brightness(self):
        steps = []
        for b in (100, 60, 30, 10, 0):
            steps.append((0, lambda b=b: (
                self.probe_log(f">>> keyboard brightness {b}%"),
                self._call(lambda b=b: core.set_kbbl(core.EFFECT_ID["static"], 0, b, 1,
                                                     255, 255, 255)))))
            steps.append((2000, lambda: None))
        self.seq.start(steps, "brightness sweep")

    # ---------------- Tab: Raw console -------------------------------------

    def _tab_raw(self):
        w = QWidget()
        lay = QVBoxLayout(w)
        box = QGroupBox("Send raw WMBH(method_id, payload)")
        form = QFormLayout(box)

        self.raw_method = QSpinBox()
        self.raw_method.setRange(0, 255)
        self.raw_method.setValue(5)
        form.addRow("Method id", self.raw_method)

        self.raw_payload = QLineEdit()
        self.raw_payload.setPlaceholderText("hex bytes, e.g.  00 00 00 00 00 00 00 00")
        form.addRow("Payload", self.raw_payload)
        lay.addWidget(box)

        row = QHBoxLayout()
        b_send = QPushButton("Send")
        b_send.clicked.connect(self.raw_send)
        row.addWidget(b_send)
        row.addStretch(1)
        lay.addLayout(row)

        self.raw_resp = ResponseView()
        lay.addWidget(self.raw_resp)

        lay.addWidget(QLabel("History"))
        self.raw_history = QListWidget()
        self.raw_history.itemDoubleClicked.connect(self.raw_reuse)
        lay.addWidget(self.raw_history, 1)

        self.raw_method.valueChanged.connect(self._raw_named)
        self._raw_named()
        return w

    def _raw_named(self):
        m = self.raw_method.value()
        self.raw_method.setToolTip(core.METHOD_NAMES.get(m, "unknown"))

    @staticmethod
    def _parse_hex(text: str) -> bytes:
        out = bytearray()
        for tok in text.replace(",", " ").split():
            tok = tok[2:] if tok.lower().startswith("0x") else tok
            if len(tok) % 2:
                tok = "0" + tok
            out += bytes.fromhex(tok)
        return bytes(out)

    def raw_send(self):
        m = self.raw_method.value()
        text = self.raw_payload.text()
        try:
            payload = self._parse_hex(text) if text.strip() else b""
        except ValueError:
            QMessageBox.warning(self, "Raw console", "Payload must be hex bytes")
            return
        try:
            data = core.call(m, payload)
        except core.AcerLightingError as exc:
            QMessageBox.warning(self, "Raw console", str(exc))
            return
        name = core.METHOD_NAMES.get(m, "?")
        self.raw_resp.show_bytes(data, prefix=f"method {m} ({name})\n")
        self.raw_history.insertItem(
            0, f"{m:>3}  {core.hexs(payload)}  ->  {core.hexs(data)}")

    def raw_reuse(self, item):
        parts = item.text().split("  ")
        if parts and parts[0].strip().isdigit():
            self.raw_method.setValue(int(parts[0]))
        if len(parts) >= 2:
            self.raw_payload.setText(parts[1] if parts[1] != "(empty)" else "")


# --------------------------------------------------------------------------

def main():
    app = QApplication(sys.argv)
    app.setApplicationName("Acer Lighting Control")
    win = MainWindow()

    if "--self-test" in sys.argv:
        # build everything and quit - used for headless smoke tests
        QTimer.singleShot(1500, app.quit)
        win.show()
        return app.exec_()

    win.show()
    return app.exec_()


if __name__ == "__main__":
    sys.exit(main())
