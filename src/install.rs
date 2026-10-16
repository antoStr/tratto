//! Linux, AppImage: Tratto puts itself in the applications menu (and takes itself out), with
//! no administrator password: the AppImage is copied to ~/.local/share/tratto, with an icon and
//! a menu entry beside the other applications of this user. Updates then replace that copy.

use std::path::PathBuf;

/// The AppImage Tratto is running from, if it runs from one.
pub fn appimage() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_file())
}

fn share() -> Option<PathBuf> {
    dirs::data_dir()
}

/// Where the installed AppImage lives.
pub fn installed_path() -> Option<PathBuf> {
    Some(share()?.join("tratto").join("Tratto.AppImage"))
}

fn desktop_file() -> Option<PathBuf> {
    Some(share()?.join("applications").join("tratto.desktop"))
}

fn icon_file() -> Option<PathBuf> {
    Some(share()?.join("icons/hicolor/256x256/apps/tratto.png"))
}

/// Tratto is in the applications menu (put there by this, or by the .deb or .rpm).
pub fn in_menu() -> bool {
    desktop_file().is_some_and(|p| p.exists()) || std::path::Path::new("/usr/share/applications/tratto.desktop").exists()
}

/// A menu entry starting `exe` (quoted as the desktop entry rules ask).
fn desktop_entry(exe: &std::path::Path) -> String {
    let quoted = exe.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"").replace('`', "\\`").replace('$', "\\$");
    format!(
        "[Desktop Entry]\nType=Application\nName=Tratto\nGenericName=Lavagna\nComment=Lavagna infinita con penna e condivisione sicura\nExec=\"{quoted}\"\nIcon=tratto\nTerminal=false\nCategories=Graphics;Office;Education;\nKeywords=lavagna;whiteboard;disegno;penna;\nStartupWMClass=tratto\n"
    )
}

/// Copies the running AppImage into place and adds the menu entry and the icon.
pub fn install() -> Result<(), String> {
    let src = appimage().ok_or("Tratto non è partito da un'AppImage.")?;
    let home = "Cartella personale non trovata.";
    let (dst, desktop, icon) = (installed_path().ok_or(home)?, desktop_file().ok_or(home)?, icon_file().ok_or(home)?);
    let fail = |e: std::io::Error| format!("Non riesco a installare Tratto: {e}");
    if src != dst {
        std::fs::create_dir_all(dst.parent().unwrap()).map_err(fail)?;
        // Copied beside and then renamed: a Tratto already installed (even running) is
        // replaced in one step, never left half-written.
        let part = dst.with_extension("part");
        std::fs::copy(&src, &part).map_err(fail)?;
        set_executable(&part).map_err(fail)?;
        std::fs::rename(&part, &dst).map_err(fail)?;
    }
    if let Some(png) = crate::assets::rasterize_svg(crate::assets::LOGO, 256).and_then(|p| p.encode_png().ok()) {
        std::fs::create_dir_all(icon.parent().unwrap()).map_err(fail)?;
        std::fs::write(&icon, png).map_err(fail)?;
    }
    std::fs::create_dir_all(desktop.parent().unwrap()).map_err(fail)?;
    std::fs::write(&desktop, desktop_entry(&dst)).map_err(fail)?;
    refresh_menu();
    Ok(())
}

/// Takes the menu entry, the icon and the installed copy away (the boards stay).
pub fn uninstall() -> Result<(), String> {
    for p in [desktop_file(), icon_file(), installed_path()].into_iter().flatten() {
        if p.exists() {
            std::fs::remove_file(&p).map_err(|e| format!("Non riesco a togliere {}: {e}", p.display()))?;
        }
    }
    refresh_menu();
    Ok(())
}

fn set_executable(p: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755))
}

/// Desktops that cache the menu read it again (where the tool exists; most watch the folder).
fn refresh_menu() {
    if let Some(dir) = desktop_file().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        let _ = std::process::Command::new("update-desktop-database").arg(dir).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    }
}

/// An update downloaded as an AppImage takes the place of the one running, in one step.
pub fn replace_running(new: &std::path::Path) -> Result<PathBuf, String> {
    let target = appimage().ok_or("Tratto non è partito da un'AppImage.")?;
    let fail = |e: std::io::Error| format!("Non riesco a sostituire Tratto: {e}");
    let part = target.with_extension("part");
    std::fs::copy(new, &part).map_err(fail)?;
    set_executable(&part).map_err(fail)?;
    std::fs::rename(&part, &target).map_err(fail)?;
    let _ = std::fs::remove_file(new);
    Ok(target)
}

#[cfg(test)]
mod tests {
    /// Installs and uninstalls for real, in a folder of choice:
    /// `XDG_DATA_HOME=/tmp/x APPIMAGE=/tmp/fake.AppImage cargo test installs_and_uninstalls -- --ignored`.
    #[test]
    #[ignore]
    fn installs_and_uninstalls() {
        super::install().unwrap();
        let app = super::installed_path().unwrap();
        assert!(app.is_file() && super::in_menu());
        let entry = std::fs::read_to_string(super::desktop_file().unwrap()).unwrap();
        assert!(entry.contains(&app.to_string_lossy().to_string()));
        assert!(super::icon_file().unwrap().is_file());
        super::uninstall().unwrap();
        assert!(!app.exists() && !super::desktop_file().unwrap().exists());
    }

    #[test]
    fn the_menu_entry_starts_the_installed_copy_whatever_its_path() {
        let e = super::desktop_entry(std::path::Path::new("/home/a b/.local/share/tratto/Tratto.AppImage"));
        assert!(e.contains("Exec=\"/home/a b/.local/share/tratto/Tratto.AppImage\"\n"));
        assert!(e.contains("Icon=tratto\n") && e.contains("StartupWMClass=tratto\n"));
        let odd = super::desktop_entry(std::path::Path::new("/x/$HOME`id`\"q/T"));
        assert!(odd.contains("Exec=\"/x/\\$HOME\\`id\\`\\\"q/T\""), "{odd}");
    }
}
