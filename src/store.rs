//! Boards on disk: SQLite in the app data folder, same schema as the first Tratto
//! (boards as Yjs state, images, thumbnails, settings).

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, OptionalExtension, params};

#[derive(Clone, Debug)]
pub struct BoardRow {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub has_thumb: bool,
}

#[derive(Clone)]
pub struct Store(Arc<Mutex<Connection>>);

pub type Result<T> = std::result::Result<T, String>;

fn err(e: rusqlite::Error) -> String {
    format!("Errore del database: {e}")
}

/// A new random id, url-safe base64 (as the first Tratto made them).
pub fn new_id(bytes: usize) -> String {
    use base64::Engine;
    let mut b = vec![0u8; bytes];
    crate::platform::random(&mut b);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

pub fn clean_title(t: &str) -> String {
    let t = t.trim();
    if t.is_empty() { "Lavagna senza titolo".into() } else { t.chars().take(120).collect() }
}

/// Detects the image type from its first bytes; SVG and anything else is refused.
pub fn sniff_image(b: &[u8]) -> Option<&'static str> {
    if b.len() < 12 {
        return None;
    }
    if b[0] == 0x89 && &b[1..4] == b"PNG" {
        Some("image/png")
    } else if b[..3] == [0xff, 0xd8, 0xff] {
        Some("image/jpeg")
    } else if &b[..4] == b"GIF8" {
        Some("image/gif")
    } else if &b[..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn now() -> i64 {
    crate::platform::now_ms() as i64
}

impl Store {
    pub fn open(file: &Path) -> Result<Store> {
        if let Some(dir) = file.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| format!("Non riesco a creare la cartella delle lavagne: {e}"))?;
        }
        let db = Connection::open(file).map_err(err)?;
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS boards (
               id TEXT PRIMARY KEY, title TEXT NOT NULL, ydoc BLOB, thumbnail BLOB,
               created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS files (
               board_id TEXT NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
               id TEXT NOT NULL, mime TEXT NOT NULL, data BLOB NOT NULL, created_at INTEGER NOT NULL,
               PRIMARY KEY (board_id, id));
             CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .map_err(err)?;
        Ok(Store(Arc::new(Mutex::new(db))))
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn boards(&self) -> Result<Vec<BoardRow>> {
        let db = self.db();
        let mut q = db.prepare("SELECT id, title, created_at, updated_at, thumbnail IS NOT NULL FROM boards ORDER BY updated_at DESC").map_err(err)?;
        let rows = q.query_map([], |r| Ok(BoardRow { id: r.get(0)?, title: r.get(1)?, created_at: r.get(2)?, updated_at: r.get(3)?, has_thumb: r.get(4)? })).map_err(err)?;
        rows.collect::<std::result::Result<_, _>>().map_err(err)
    }

    pub fn board(&self, id: &str) -> Result<Option<BoardRow>> {
        self.db()
            .query_row("SELECT id, title, created_at, updated_at, thumbnail IS NOT NULL FROM boards WHERE id = ?", [id], |r| {
                Ok(BoardRow { id: r.get(0)?, title: r.get(1)?, created_at: r.get(2)?, updated_at: r.get(3)?, has_thumb: r.get(4)? })
            })
            .optional()
            .map_err(err)
    }

    pub fn create(&self, title: &str, ydoc: Option<&[u8]>) -> Result<String> {
        let id = new_id(9);
        let t = now();
        self.db().execute("INSERT INTO boards (id, title, ydoc, created_at, updated_at) VALUES (?, ?, ?, ?, ?)", params![id, clean_title(title), ydoc, t, t]).map_err(err)?;
        Ok(id)
    }

    pub fn rename(&self, id: &str, title: &str) -> Result<()> {
        self.db().execute("UPDATE boards SET title = ? WHERE id = ?", params![clean_title(title), id]).map_err(err)?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        self.db().execute("DELETE FROM boards WHERE id = ?", [id]).map_err(err)?;
        Ok(())
    }

    /// A copy with its images; `state` is the live document when the board is open.
    pub fn duplicate(&self, id: &str, state: Option<Vec<u8>>) -> Result<String> {
        let src = self.board(id)?.ok_or("Questa lavagna non esiste più")?;
        let state = match state {
            Some(s) => Some(s),
            None => self.ydoc(id)?,
        };
        let new = self.create(&format!("{} (copia)", src.title), state.as_deref())?;
        self.db().execute("INSERT INTO files (board_id, id, mime, data, created_at) SELECT ?, id, mime, data, created_at FROM files WHERE board_id = ?", params![new, id]).map_err(err)?;
        Ok(new)
    }

    pub fn ydoc(&self, id: &str) -> Result<Option<Vec<u8>>> {
        self.db().query_row("SELECT ydoc FROM boards WHERE id = ?", [id], |r| r.get::<_, Option<Vec<u8>>>(0)).optional().map(Option::flatten).map_err(err)
    }

    pub fn save_ydoc(&self, id: &str, state: &[u8]) -> Result<()> {
        self.db().execute("UPDATE boards SET ydoc = ?, updated_at = ? WHERE id = ?", params![state, now(), id]).map_err(err)?;
        Ok(())
    }

    pub fn thumbnail(&self, id: &str) -> Result<Option<Vec<u8>>> {
        self.db().query_row("SELECT thumbnail FROM boards WHERE id = ?", [id], |r| r.get::<_, Option<Vec<u8>>>(0)).optional().map(Option::flatten).map_err(err)
    }

    pub fn set_thumbnail(&self, id: &str, png: &[u8]) -> Result<()> {
        if sniff_image(png).is_none() {
            return Err("Miniatura non valida".into());
        }
        self.db().execute("UPDATE boards SET thumbnail = ? WHERE id = ?", params![png, id]).map_err(err)?;
        Ok(())
    }

    pub fn file(&self, board: &str, id: &str) -> Result<Option<(String, Vec<u8>)>> {
        self.db().query_row("SELECT mime, data FROM files WHERE board_id = ? AND id = ?", [board, id], |r| Ok((r.get(0)?, r.get(1)?))).optional().map_err(err)
    }

    /// Stores a validated image and returns its id.
    pub fn save_file(&self, board: &str, data: &[u8]) -> Result<String> {
        let mime = sniff_image(data).ok_or("Formato non supportato: usa PNG, JPG, GIF o WebP")?;
        if self.board(board)?.is_none() {
            return Err("Questa lavagna non esiste più".into());
        }
        let id = new_id(12);
        self.db().execute("INSERT INTO files (board_id, id, mime, data, created_at) VALUES (?, ?, ?, ?, ?)", params![board, id, mime, data, now()]).map_err(err)?;
        Ok(id)
    }

    pub fn files(&self, board: &str) -> Result<Vec<(String, String, Vec<u8>)>> {
        let db = self.db();
        let mut q = db.prepare("SELECT id, mime, data FROM files WHERE board_id = ?").map_err(err)?;
        let rows = q.query_map([board], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map_err(err)?;
        rows.collect::<std::result::Result<_, _>>().map_err(err)
    }

    /// A board read from a .tratto file, with its images, in one transaction.
    pub fn import(&self, title: &str, state: &[u8], files: &[(String, Vec<u8>)]) -> Result<String> {
        let id = new_id(9);
        let t = now();
        let mut db = self.db();
        let tx = db.transaction().map_err(err)?;
        tx.execute("INSERT INTO boards (id, title, ydoc, created_at, updated_at) VALUES (?, ?, ?, ?, ?)", params![id, clean_title(title), state, t, t]).map_err(err)?;
        for (fid, data) in files {
            if let Some(mime) = sniff_image(data)
                && crate::model::valid_file_id(fid)
            {
                tx.execute("INSERT OR IGNORE INTO files (board_id, id, mime, data, created_at) VALUES (?, ?, ?, ?, ?)", params![id, fid, mime, data, t]).map_err(err)?;
            }
        }
        tx.commit().map_err(err)?;
        Ok(id)
    }

    pub fn setting(&self, key: &str) -> Option<String> {
        self.db().query_row("SELECT value FROM settings WHERE key = ?", [key], |r| r.get(0)).optional().ok().flatten()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.db().execute("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [key, value]).map_err(err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boards_files_and_settings() {
        let s = Store::open(Path::new(":memory:")).unwrap();
        let id = s.create("  Prova  ", None).unwrap();
        assert_eq!(s.board(&id).unwrap().unwrap().title, "Prova");
        assert!(s.save_file(&id, b"<svg onload=alert(1)>....").is_err());
        let png = [0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0, 0, 13];
        let f = s.save_file(&id, &png).unwrap();
        assert_eq!(s.file(&id, &f).unwrap().unwrap().0, "image/png");
        let copy = s.duplicate(&id, None).unwrap();
        assert_eq!(s.board(&copy).unwrap().unwrap().title, "Prova (copia)");
        assert!(s.file(&copy, &f).unwrap().is_some());
        s.delete(&id).unwrap();
        assert!(s.file(&id, &f).unwrap().is_none(), "images go with their board");
        s.set_setting("prefs", "{}").unwrap();
        assert_eq!(s.setting("prefs").as_deref(), Some("{}"));
    }

    /// Opens every board of a copy of real data: `TRATTO_TEST_DB=path cargo test real_boards -- --nocapture`.
    #[test]
    fn real_boards_load() {
        let Ok(path) = std::env::var("TRATTO_TEST_DB") else { return };
        let s = Store::open(Path::new(&path)).unwrap();
        for b in s.boards().unwrap() {
            let state = s.ydoc(&b.id).unwrap().unwrap_or_default();
            let doc = yrs::Doc::new();
            if !state.is_empty() {
                crate::doc::apply_update(&doc, &state, crate::doc::REMOTE).unwrap();
            }
            let raw = {
                use yrs::{Map, Transact};
                doc.get_or_insert_map("elements").len(&doc.transact())
            };
            let board = crate::doc::Board::new(doc, std::sync::Arc::new(|| {}));
            println!("{:<40} {:>5} elements, {:>5} parsed, {} bytes", b.title, raw, board.len(), state.len());
            assert_eq!(raw as usize, board.len(), "every element of {} parses", b.title);
        }
    }
}
