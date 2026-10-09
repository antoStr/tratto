//! The shared board: a Yjs document (via yrs) with one map of elements, each a plain JSON
//! object replaced as a whole on edit, so concurrent edits of one element are last-writer-wins.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use serde_json::Value;
use yrs::types::map::MapEvent;
use yrs::undo::{Options as UndoOptions, UndoManager};
use yrs::{Any, Doc, Map, MapRef, Number, Observable, Origin, Out, ReadTxn, Transact, TransactionMut};

use crate::geom::route_connector;
use crate::model::{BoardMeta, El, GRID_SIZES, Pattern};

/// Transaction origin of this user's edits: the undo manager only tracks these.
pub const LOCAL: &str = "local";
/// Origin of edits that came from other people.
pub const REMOTE: &str = "remote";

#[derive(Default)]
struct Changes {
    keys: HashSet<String>,
    all: bool,
    other: bool,
}

pub struct Board {
    pub doc: Doc,
    pub elements: MapRef,
    pub meta: MapRef,
    /// Folder names; membership is the `group_id` of each element.
    pub groups: MapRef,
    /// Shared countdown. Not undoable.
    pub timer: MapRef,
    /// Voting session: "per" votes each, "ended", and one key per voter and element.
    pub votes: MapRef,
    undo: UndoManager<()>,
    els: HashMap<String, Arc<El>>,
    sorted: Option<Vec<Arc<El>>>,
    painted: Option<Vec<Arc<El>>>,
    changes: Arc<Mutex<Changes>>,
    /// Bumped on every change, for caches outside the board.
    pub version: u64,
    last_local: f64,
}

pub type Notify = Arc<dyn Fn() + Send + Sync>;

impl Board {
    /// `notify` is called (from any thread) whenever the document changes.
    pub fn new(doc: Doc, notify: Notify) -> Board {
        let elements = doc.get_or_insert_map("elements");
        let meta = doc.get_or_insert_map("meta");
        let groups = doc.get_or_insert_map("groups");
        let timer = doc.get_or_insert_map("timer");
        let votes = doc.get_or_insert_map("votes");
        let changes = Arc::new(Mutex::new(Changes { all: true, ..Default::default() }));
        {
            let (c, n) = (changes.clone(), notify.clone());
            elements.observe("board", move |txn: &TransactionMut, e: &MapEvent| {
                let mut c = c.lock().unwrap();
                c.keys.extend(e.keys(txn).keys().map(|k| k.to_string()));
                drop(c);
                n();
            });
        }
        for m in [&meta, &groups, &timer, &votes] {
            let (c, n) = (changes.clone(), notify.clone());
            m.observe("board", move |_: &TransactionMut, _: &MapEvent| {
                c.lock().unwrap().other = true;
                n();
            });
        }
        // One undo step per gesture: callers call `stop_capturing` when a gesture starts.
        let opts = UndoOptions::<()> {
            capture_timeout_millis: 60_000,
            tracked_origins: HashSet::from([Origin::from(LOCAL)]),
            capture_transaction: None,
            timestamp: Arc::new(|| crate::platform::now_ms() as u64),
            init_undo_stack: Vec::new(),
            init_redo_stack: Vec::new(),
        };
        let mut undo = UndoManager::with_options(opts);
        undo.expand_scope(&doc, &elements);
        undo.expand_scope(&doc, &meta);
        undo.expand_scope(&doc, &groups);
        let mut b = Board { doc, elements, meta, groups, timer, votes, undo, els: HashMap::new(), sorted: None, painted: None, changes, version: 0, last_local: 0.0 };
        b.refresh();
        b
    }

    /// Brings the parsed elements up to date with the document. Cheap when nothing changed.
    pub fn refresh(&mut self) -> bool {
        let c = std::mem::take(&mut *self.changes.lock().unwrap());
        if !c.all && c.keys.is_empty() && !c.other {
            return false;
        }
        let txn = self.doc.transact();
        if c.all {
            self.els = self.elements.iter(&txn).filter_map(|(k, v)| parse(k, &v).map(|el| (k.to_string(), Arc::new(el)))).collect();
        } else {
            for k in c.keys {
                match self.elements.get(&txn, &k).and_then(|v| parse(&k, &v)) {
                    Some(el) => self.els.insert(k, Arc::new(el)),
                    None => self.els.remove(&k),
                };
            }
        }
        self.sorted = None;
        self.painted = None;
        self.version += 1;
        true
    }

    /// Elements bottom to top.
    pub fn all(&mut self) -> &[Arc<El>] {
        if self.sorted.is_none() {
            let mut v: Vec<Arc<El>> = self.els.values().cloned().collect();
            v.sort_by(|a, b| a.z.total_cmp(&b.z).then_with(|| a.id.cmp(&b.id)));
            self.sorted = Some(v);
        }
        self.sorted.as_deref().unwrap()
    }

    /// Paint order: sections at the back (largest first), then everything else by z.
    pub fn paint_order(&mut self) -> &[Arc<El>] {
        if self.painted.is_none() {
            let all = self.all().to_vec();
            let mut sections: Vec<Arc<El>> = all.iter().filter(|e| e.is_section()).cloned().collect();
            sections.sort_by(|a, b| (b.w * b.h).total_cmp(&(a.w * a.h)));
            sections.extend(all.into_iter().filter(|e| !e.is_section()));
            self.painted = Some(sections);
        }
        self.painted.as_deref().unwrap()
    }

    pub fn get(&self, id: &str) -> Option<&Arc<El>> {
        self.els.get(id)
    }

    pub fn len(&self) -> usize {
        self.els.len()
    }

    pub fn meta(&self) -> BoardMeta {
        let txn = self.doc.transact();
        let d = BoardMeta::default();
        let get = |k: &str| self.meta.get(&txn, k).and_then(|v| out_json(&v));
        let background = get("background").and_then(|v| v.as_str().map(str::to_string)).filter(|s| s.len() == 7 && crate::model::parse_color(s).is_some()).unwrap_or(d.background);
        let pattern = get("pattern").and_then(|v| serde_json::from_value::<Pattern>(v).ok()).unwrap_or(d.pattern);
        let grid_size = get("gridSize").and_then(|v| v.as_f64()).filter(|g| GRID_SIZES.contains(g)).unwrap_or(d.grid_size);
        BoardMeta { background, pattern, grid_size }
    }

    pub fn has_meta(&self) -> bool {
        self.meta.len(&self.doc.transact()) > 0
    }

    pub fn top_z(&mut self) -> f64 {
        self.all().last().map_or(0.0, |e| e.z + 1.0)
    }
    pub fn bottom_z(&mut self) -> f64 {
        self.all().first().map_or(0.0, |e| e.z - 1.0)
    }

    /* ---------- editing ---------- */

    /// A local edit, undoable. Edits after a pause of more than 0.8 s start a new undo step.
    /// Connectors attached to what changed are re-routed in the same step.
    pub fn transact<R>(&mut self, f: impl FnOnce(&mut Edit) -> R) -> R {
        let now = crate::platform::now_ms();
        if now - self.last_local > 800.0 {
            self.undo.reset();
        }
        self.last_local = now;
        let (r, touched) = {
            let mut e = Edit { txn: self.doc.transact_mut_with(LOCAL), maps: (&self.elements, &self.meta, &self.groups), touched: HashSet::new() };
            let r = f(&mut e);
            (r, e.touched)
        };
        self.refresh();
        self.reroute(&touched);
        r
    }

    /// Edits that are not part of anyone's undo history (timer, votes, a new board's background).
    pub fn transact_plain<R>(&mut self, f: impl FnOnce(&mut TransactionMut, &Board) -> R) -> R {
        let r = {
            let mut txn = self.doc.transact_mut();
            f(&mut txn, self)
        };
        self.refresh();
        r
    }

    pub fn stop_capturing(&mut self) {
        self.undo.reset();
    }

    pub fn undo(&mut self) -> bool {
        let done = self.undo.undo_blocking();
        self.refresh();
        done
    }

    pub fn redo(&mut self) -> bool {
        let done = self.undo.redo_blocking();
        self.refresh();
        done
    }

    pub fn put(&mut self, els: impl IntoIterator<Item = El>) {
        self.transact(|e| {
            for el in els {
                e.put(&el);
            }
        });
    }

    pub fn remove(&mut self, ids: impl IntoIterator<Item = String>) {
        self.transact(|e| {
            for id in ids {
                e.remove(&id);
            }
        });
    }

    /// Changes elements in place through `f`.
    pub fn update(&mut self, ids: &[String], mut f: impl FnMut(&mut El)) {
        let next: Vec<El> = ids
            .iter()
            .filter_map(|id| self.get(id))
            .map(|el| {
                let mut el = (**el).clone();
                f(&mut el);
                el
            })
            .collect();
        self.put(next);
    }

    pub fn set_meta(&mut self, meta: &BoardMeta) {
        self.transact(|e| e.set_meta(meta));
    }

    /// Connectors attached to any of `ids` (or being one of them) follow the elements they join.
    fn reroute(&mut self, ids: &HashSet<String>) {
        if ids.is_empty() {
            return;
        }
        let mut fixes = Vec::new();
        for el in self.els.values() {
            let Some(l) = el.line() else { continue };
            if l.from.is_none() && l.to.is_none() {
                continue;
            }
            let hit = |o: &Option<String>| o.as_ref().is_some_and(|i| ids.contains(i));
            if !ids.contains(&el.id) && !hit(&l.from) && !hit(&l.to) {
                continue;
            }
            if let Some(next) = route_connector(el, |id| self.els.get(id).map(|e| &**e))
                && !same_line(&next, el)
            {
                fixes.push(next);
            }
        }
        if !fixes.is_empty() {
            {
                let mut e = Edit { txn: self.doc.transact_mut_with(LOCAL), maps: (&self.elements, &self.meta, &self.groups), touched: HashSet::new() };
                for l in &fixes {
                    e.put(l);
                }
            }
            self.refresh();
        }
    }

    /* ---------- folders ---------- */

    /// Elements of a folder, bottom to top.
    pub fn members(&mut self, group: &str) -> Vec<Arc<El>> {
        self.all().iter().filter(|e| e.group_id.as_deref() == Some(group)).cloned().collect()
    }

    /// A folder's name ("Cartella" when it has none).
    pub fn group_name(&self, group: &str) -> String {
        let txn = self.doc.transact();
        self.groups
            .get(&txn, group)
            .and_then(|v| out_json(&v))
            .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(|s| s.chars().take(80).collect::<String>()))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Cartella".into())
    }

    /// Folders that still have elements, in stacking order: (id, name).
    pub fn group_list(&mut self) -> Vec<(String, String)> {
        let mut seen: Vec<String> = Vec::new();
        for el in self.all() {
            if let Some(g) = &el.group_id
                && !seen.contains(g)
            {
                seen.push(g.clone());
            }
        }
        seen.into_iter().map(|g| {
            let name = self.group_name(&g);
            (g, name)
        }).collect()
    }

    pub fn next_group_name(&mut self) -> String {
        let list = self.group_list();
        let mut n = list.len() + 1;
        while list.iter().any(|(_, name)| *name == format!("Cartella {n}")) {
            n += 1;
        }
        format!("Cartella {n}")
    }

    /// Puts elements into a new folder and returns its id.
    pub fn create_group(&mut self, ids: &[String], name: Option<String>) -> String {
        let name = name.unwrap_or_else(|| self.next_group_name());
        let id = crate::model::uid();
        let els: Vec<El> = ids.iter().filter_map(|i| self.get(i)).map(|e| El { group_id: Some(id.clone()), ..(**e).clone() }).collect();
        self.transact(|e| {
            e.set_group_name(&id, &name);
            for el in &els {
                e.put(el);
            }
        });
        self.restack(&id);
        self.prune_groups();
        id
    }

    /// Moves elements into an existing folder, or out of any folder with None.
    pub fn set_group(&mut self, ids: &[String], group: Option<&str>) {
        let els: Vec<El> = ids.iter().filter_map(|i| self.get(i)).map(|e| El { group_id: group.map(str::to_string), ..(**e).clone() }).collect();
        self.put(els);
        if let Some(g) = group {
            self.restack(g);
        }
        self.prune_groups();
    }

    pub fn rename_group(&mut self, group: &str, name: &str) {
        let clean: String = name.trim().chars().take(80).collect();
        if !clean.is_empty() {
            self.transact(|e| e.set_group_name(group, &clean));
        }
    }

    /// Like a Figma group, a folder's elements sit next to each other in the stack, right under
    /// its top element. Elements that were in between end up below the folder.
    fn restack(&mut self, group: &str) {
        let all = self.all().to_vec();
        let members: Vec<&Arc<El>> = all.iter().filter(|e| e.group_id.as_deref() == Some(group)).collect();
        if members.len() < 2 {
            return;
        }
        let top = members[members.len() - 1];
        let top_at = all.iter().position(|e| e.id == top.id).unwrap();
        let below = all[..top_at].iter().rev().find(|e| e.group_id.as_deref() != Some(group));
        let lo = below.map_or(top.z - 1.0, |b| b.z);
        let step = (top.z - lo) / members.len() as f64;
        let mut fixes = Vec::new();
        for (k, el) in members[..members.len() - 1].iter().enumerate() {
            let z = lo + step * (k + 1) as f64;
            if el.z != z {
                fixes.push(El { z, ..(***el).clone() });
            }
        }
        if !fixes.is_empty() {
            self.put(fixes);
        }
    }

    /// Forgets folders that no element belongs to any more.
    fn prune_groups(&mut self) {
        let used: HashSet<String> = self.els.values().filter_map(|e| e.group_id.clone()).collect();
        let stale: Vec<String> = {
            let txn = self.doc.transact();
            self.groups.keys(&txn).filter(|k| !used.contains(*k)).map(str::to_string).collect()
        };
        if !stale.is_empty() {
            self.transact(|e| {
                for k in &stale {
                    e.maps.2.remove(&mut e.txn, k);
                }
            });
        }
    }

    /* ---------- voting ---------- */

    /// Votes each, and whether the session has ended; None when no vote is on.
    pub fn voting(&self) -> Option<(u32, bool)> {
        let txn = self.doc.transact();
        let per = self.votes.get(&txn, "per").and_then(|v| out_json(&v)).and_then(|v| v.as_f64()).filter(|p| *p > 0.0)?;
        let ended = self.votes.get(&txn, "ended").and_then(|v| out_json(&v)).and_then(|v| v.as_bool()).unwrap_or(false);
        Some((per.min(50.0) as u32, ended))
    }

    pub fn start_voting(&mut self, per: u32) {
        self.transact_plain(|txn, b| {
            b.votes.clear(txn);
            b.votes.insert(txn, "per", per as f64);
        });
    }

    pub fn end_voting(&mut self) {
        self.transact_plain(|txn, b| {
            b.votes.insert(txn, "ended", true);
        });
    }

    pub fn clear_voting(&mut self) {
        self.transact_plain(|txn, b| b.votes.clear(txn));
    }

    /// Votes per element: everyone's, or one voter's.
    pub fn tally(&self, voter: Option<&str>) -> HashMap<String, u32> {
        let txn = self.doc.transact();
        let mut out = HashMap::new();
        for (k, v) in self.votes.iter(&txn) {
            let Some(rest) = k.strip_prefix("v:") else { continue };
            let Some((who, id)) = rest.split_once(':') else { continue };
            let n = out_json(&v).and_then(|v| v.as_f64()).unwrap_or(0.0);
            if n > 0.0 && voter.is_none_or(|w| w == who) && self.els.contains_key(id) {
                *out.entry(id.to_string()).or_insert(0) += n.min(50.0) as u32;
            }
        }
        out
    }

    /// Adds (or with -1 takes back) one of `voter`'s votes on an element, within their allowance.
    pub fn cast_vote(&mut self, voter: &str, id: &str, delta: i32) {
        let Some((per, ended)) = self.voting() else { return };
        if ended || voter.contains(':') {
            return;
        }
        let key = format!("v:{voter}:{id}");
        let mine = self.votes.get(&self.doc.transact(), &key).and_then(|v| out_json(&v)).and_then(|v| v.as_f64()).unwrap_or(0.0) as i32;
        let used: u32 = self.tally(Some(voter)).values().sum();
        if delta > 0 && used >= per {
            return;
        }
        let next = (mine + delta).max(0);
        self.transact_plain(|txn, b| {
            if next > 0 {
                b.votes.insert(txn, key, next as f64);
            } else {
                b.votes.remove(txn, &key);
            }
        });
    }

    /* ---------- timer ---------- */

    pub fn timer_field(&self, k: &str) -> Option<Value> {
        let txn = self.doc.transact();
        self.timer.get(&txn, k).and_then(|v| out_json(&v))
    }
}

/// One local transaction: writes elements, board settings and folder names.
pub struct Edit<'a> {
    pub txn: TransactionMut<'a>,
    maps: (&'a MapRef, &'a MapRef, &'a MapRef),
    touched: HashSet<String>,
}

impl Edit<'_> {
    pub fn put(&mut self, el: &El) {
        let v = serde_json::to_value(el).expect("element serializes");
        self.maps.0.insert(&mut self.txn, el.id.as_str(), json_any(&v));
        self.touched.insert(el.id.clone());
    }
    pub fn remove(&mut self, id: &str) {
        self.maps.0.remove(&mut self.txn, id);
        self.touched.insert(id.to_string());
    }
    pub fn set_meta(&mut self, meta: &BoardMeta) {
        let v = serde_json::to_value(meta).unwrap();
        for (k, v) in v.as_object().unwrap() {
            self.maps.1.insert(&mut self.txn, k.as_str(), json_any(v));
        }
    }
    fn set_group_name(&mut self, id: &str, name: &str) {
        self.maps.2.insert(&mut self.txn, id, json_any(&serde_json::json!({ "id": id, "name": name })));
    }
}

fn same_line(a: &El, b: &El) -> bool {
    let near = |x: f64, y: f64| (x - y).abs() < 1e-6;
    match (a.line(), b.line()) {
        (Some(la), Some(lb)) => near(a.x, b.x) && near(a.y, b.y) && la.points.iter().zip(&lb.points).all(|(p, q)| near(*p as f64, *q as f64)),
        _ => false,
    }
}

fn parse(key: &str, v: &Out) -> Option<El> {
    let el: El = serde_json::from_value(out_json(v)?).ok()?;
    (el.id == key && el.is_valid()).then_some(el)
}

pub fn out_json(v: &Out) -> Option<Value> {
    match v {
        Out::Any(a) => Some(any_json(a)),
        _ => None,
    }
}

pub fn any_json(a: &Any) -> Value {
    match a {
        Any::Null | Any::Undefined | Any::Buffer(_) => Value::Null,
        Any::Bool(b) => Value::Bool(*b),
        Any::Number(Number::Int(i)) => Value::from(*i),
        Any::Number(Number::Float(f)) => serde_json::Number::from_f64(*f).map(Value::Number).unwrap_or(Value::Null),
        Any::String(s) => Value::String(s.to_string()),
        Any::Array(a) => Value::Array(a.iter().map(any_json).collect()),
        Any::Map(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), any_json(v))).collect()),
    }
}

pub fn json_any(v: &Value) -> Any {
    match v {
        Value::Null => Any::Null,
        Value::Bool(b) => Any::Bool(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) if i.abs() <= Number::I64_MAX_SAFE_INTEGER => Any::Number(Number::Int(i)),
            _ => Any::Number(Number::Float(n.as_f64().unwrap_or(0.0))),
        },
        Value::String(s) => Any::String(s.as_str().into()),
        Value::Array(a) => Any::Array(a.iter().map(json_any).collect()),
        Value::Object(m) => Any::Map(Arc::new(m.iter().map(|(k, v)| (k.clone(), json_any(v))).collect())),
    }
}

/* ---------------- sharing the document ---------------- */

/// The whole document as one update (what is stored on disk and sent to newcomers).
pub fn encode_state(doc: &Doc) -> Vec<u8> {
    doc.transact().encode_state_as_update_v1(&yrs::StateVector::default())
}

pub fn apply_update(doc: &Doc, update: &[u8], origin: &str) -> Result<(), String> {
    use yrs::updates::decoder::Decode;
    let u = yrs::Update::decode_v1(update).map_err(|e| e.to_string())?;
    doc.transact_mut_with(origin).apply_update(u).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::tests::{line, rect};
    use crate::model::{Kind, ShapeKind};

    fn board() -> Board {
        Board::new(Doc::new(), Arc::new(|| {}))
    }
    fn r(id: &str, z: f64) -> El {
        El { z, ..rect(id, 0.0, 10.0, 10.0, ShapeKind::Rect) }
    }
    fn ids(v: &[Arc<El>]) -> Vec<&str> {
        v.iter().map(|e| e.id.as_str()).collect()
    }

    #[test]
    fn a_folder_keeps_its_elements_together_and_disappears_when_emptied() {
        let mut b = board();
        b.put([r("a", 0.0), r("x", 1.0), r("b", 2.0), r("y", 3.0)]);
        let g = b.create_group(&["a".into(), "b".into()], None);
        assert_eq!(ids(b.all()), ["x", "a", "b", "y"]);
        assert_eq!(b.group_list().len(), 1);
        assert_eq!(b.group_name(&g), "Cartella 1");
        b.set_group(&["y".into()], Some(&g));
        assert_eq!(ids(&b.members(&g)), ["a", "b", "y"]);
        b.rename_group(&g, "  Schizzi  ");
        assert_eq!(b.group_name(&g), "Schizzi");
        b.set_group(&["a".into(), "b".into(), "y".into()], None);
        assert_eq!(b.group_list().len(), 0);
        assert_eq!(b.groups.len(&b.doc.transact()), 0);
    }

    #[test]
    fn moving_a_shape_drags_its_connectors_along_in_the_same_undo_step() {
        let mut b = board();
        let mut l = line(vec![0.0; 4], 0.0);
        l.id = "l".into();
        l.z = 2.0;
        if let Kind::Line(ln) = &mut l.kind {
            ln.from = Some("a".into());
            ln.to = Some("b".into());
            ln.arrow_end = true;
        }
        b.put([r("a", 0.0), El { x: 300.0, ..r("b", 1.0) }, l]);
        let ends = |b: &Board| {
            let l = b.get("l").unwrap();
            let p = &l.line().unwrap().points;
            [l.x + p[0] as f64, l.y + p[1] as f64, l.x + p[2] as f64, l.y + p[3] as f64]
        };
        assert_eq!(ends(&b), [10.0, 5.0, 300.0, 5.0]);
        b.stop_capturing();
        b.update(&["b".into()], |el| el.y = 100.0);
        assert_eq!(ends(&b).map(|v| v.round()), [10.0, 7.0, 300.0, 103.0]);
        b.undo();
        assert_eq!(ends(&b), [10.0, 5.0, 300.0, 5.0]);
    }

    #[test]
    fn sections_paint_behind_everything_largest_first_and_bad_elements_are_refused() {
        let mut b = board();
        let section = |id: &str, w: f64, z: f64| El { id: id.into(), w, h: w, z, ..El::new(Kind::Section { fill: "#FFFFFF".into() }) };
        b.put([r("r", 0.0), section("small", 50.0, 5.0), section("big", 500.0, 9.0)]);
        assert_eq!(ids(b.paint_order()), ["big", "small", "r"]);
        // A malformed element from someone else is ignored.
        {
            let mut txn = b.doc.transact_mut_with(REMOTE);
            b.elements.insert(&mut txn, "bad", json_any(&serde_json::json!({"id": "bad", "type": "ink", "points": "x"})));
        }
        b.refresh();
        assert!(b.get("bad").is_none());
        assert_eq!(b.len(), 3);
    }

    #[test]
    fn boards_travel_as_yjs_updates() {
        let mut a = board();
        a.put([r("a", 0.0)]);
        a.start_voting(3);
        let state = encode_state(&a.doc);
        let mut b = board();
        apply_update(&b.doc, &state, REMOTE).unwrap();
        b.refresh();
        assert!(b.get("a").is_some());
        assert_eq!(b.voting(), Some((3, false)));
        b.cast_vote("me", "a", 1);
        b.cast_vote("me", "a", 1);
        assert_eq!(b.tally(Some("me")).get("a"), Some(&2));
        // Remote edits are not in our undo history.
        assert!(!b.undo());
    }
}

#[cfg(test)]
mod compat {
    use super::*;
    use crate::model::{FontKind, Kind, ShapeKind};

    /// A board written by the first Tratto (its own Yjs, one element of each kind).
    #[test]
    fn boards_made_by_the_first_tratto_open_unchanged() {
        let doc = Doc::new();
        apply_update(&doc, include_bytes!("../testdata/legacy.ydoc"), REMOTE).unwrap();
        let mut b = Board::new(doc, Arc::new(|| {}));
        assert_eq!(b.len(), 12, "every element parses");
        let ink = b.get("ink1").unwrap().clone();
        assert_eq!(ink.ink().unwrap().points[3], 10.12);
        assert_eq!(ink.erase.len(), 1);
        assert_eq!(ink.opacity, 0.8);
        let star = b.get("sh1").unwrap().clone();
        let s = star.shape().unwrap();
        assert_eq!((s.shape, s.font, s.text.as_deref(), s.dash), (ShapeKind::Star, Some(FontKind::Hand), Some("Ciao"), true));
        assert_eq!(star.group_id.as_deref(), Some("g1"));
        assert_eq!(b.group_name("g1"), "Mia cartella");
        assert!(b.get("tp1").unwrap().line().unwrap().tape);
        assert!(matches!(&b.get("cm1").unwrap().kind, Kind::Comment { thread } if thread[0].text == "Bello!"));
        assert_eq!(b.meta().background, "#26292E");
        assert_eq!(b.meta().grid_size, 48.0);
        assert_eq!(b.tally(None).get("sh1"), Some(&2));
        // Writing an element back keeps the fields the first Tratto reads.
        b.update(&["tx1".into()], |el| el.x += 1.0);
        let v = out_json(&b.elements.get(&b.doc.transact(), "tx1").unwrap()).unwrap();
        assert_eq!(v["fontSize"].as_f64(), Some(24.0));
        assert_eq!(v["fixedWidth"], false);
        assert_eq!(v["type"], "text");
        assert_eq!(v["x"], 1.0);
    }
}
