use std::fs;
use std::path::{Path, PathBuf};

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            home.join(fallback)
        })
}

pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("predator-sense")
}

pub fn state_dir() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state").join("predator-sense")
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
    let old_cfg = xdg("XDG_CONFIG_HOME", ".config").join("acer-lighting");
    let new_cfg = config_dir();
    if !new_cfg.exists() && old_cfg.exists() {
        let _ = copy_tree(&old_cfg, &new_cfg);
    }

    let old_state = xdg("XDG_STATE_HOME", ".local/state").join("acer-lighting");
    let new_state = state_dir();
    if !new_state.exists() && old_state.exists() {
        let _ = copy_tree(&old_state, &new_state);
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
