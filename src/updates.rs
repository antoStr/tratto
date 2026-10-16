//! New versions: Tratto asks GitHub for the latest release and offers its download page.

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
                        let matches = |name: &str| name.ends_with("-setup.exe") || name.ends_with(".exe");
                        #[cfg(target_os = "macos")]
                        let matches = |name: &str| name.ends_with(".dmg");
                        // Linux: an AppImage replaces itself; a .deb or .rpm is updated by the
                        // system's software centre (the button opens the download page).
                        #[cfg(target_os = "linux")]
                        let appimage = crate::install::appimage().is_some();
                        #[cfg(target_os = "linux")]
                        let matches = |name: &str| appimage && name.ends_with(".AppImage");
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

pub fn install_and_restart() {
    let s = state().clone();
    let current_state = s.lock().unwrap().clone();
    let State::ReadyToInstall { path, .. } = current_state else { return };

    #[cfg(windows)]
    {
        let spawned = std::process::Command::new(&path).spawn().is_ok()
            || std::process::Command::new("cmd").args(["/c", "start", "", &path.to_string_lossy()]).spawn().is_ok()
            || webbrowser::open(&path.to_string_lossy()).is_ok()
            || std::process::Command::new("powershell").args(["-WindowStyle", "Hidden", "-Command", &format!("Start-Process -FilePath '{}'", path.display())]).spawn().is_ok();
        if spawned {
            std::process::exit(0);
        }
    }
    #[cfg(target_os = "macos")]
    {
        if std::process::Command::new("open").arg(&path).spawn().is_ok() {
            std::process::exit(0);
        }
    }
    // Linux: the new AppImage takes the place of the one running, then starts.
    #[cfg(target_os = "linux")]
    match crate::install::replace_running(&path) {
        Ok(exe) => {
            if std::process::Command::new(&exe).spawn().is_ok() {
                std::process::exit(0);
            }
        }
        Err(e) => *s.lock().unwrap() = State::Error(e),
    }
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
        State::ReadyToInstall { version, .. } => (format!("La versione {version} è pronta per l'installazione!"), false),
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
                    if ui::button(ui, &format!("Aggiorna a Tratto {version}"), Btn::Primary, Some("download"), false, true).clicked() {
                        start_download(ui.ctx());
                    }
                } else if ui::button(ui, "Scarica dal browser", Btn::Primary, Some("download"), false, true).clicked() {
                    let _ = webbrowser::open(url);
                }
                if download_url.is_some() && ui::button(ui, "Apri note su GitHub", Btn::Ghost, None, false, true).clicked() {
                    let _ = webbrowser::open(url);
                }
            }
            State::Downloading { .. } => {
                ui.spinner();
                ui.label(RichText::new("Download in corso…").color(t.text2));
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
