//! Linux: puts Holt in the app menu on the first run of a package.
//!
//! A source build already does this while compiling (`crates/app/build.rs`) and
//! `scripts/install-desktop.sh` does it by hand. A package extracted from the release tarball has no build step,
//! so the binary itself writes `~/.local/share/applications/holt.desktop` (pointing at itself) and the icon the
//! first time it runs. The entry's name is the window's app id (`holt`), which is how Wayland desktops find the
//! icon and the name. Nothing outside `~/.local/share` is touched, and `HOLT_NO_DESKTOP_ENTRY` disables it.

#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};

/// The `.desktop` template: `@EXEC@` becomes the binary's path.
const DESKTOP: &str = include_str!("../assets/holt.desktop");
/// The menu icon (256 px, the size the template declares).
const ICON: &[u8] = include_bytes!("../assets/holt.png");

/// Writes the menu entry and the icon when this binary came from a package. Never fails the app: without a
/// menu entry Holt still works, so errors only go to the log.
pub fn install_if_packaged() {
    let Ok(exe) = std::env::current_exe() else { return };
    let Some(base) = data_dir() else { return };
    if std::env::var_os("HOLT_NO_DESKTOP_ENTRY").is_some_and(|v| !v.is_empty()) {
        return;
    }
    let desktop = base.join("applications").join("holt.desktop");
    let icon = base.join("icons").join("hicolor").join("256x256").join("apps").join("holt.png");
    // A Cargo build has `build.rs` for this; a package does not (and the CI build also skips it).
    if in_cargo_target(&exe) {
        return;
    }
    if is_current(&desktop, &icon, &exe) {
        return;
    }
    match write_entry(&desktop, &icon, &exe) {
        Ok(()) => log::info!("menu entry written: {}", desktop.display()),
        Err(e) => log::warn!("could not write the menu entry {}: {e}", desktop.display()),
    }
}

/// `$XDG_DATA_HOME` or `~/.local/share`.
fn data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").filter(|v| !v.is_empty()).map(|h| PathBuf::from(h).join(".local/share")))
}

/// The text of the entry, with `exe` as `Exec`.
fn render(exe: &Path) -> String {
    DESKTOP.replace("@EXEC@", &exe.to_string_lossy())
}

/// Whether the entry and the icon are already what this binary would write.
fn is_current(desktop: &Path, icon: &Path, exe: &Path) -> bool {
    icon.is_file() && std::fs::read_to_string(desktop).is_ok_and(|text| text == render(exe))
}

fn write_entry(desktop: &Path, icon: &Path, exe: &Path) -> std::io::Result<()> {
    if let Some(dir) = desktop.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if let Some(dir) = icon.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(icon, ICON)?;
    std::fs::write(desktop, render(exe))
}

/// Whether `exe` lives under a Cargo `target/` folder (a development build, not a package).
fn in_cargo_target(exe: &Path) -> bool {
    exe.components().any(|c| c.as_os_str() == "target")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry_points_at_the_binary_and_keeps_the_icon_name() {
        let text = render(Path::new("/home/someone/Holt/holt"));
        assert!(text.contains("Exec=\"/home/someone/Holt/holt\""), "{text}");
        assert!(text.contains("Icon=holt") && text.contains("StartupWMClass=holt"));
        assert!(!text.contains("@EXEC@"));
    }

    #[test]
    fn a_cargo_build_is_not_a_package() {
        assert!(in_cargo_target(Path::new("/home/x/Holt/target/release/holt")));
        assert!(!in_cargo_target(Path::new("/home/x/Downloads/holt-v0.3.0-linux-x64/holt")));
    }

    #[test]
    fn an_up_to_date_entry_is_not_rewritten() {
        let dir = std::env::temp_dir().join(format!("holt-desktop-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let desktop = dir.join("applications/holt.desktop");
        let icon = dir.join("icons/hicolor/256x256/apps/holt.png");
        let exe = Path::new("/opt/holt/holt");
        assert!(!is_current(&desktop, &icon, exe));
        write_entry(&desktop, &icon, exe).expect("write");
        assert!(is_current(&desktop, &icon, exe));
        // Another path (the binary moved) makes it stale again.
        assert!(!is_current(&desktop, &icon, Path::new("/opt/holt2/holt")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
