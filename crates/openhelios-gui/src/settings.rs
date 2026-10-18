use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use openhelios_core::config;
use serde::{Deserialize, Serialize};

const AUTOSTART_FILE: &str = "openhelios.desktop";

/// Desktop-session integration settings, stored in `~/.config/openhelios/gui.toml`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GuiSettings {
    /// Install `~/.config/autostart/openhelios.desktop` so the GUI starts with the session.
    pub start_with_system: bool,
    /// Launch the autostart entry hidden in the tray.
    pub start_in_tray: bool,
    /// Hide to the tray when the window close button is used.
    pub close_to_tray: bool,
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            start_with_system: true,
            start_in_tray: true,
            close_to_tray: true,
        }
    }
}

impl GuiSettings {
    pub fn path() -> PathBuf {
        config::config_dir().join("gui.toml")
    }

    pub fn autostart_path() -> PathBuf {
        config::config_dir()
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("autostart")
            .join(AUTOSTART_FILE)
    }

    pub fn load() -> Self {
        fs::read_to_string(Self::path())
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> io::Result<()> {
        fs::create_dir_all(config::config_dir())?;
        let text = toml::to_string_pretty(self).map_err(io::Error::other)?;
        fs::write(Self::path(), text)
    }

    /// Create, refresh or remove the XDG autostart entry to match these settings.
    pub fn sync_autostart(&self) -> io::Result<()> {
        let path = Self::autostart_path();
        if !self.start_with_system {
            return match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            };
        }

        let entry = self.autostart_entry();
        if fs::read_to_string(&path).is_ok_and(|current| current == entry) {
            return Ok(());
        }
        fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new(".")))?;
        fs::write(path, entry)
    }

    fn autostart_entry(&self) -> String {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("openhelios-gui"));
        let mut exec = quote_exec(&exe);
        if self.start_in_tray {
            exec.push_str(" --minimized");
        }
        format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Version=1.0\n\
             Name=OpenHelios\n\
             GenericName=Lighting, fan and overclock control\n\
             Comment=Open Linux control for the Acer Predator PH717\n\
             Exec={exec}\n\
             Icon=input-keyboard\n\
             Terminal=false\n\
             X-GNOME-Autostart-enabled=true\n"
        )
    }
}

/// Quote one `Exec` argument as required by the Desktop Entry spec.
fn quote_exec(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let mut escaped = String::with_capacity(raw.len());
    let mut needs_quotes = false;
    for ch in raw.chars() {
        match ch {
            '%' => escaped.push_str("%%"),
            '"' | '`' | '$' | '\\' => {
                needs_quotes = true;
                escaped.push('\\');
                escaped.push(ch);
            }
            ' ' | '\t' | '\n' => {
                needs_quotes = true;
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    if needs_quotes {
        format!("\"{escaped}\"")
    } else {
        escaped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_exec_arguments() {
        assert_eq!(
            quote_exec(Path::new("/usr/local/bin/openhelios-gui")),
            "/usr/local/bin/openhelios-gui"
        );
        assert_eq!(
            quote_exec(Path::new("/opt/Open Helios/gui")),
            "\"/opt/Open Helios/gui\""
        );
        assert_eq!(quote_exec(Path::new("/opt/100%/gui")), "/opt/100%%/gui");
        assert_eq!(
            quote_exec(Path::new("/opt/\"quoted\"/gui")),
            "\"/opt/\\\"quoted\\\"/gui\""
        );
    }

    #[test]
    fn autostart_entry_matches_settings() {
        let settings = GuiSettings::default();
        let entry = settings.autostart_entry();
        assert!(entry.contains("Exec="));
        assert!(entry.contains("--minimized"));
        assert!(entry.contains("X-GNOME-Autostart-enabled=true"));

        let settings = GuiSettings {
            start_in_tray: false,
            ..Default::default()
        };
        assert!(!settings.autostart_entry().contains("--minimized"));
    }
}
