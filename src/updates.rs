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
    Available { version: String, url: String },
    Error,
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
                if newer(&tag, env!("CARGO_PKG_VERSION")) { State::Available { version: tag, url: v["html_url"].as_str().unwrap_or("https://github.com/antoStr/tratto/releases/latest").to_string() } } else { State::Latest }
            })
            .unwrap_or(State::Error);
        *s.lock().unwrap() = next;
        ctx.request_repaint();
    });
}

pub fn current() -> State {
    state().lock().unwrap().clone()
}

pub fn settings_ui(ui: &mut egui::Ui) {
    let t = ui::theme(ui.ctx());
    let s = current();
    let text = match &s {
        State::Idle => "Nessun controllo ancora eseguito.".to_string(),
        State::Checking => "Controllo in corso…".into(),
        State::Latest => "Hai l'ultima versione.".into(),
        State::Available { version, .. } => format!("È disponibile la versione {version}."),
        State::Error => "Impossibile controllare gli aggiornamenti. Controlla la connessione e riprova.".into(),
    };
    ui.label(RichText::new(text).color(if matches!(s, State::Error) { t.danger_text } else { t.text2 }));
    ui.horizontal(|ui| {
        if ui::button(ui, "Cerca aggiornamenti", Btn::Secondary, None, false, !matches!(s, State::Checking)).clicked() {
            check(ui.ctx());
        }
        if let State::Available { url, .. } = &s
            && ui::button(ui, "Scarica la nuova versione", Btn::Primary, Some("download"), false, true).clicked()
        {
            let _ = webbrowser::open(url);
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
