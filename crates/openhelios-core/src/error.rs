use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("WMI device {0} is not available; load acer_gaming_probe (see docs/RECOVERY.md)")]
    WmiUnavailable(PathBuf),

    #[error("WMI call failed: {0}")]
    Wmi(String),

    #[error("keyboard HID device 0d62:9a1b is not available")]
    HidUnavailable,

    #[error("HID error: {0}")]
    Hid(String),

    #[error("permission denied for {path}; install packaging/99-openhelios.rules or run as root")]
    Permission { path: PathBuf },

    #[error("safety: {0}")]
    Safety(String),

    #[error("unsupported value: {0}")]
    Unsupported(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("TOML write error: {0}")]
    TomlWrite(#[from] toml::ser::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl From<std::str::Utf8Error> for Error {
    fn from(e: std::str::Utf8Error) -> Self {
        Error::Config(e.to_string())
    }
}
