//! New versions: Tratto asks GitHub for the latest release, and one click updates it in place
//! and starts it again: the installer for Windows (passive, it relaunches Tratto), the app
//! copied into place on a Mac, the AppImage replaced, or the .rpm / .deb installed through the
//! system's own password prompt on Linux. Without a package for this system, the release page.

use std::sync::{Arc, Mutex};

use egui::RichText;

use crate::ui::{self, Kind as Btn};

const LATEST: &str = "https://api.github.com/repos/antoStr/tratto/releases/latest";

#[derive(Clone, Debug, Default)]
pub enum State {
    #[default]
    Idle,
    Checking,
    Latest,
    Available {
        version: String,
        url: String,
        download_url: Option<String>,
        filename: Option<String>,
    },
    Downloading {
        version: String,
        #[allow(dead_code)]
        download_url: String,
        #[allow(dead_code)]
        filename: String,
        #[allow(dead_code)]
        progress: f32,
    },
    ReadyToInstall {
        version: String,
        path: std::path::PathBuf,
    },
    /// Being installed (the system may be asking for the password).
    Installing {
        version: String,
    },
    Error(String),
}

fn state() -> &'static Arc<Mutex<State>> {
    static S: std::sync::OnceLock<Arc<Mutex<State>>> = std::sync::OnceLock::new();
    S.get_or_init(Default::default)
}

/// "1.4.0" > "1.3.2"
pub fn newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| v.trim_start_matches('v').split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    parse(latest) > parse(current)
}

pub fn check(ctx: &egui::Context) {
    let s = state().clone();
    *s.lock().unwrap() = State::Checking;
    let ctx = ctx.clone();
    let mut req = ehttp::Request::get(LATEST);
    req.headers.insert("User-Agent", "Tratto");
    ehttp::fetch(req, move |res| {
        let next = res
            .ok()
            .filter(|r| r.ok)
            .and_then(|r| serde_json::from_slice::<serde_json::Value>(&r.bytes).ok())
            .map(|v| {
                let tag = v["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_string();
                if newer(&tag, env!("CARGO_PKG_VERSION")) {
                    let (download_url, filename) = v["assets"].as_array().and_then(|assets| {
                        #[cfg(windows)]
                        let matches = |name: &str| name.ends_with("-setup.exe");
                        #[cfg(target_os = "macos")]
                        let matches = |name: &str| name.ends_with(".dmg") && name.contains(if cfg!(target_arch = "aarch64") { "aarch64" } else { "x64" });
                        // Linux: the kind of package Tratto came in.
                        #[cfg(target_os = "linux")]
                        let kind = linux_package();
                        #[cfg(target_os = "linux")]
                        let matches = |name: &str| match kind {
                            Some(Package::AppImage) => name.ends_with(".AppImage"),
                            Some(Package::Rpm) => name.ends_with(".rpm"),
                            Some(Package::Deb) => name.ends_with(".deb"),
                            None => false,
                        };
                        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
                        let matches = |_: &str| false;

                        assets.iter().find(|a| {
                            a["name"].as_str().map(matches).unwrap_or(false)
                        }).and_then(|a| {
                            let dl = a["browser_download_url"].as_str()?.to_string();
                            let name = a["name"].as_str()?.to_string();
                            Some((dl, name))
                        })
                    }).unzip();

                    State::Available {
                        version: tag,
                        url: v["html_url"].as_str().unwrap_or("https://github.com/antoStr/tratto/releases/latest").to_string(),
                        download_url,
                        filename,
                    }
                } else {
                    State::Latest
                }
            })
            .unwrap_or_else(|| State::Error("Impossibile contattare il server di aggiornamento.".into()));
        *s.lock().unwrap() = next;
        ctx.request_repaint();
    });
}

/// How Tratto was installed on this Linux.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, PartialEq, Debug)]
enum Package {
    AppImage,
    Rpm,
    Deb,
}

#[cfg(target_os = "linux")]
fn linux_package() -> Option<Package> {
    if crate::install::appimage().is_some() {
        return Some(Package::AppImage);
    }
    let exe = std::env::current_exe().ok()?;
    if !exe.starts_with("/usr") {
        return None;
    }
    let owns = |cmd: &str, arg: &str| std::process::Command::new(cmd).args([arg, &exe.to_string_lossy()]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().is_ok_and(|s| s.success());
    if owns("rpm", "-qf") {
        Some(Package::Rpm)
    } else if owns("dpkg", "-S") {
        Some(Package::Deb)
    } else {
        None
    }
}

/// Downloads the update and, once it is here, installs it and starts Tratto again.
pub fn start_download(ctx: &egui::Context) {
    let s = state().clone();
    let current_state = s.lock().unwrap().clone();
    let State::Available { version, url, download_url, filename } = current_state else {
        return;
    };
    let Some(dl_url) = download_url else {
        let _ = webbrowser::open(&url);
        return;
    };
    let fname = filename.unwrap_or_else(|| format!("tratto-update-{version}.exe"));
    *s.lock().unwrap() = State::Downloading {
        version: version.clone(),
        download_url: dl_url.clone(),
        filename: fname.clone(),
        progress: 0.1,
    };
    let ctx = ctx.clone();
    let s_clone = s.clone();

    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(move || {
        // In this person's own cache folder, not the shared temporary one, where someone else
        // on the computer could put something in its place.
        let dir = dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("tratto-updates");
        let _ = std::fs::create_dir_all(&dir);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        }
        let fname = std::path::Path::new(&fname).file_name().map_or_else(|| "tratto-update".into(), |n| n.to_string_lossy().to_string());
        let target_path = dir.join(fname);
        let mut req = ehttp::Request::get(&dl_url);
        req.headers.insert("User-Agent", "Tratto");
        ehttp::fetch(req, move |res| {
            match res {
                Ok(r) if r.ok => {
                    match std::fs::write(&target_path, &r.bytes) {
                        Ok(()) => {
                            *s_clone.lock().unwrap() = State::ReadyToInstall {
                                version,
                                path: target_path,
                            };
                            install_and_restart();
                        }
                        Err(e) => {
                            *s_clone.lock().unwrap() = State::Error(format!("Impossibile salvare l'aggiornamento: {e}"));
                        }
                    }
                }
                Ok(r) => {
                    *s_clone.lock().unwrap() = State::Error(format!("Errore nel download: HTTP {}", r.status));
                }
                Err(e) => {
                    *s_clone.lock().unwrap() = State::Error(format!("Errore nel download: {e}"));
                }
            }
            ctx.request_repaint();
        });
    });
}

/// Installs the update downloaded and starts the new Tratto (in a thread: the system may ask
/// for a password meanwhile). If it can't, the button to try again stays.
pub fn install_and_restart() {
    let s = state().clone();
    let current_state = s.lock().unwrap().clone();
    let State::ReadyToInstall { path, version } = current_state else { return };
    *s.lock().unwrap() = State::Installing { version: version.clone() };
    std::thread::spawn(move || {
        let back = |e: String| *s.lock().unwrap() = State::Error(e);
        // Windows: the installer without questions (it closes Tratto) and Tratto started again.
        #[cfg(windows)]
        {
            match std::process::Command::new(&path).args(["/P", "/R"]).spawn() {
                Ok(_) => std::process::exit(0),
                Err(e) => back(format!("Non riesco ad avviare l'installazione: {e}")),
            }
        }
        // A Mac: the app in the disk image copied over this one, then opened again.
        #[cfg(target_os = "macos")]
        {
            match mac_replace(&path) {
                Ok(app) => {
                    let _ = std::process::Command::new("open").args(["-n"]).arg(&app).spawn();
                    std::process::exit(0);
                }
                Err(e) => {
                    // Not possible in place (e.g. no permission): the disk image, as before.
                    let _ = std::process::Command::new("open").arg(&path).spawn();
                    back(e);
                }
            }
        }
        #[cfg(target_os = "linux")]
        {
            let exe = match linux_package() {
                Some(Package::AppImage) => crate::install::replace_running(&path),
                Some(kind) => linux_install(kind, &path),
                None => Err("Tratto non sa come si aggiorna qui: scaricalo dalla pagina della versione.".into()),
            };
            match exe {
                Ok(exe) => {
                    if std::process::Command::new(&exe).spawn().is_ok() {
                        std::process::exit(0);
                    }
                    back("Aggiornato: riapri Tratto.".into());
                }
                Err(e) => back(e),
            }
        }
        let _ = (&path, &version);
    });
}

/// An .rpm or .deb installed with the system's own password prompt (pkexec), with its package
/// tool; the program it installs.
#[cfg(target_os = "linux")]
fn linux_install(kind: Package, path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let has = |tool: &str| std::process::Command::new("sh").args(["-c", &format!("command -v {tool}")]).stdout(std::process::Stdio::null()).status().is_ok_and(|s| s.success());
    let p = path.to_string_lossy().to_string();
    let args: Vec<String> = match kind {
        Package::Rpm if has("dnf") => vec!["dnf".into(), "install".into(), "-y".into(), p],
        Package::Rpm if has("zypper") => vec!["zypper".into(), "--non-interactive".into(), "install".into(), "--allow-unsigned-rpm".into(), p],
        Package::Rpm => vec!["rpm".into(), "-U".into(), "--replacepkgs".into(), p],
        Package::Deb if has("apt-get") => vec!["apt-get".into(), "install".into(), "-y".into(), p],
        _ => vec!["dpkg".into(), "-i".into(), p],
    };
    let status = std::process::Command::new("pkexec").args(&args).status().map_err(|e| format!("Non riesco a chiedere la password: {e}"))?;
    if !status.success() {
        return Err("Installazione annullata.".into());
    }
    let _ = std::fs::remove_file(path);
    std::env::current_exe().map_err(|e| e.to_string())
}

/// The app inside a downloaded disk image put in the place of the one running.
#[cfg(target_os = "macos")]
fn mac_replace(dmg: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // …/Tratto.app/Contents/MacOS/tratto
    let app = exe.ancestors().nth(3).filter(|a| a.extension().is_some_and(|x| x == "app")).ok_or("Tratto non è in un'app")?.to_path_buf();
    let mount = std::env::temp_dir().join(format!("tratto-update-{}", std::process::id()));
    let ok = std::process::Command::new("hdiutil").args(["attach", "-nobrowse", "-quiet", "-mountpoint"]).arg(&mount).arg(dmg).status().is_ok_and(|s| s.success());
    if !ok {
        return Err("Non riesco ad aprire l'aggiornamento.".into());
    }
    let new_app = std::fs::read_dir(&mount).ok().and_then(|d| d.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "app")));
    let copied = new_app.as_ref().is_some_and(|src| {
        let staged = app.with_extension("app.new");
        let _ = std::fs::remove_dir_all(&staged);
        std::process::Command::new("ditto").arg(src).arg(&staged).status().is_ok_and(|s| s.success())
            && std::fs::remove_dir_all(&app).is_ok()
            && std::fs::rename(&staged, &app).is_ok()
    });
    let _ = std::process::Command::new("hdiutil").args(["detach", "-quiet"]).arg(&mount).status();
    if copied { Ok(app) } else { Err("Non riesco a sostituire l'app: aggiornala dall'immagine disco.".into()) }
}

pub fn current() -> State {
    state().lock().unwrap().clone()
}

pub fn settings_ui(ui: &mut egui::Ui) {
    let t = ui::theme(ui.ctx());
    let s = current();
    let (text, is_err) = match &s {
        State::Idle => ("Nessun controllo ancora eseguito.".to_string(), false),
        State::Checking => ("Controllo in corso…".into(), false),
        State::Latest => ("Hai l'ultima versione.".into(), false),
        State::Available { version, .. } => (format!("È disponibile la versione {version}."), false),
        State::Downloading { version, .. } => (format!("Download della versione {version} in corso…"), false),
        State::ReadyToInstall { version, .. } => (format!("Versione {version} scaricata"), false),
        State::Installing { version } => (format!("Installo la versione {version}…"), false),
        State::Error(msg) => (format!("Errore: {msg}"), true),
    };
    ui.label(RichText::new(text).color(if is_err { t.danger_text } else { t.text2 }));
    ui.horizontal(|ui| {
        match &s {
            State::Idle | State::Latest | State::Error(_) => {
                if ui::button(ui, "Cerca aggiornamenti", Btn::Secondary, None, false, true).clicked() {
                    check(ui.ctx());
                }
            }
            State::Checking => {
                ui.spinner();
            }
            State::Available { version, url, download_url, .. } => {
                if download_url.is_some() {
                    if ui::button(ui, &format!("Aggiorna a {version} e riavvia"), Btn::Primary, Some("download"), false, true).clicked() {
                        start_download(ui.ctx());
                    }
                } else if ui::button(ui, "Scarica dal browser", Btn::Primary, Some("download"), false, true).clicked() {
                    let _ = webbrowser::open(url);
                }
                if download_url.is_some() && ui::button(ui, "Apri note su GitHub", Btn::Ghost, None, false, true).clicked() {
                    let _ = webbrowser::open(url);
                }
            }
            State::Downloading { .. } | State::Installing { .. } => {
                ui.spinner();
            }
            State::ReadyToInstall { version, .. } => {
                if ui::button(ui, &format!("Installa {version} e riavvia"), Btn::Primary, Some("check"), false, true).clicked() {
                    install_and_restart();
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn versions_compare_by_number() {
        assert!(super::newer("1.10.0", "1.9.3"));
        assert!(super::newer("v2.0.0", "1.99.99"));
        assert!(!super::newer("1.3.0", "1.3.0"));
    }
}
