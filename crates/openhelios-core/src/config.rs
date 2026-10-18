use std::fs;
use std::path::{Path, PathBuf};

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| home_dir().join(fallback))
}

/// Home of the invoking user.
///
/// `sudo openhelios ...` otherwise writes profiles and snapshots under
/// `/root`, which is why CLI changes appeared not to persist. Resolve
/// `SUDO_USER` through `/etc/passwd` and fall back to `HOME`.
fn home_dir() -> PathBuf {
    if let Some(user) = std::env::var_os("SUDO_USER") {
        let user = user.to_string_lossy();
        if !user.is_empty() && user != "root" {
            if let Some(home) = passwd_home(&user) {
                return home;
            }
        }
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn passwd_home(user: &str) -> Option<PathBuf> {
    let text = fs::read_to_string("/etc/passwd").ok()?;
    for line in text.lines() {
        let mut fields = line.split(':');
        if fields.next() != Some(user) {
            continue;
        }
        let _password = fields.next()?;
        let _uid = fields.next()?;
        let _gid = fields.next()?;
        let _gecos = fields.next()?;
        return fields.next().map(PathBuf::from);
    }
    None
}

pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("openhelios")
}

pub fn state_dir() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state").join("openhelios")
}

pub fn profiles_dir() -> PathBuf {
    migrate_legacy();
    config_dir().join("profiles")
}

pub fn snapshots_dir() -> PathBuf {
    migrate_legacy();
    state_dir().join("snapshots")
}

pub fn ensure_dirs() -> std::io::Result<()> {
    fs::create_dir_all(profiles_dir())?;
    fs::create_dir_all(snapshots_dir())?;
    Ok(())
}

/// Copy the pre-rename `acer-lighting` profiles/snapshots once.
///
/// The first Rust build used `~/.config/acer-lighting` and
/// `~/.local/state/acer-lighting`. Nothing is deleted; the old directories are
/// left untouched so a failed migration can be inspected or retried.
pub fn migrate_legacy() {
    let new_cfg = config_dir();
    if !new_cfg.exists() {
        for old in ["predator-sense", "acer-lighting"] {
            let old_cfg = xdg("XDG_CONFIG_HOME", ".config").join(old);
            if old_cfg.exists() {
                let _ = copy_tree(&old_cfg, &new_cfg);
                break;
            }
        }
    }

    let new_state = state_dir();
    if !new_state.exists() {
        for old in ["predator-sense", "acer-lighting"] {
            let old_state = xdg("XDG_STATE_HOME", ".local/state").join(old);
            if old_state.exists() {
                let _ = copy_tree(&old_state, &new_state);
                break;
            }
        }
    }
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
