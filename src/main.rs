use directories::UserDirs;
use gtk4::gio::prelude::*;
use gtk4::prelude::*;
use sourceview5::prelude::*;
use std::cell::{Cell, RefCell};
use std::io::Write as _;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

// ---------- paths ----------

fn notes_dir() -> PathBuf {
    let home = UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())));
    let dir = home.join("Notes");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn stamp() -> String {
    glib::DateTime::now_local()
        .ok()
        .and_then(|dt| dt.format("%Y%m%d-%H%M%S").ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "00000000-000000".into())
}

fn new_note_path() -> PathBuf {
    notes_dir().join(format!("{}.md", stamp()))
}

fn last_note() -> Option<PathBuf> {
    let dir = notes_dir();
    std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().map(|x| x == "md").unwrap_or(false)) // flat Fase 2: ignora subdirectorios
        .max() // YYYYMMDD-HHMMSS.md ordena lexicográficamente
}

// ---------- título derivado (Fase 2, spec #20, fn pura) ----------

fn truncate_title(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let mut t: String = s.chars().take(max).collect();
        t.push('…');
        t
    } else {
        s.to_string()
    }
}

/// Primer heading (`# `, `## `…: strip `#` + trim) → si no hay, primera línea
/// no vacía (trim, máx 60 + `…`) → si vacío, `"Untitled"`.
/// Sin frontmatter en Fase 2: un `---` inicial cuenta como línea normal.
fn derive_title(text: &str) -> String {
    let mut fallback: Option<&str> = None;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with('#') {
            let h = t.trim_start_matches('#').trim();
            if !h.is_empty() {
                return h.to_string();
            }
            continue;
        }
        if fallback.is_none() {
            fallback = Some(t);
        }
    }
    match fallback {
        Some(f) => truncate_title(f, 60),
        None => "Untitled".into(),
    }
}

// ---------- theme (Fase 1: mínimo, tolerante, re-lee por mtime) ----------

#[derive(Clone)]
struct Theme {
    background: String,
    foreground: String,
    accent: String,
    selection: String,
    muted: String,
    font_size: i64,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: "#000000".into(),
            foreground: "#FFFFFF".into(),
            accent: "#888888".into(),
            selection: "#343D41".into(),
            muted: "#4B4E55".into(),
            font_size: 12,
        }
    }
}

fn theme_files() -> (PathBuf, PathBuf) {
    let home = UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())));
    (
        home.join(".local/state/omarchy/current/theme/colors.toml"),
        home.join(".local/state/omarchy/current/theme/shell.toml"),
    )
}

fn colors_mtime() -> Option<SystemTime> {
    let (colors, _) = theme_files();
    std::fs::metadata(colors).and_then(|m| m.modified()).ok()
}

fn load_theme() -> Theme {
    let mut t = Theme::default();
    let (colors_path, shell_path) = theme_files();
    if let Ok(txt) = std::fs::read_to_string(&colors_path) {
        if let Ok(v) = txt.parse::<toml::Value>() {
            let get = |k: &str| v.get(k).and_then(|x| x.as_str()).map(|s| s.to_string());
            if let Some(s) = get("background") {
                t.background = s;
            }
            if let Some(s) = get("foreground").or_else(|| get("fg")) {
                t.foreground = s;
            }
            if let Some(s) = get("accent") {
                t.accent = s;
            }
            if let Some(s) = get("selection").or_else(|| get("selection_background")) {
                t.selection = s;
            }
            if let Some(s) = get("muted") {
                t.muted = s;
            }
        }
    }
    if let Ok(txt) = std::fs::read_to_string(&shell_path) {
        if let Ok(v) = txt.parse::<toml::Value>() {
            if let Some(n) = v
                .get("font")
                .and_then(|f| f.get("base-size"))
                .and_then(|x| x.as_integer())
            {
                if (8..=32).contains(&n) {
                    t.font_size = n;
                }
            }
        }
    }
    t
}

fn theme_css(t: &Theme) -> String {
    format!(
        "window, textview, textview text {{ background-color: {}; color: {}; caret-color: {}; font-size: {}pt; }}\n\
         textview selection {{ background-color: {}; color: {}; }}\n\
         .emax-palette {{ background-color: {}; }}\n\
         .emax-palette scrolledwindow, .emax-palette list {{ background-color: transparent; }}\n\
         .emax-palette row {{ padding: 6px 10px; border-radius: 8px; }}\n\
         .emax-palette row:selected {{ background-color: {}; }}\n\
         .emax-palette row:selected label {{ color: {}; }}\n\
         .emax-section {{ color: {}; font-weight: bold; font-size: smaller; }}\n\
         .emax-title {{ color: {}; }}\n\
         .emax-snippet {{ color: {}; font-size: smaller; }}\n\
         .emax-cmd {{ color: {}; }}\n\
         .emax-entry {{ background-color: {}; color: {}; caret-color: {}; font-size: {}pt; \
         border: 1px solid {}; border-radius: 8px; padding: 6px 8px; }}\n\
         .emax-entry:focus {{ border-color: {}; outline: none; }}\n\
         .emax-find {{ background-color: {}; border: 1px solid {}; border-radius: 8px; padding: 6px; }}\n\
         .emax-find entry {{ background-color: transparent; color: {}; }}\n\
         .emax-count {{ color: {}; font-size: smaller; }}\n",
        t.background,
        t.foreground,
        t.accent,
        t.font_size,
        t.selection,
        t.foreground,
        t.background,
        t.selection,
        t.foreground,
        t.muted,
        t.foreground,
        t.accent,
        t.accent,
        t.background,
        t.foreground,
        t.accent,
        t.font_size,
        t.muted,
        t.accent,
        t.background,
        t.muted,
        t.foreground,
        t.muted,
    )
}

fn apply_theme(provider: &gtk4::CssProvider, t: &Theme) {
    provider.load_from_data(&theme_css(t));
}

// ---------- app state ----------

struct State {
    window: gtk4::ApplicationWindow,
    view: sourceview5::View,
    path: RefCell<Option<PathBuf>>,
    save_src: RefCell<Option<glib::SourceId>>,
    suppress: Cell<bool>,
    css: gtk4::CssProvider,
    theme_mtime: RefCell<Option<SystemTime>>,
    last_synced: RefCell<Option<String>>, // buffer tal como quedó en disco (save/load)
    dialog_open: Cell<bool>,              // un solo diálogo de conflicto a la vez
    palette: RefCell<Option<PaletteUi>>,  // Some solo mientras la palette está abierta
    index: RefCell<Option<rusqlite::Connection>>, // FTS5 (main thread); None → palette solo títulos
    overlay: gtk4::Overlay,               // hijo de la ventana; hijo principal = editor o preview
    preview_on: Cell<bool>,
    find: RefCell<Option<FindUi>>, // barra Ctrl+F, solo mientras está abierta
    src: sourceview5::Buffer,      // mismo objeto que view.buffer() (SearchContext pide Buffer)
}

type Shared = Rc<State>;

fn buffer_text(s: &Shared) -> String {
    let buf = s.view.buffer();
    let (a, b) = (buf.start_iter(), buf.end_iter());
    buf.text(&a, &b, false).to_string()
}

fn refresh_title(s: &Shared) {
    s.window.set_title(Some(&derive_title(&buffer_text(s))));
}

fn maybe_refresh_theme(s: &Shared) {
    let m = colors_mtime();
    if m != *s.theme_mtime.borrow() {
        *s.theme_mtime.borrow_mut() = m;
        apply_theme(&s.css, &load_theme());
    }
}

// Guardado atómico: tempfile write → fsync → persist. No crea archivo si vacío.
fn save_now(s: &Shared) {
    *s.save_src.borrow_mut() = None;
    let text = buffer_text(s);
    if text.is_empty() {
        return; // lazy: vacío nunca toca disco
    }
    if s.path.borrow().is_none() {
        *s.path.borrow_mut() = Some(new_note_path());
    }
    let path = s.path.borrow().clone().unwrap();
    let dir = notes_dir();
    let r: Result<(), Box<dyn std::error::Error>> = (|| {
        let mut tmp = tempfile::NamedTempFile::new_in(&dir)?;
        tmp.write_all(text.as_bytes())?;
        tmp.as_file().sync_all()?;
        tmp.persist(&path)
            .map(|_| ())
            .map_err(|e| Box::<dyn std::error::Error>::from(e.error))?;
        Ok(())
    })();
    if let Err(e) = r {
        eprintln!("[emax-notes] save error {path:?}: {e}");
    } else {
        *s.last_synced.borrow_mut() = Some(text.clone());
        touch_recent(&path);
        index_upsert_path(s, &path, &text);
    }
}

fn schedule_save(s: &Shared) {
    if s.suppress.get() {
        return;
    }
    if buffer_text(s).is_empty() {
        if let Some(id) = s.save_src.borrow_mut().take() {
            id.remove();
        }
        return;
    }
    if let Some(id) = s.save_src.borrow_mut().take() {
        id.remove();
    }
    let weak = Rc::downgrade(s);
    let id = glib::timeout_add_local_once(Duration::from_millis(400), move || {
        if let Some(st) = weak.upgrade() {
            save_now(&st);
            refresh_title(&st); // título con el mismo debounce, barato
        }
    });
    *s.save_src.borrow_mut() = Some(id);
}

fn set_text_silent(s: &Shared, text: &str, path: Option<PathBuf>) {
    s.suppress.set(true);
    if let Some(id) = s.save_src.borrow_mut().take() {
        id.remove();
    }
    s.view.buffer().set_text(text);
    if let Some(p) = &path {
        touch_recent(p); // abrir/mostrar nota → recent[0]
    }
    *s.path.borrow_mut() = path.clone();
    *s.last_synced.borrow_mut() = path.map(|_| text.to_string());
    s.suppress.set(false);
    refresh_title(s);
    if s.preview_on.get() {
        show_preview(s); // re-renderiza; el source no cambió por el preview
    }
}

fn flush_or_cancel(s: &Shared) {
    // Al ocultar: si hay texto pendiente se guarda ya; si vacío se descarta.
    if buffer_text(s).is_empty() {
        if let Some(id) = s.save_src.borrow_mut().take() {
            id.remove();
        }
        *s.path.borrow_mut() = None;
    } else if s.save_src.borrow().is_some() {
        if let Some(id) = s.save_src.borrow_mut().take() {
            id.remove();
        }
        save_now(s);
    }
}

// ---------- file watcher (Fase 2, spec #27-28) ----------

/// Solo importa la nota ABIERTA. Filtro de self-events del autosave atómico:
/// si el contenido en disco == `last_synced` (lo último que escribimos/leímos),
/// el evento es nuestro (rename tempfile→persist) y se ignora. Sin ventanas
/// de tiempo ni PIDs: comparación de contenido, robusta a debounce y races.
fn on_watch_event(s: &Shared, paths: &[PathBuf]) {
    let open = s.path.borrow().clone();
    let Some(path) = open else { return }; // nota nueva sin archivo: creates externos → nada
    if !paths.iter().any(|p| p == &path) {
        return;
    }
    match std::fs::read_to_string(&path) {
        Err(_) => {
            // Delete externo de la abierta: se conserva el buffer;
            // al guardar se recrea el archivo (mismo path).
            eprintln!(
                "[emax-notes] borrado externo, conservo buffer: {}",
                path.display()
            );
        }
        Ok(disk) => {
            if Some(&disk) == s.last_synced.borrow().as_ref() {
                return; // self-event o sin cambios reales
            }
            let pending = s.save_src.borrow().is_some()
                || buffer_text(s) != s.last_synced.borrow().clone().unwrap_or_default();
            if !pending {
                // Sin edición local: reload silencioso (un solo paso de undo,
                // historial previo intacto) + aviso mínimo a stderr.
                set_text_silent(s, &disk, Some(path.clone()));
                eprintln!("[emax-notes] reload externo silencioso: {}", path.display());
            } else {
                ask_conflict(s);
            }
        }
    }
}

/// Conflicto (edición local pendiente + cambio externo): diálogo modal mínimo
/// con 2 botones. Sin Compare en Fase 2.
fn ask_conflict(s: &Shared) {
    if s.dialog_open.get() {
        return; // un solo diálogo; el reload lee disco fresco al confirmar
    }
    s.dialog_open.set(true);
    let dlg = gtk4::AlertDialog::builder()
        .message("La nota cambió en disco")
        .detail("Tenés cambios sin guardar. ¿Recargar la versión externa o conservar la tuya?")
        .buttons(["Reload external", "Keep mine"])
        .build();
    let st = s.clone();
    dlg.choose(
        Some(&s.window),
        None::<&gtk4::gio::Cancellable>,
        move |resp: Result<i32, glib::Error>| {
            st.dialog_open.set(false);
            if resp == Ok(0) {
                // Reload external
                if let Some(path) = st.path.borrow().clone() {
                    match std::fs::read_to_string(&path) {
                        Ok(disk) => {
                            set_text_silent(&st, &disk, Some(path));
                            eprintln!("[emax-notes] conflicto: reload externo");
                        }
                        Err(_) => eprintln!(
                            "[emax-notes] conflicto: el archivo ya no existe, conservo buffer"
                        ),
                    }
                }
            } else {
                // Keep mine (resp == 1; Err o -1 = descartado → también conserva)
                save_now(&st);
                eprintln!("[emax-notes] conflicto: keep mine (guardado)");
            }
        },
    );
}

// ---------- estado XDG (Fase 3): ~/.local/state/emax-notes/state.toml ----------
// Los .md no se tocan: recents/favoritos viven acá, cero ruido git.

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct NotesState {
    recent: Vec<PathBuf>,    // más reciente primero, máx 20
    favorites: Vec<PathBuf>, // sin orden garantizado
}

fn state_path() -> PathBuf {
    let base = directories::BaseDirs::new()
        .and_then(|b| b.state_dir().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
                .join(".local/state")
        });
    base.join("emax-notes/state.toml")
}

fn load_state() -> NotesState {
    let mut st: NotesState = std::fs::read_to_string(state_path())
        .ok()
        .and_then(|txt| toml::from_str(&txt).ok())
        .unwrap_or_default();
    st.recent.retain(|p| p.is_file()); // purga lo que ya no existe
    st.favorites.retain(|p| p.is_file());
    st.recent.truncate(20);
    st
}

fn save_state(st: &NotesState) {
    let path = state_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match toml::to_string(st) {
        Ok(txt) => {
            if let Err(e) = std::fs::write(&path, txt) {
                eprintln!("[emax-notes] state write error: {e}");
            }
        }
        Err(e) => eprintln!("[emax-notes] state serialize error: {e}"),
    }
}

fn touch_recent(path: &PathBuf) {
    let mut st = load_state();
    if st.recent.first() == Some(path) {
        return; // ya es recent[0]: sin IO extra
    }
    st.recent.retain(|p| p != path);
    st.recent.insert(0, path.clone());
    st.recent.truncate(20);
    save_state(&st);
}

fn toggle_favorite(path: &PathBuf) -> bool {
    let mut st = load_state();
    let fav = if st.favorites.iter().any(|p| p == path) {
        st.favorites.retain(|p| p != path);
        false
    } else {
        st.favorites.push(path.clone());
        true
    };
    save_state(&st);
    fav
}

fn remove_from_state(path: &PathBuf) {
    let mut st = load_state();
    st.recent.retain(|p| p != path);
    st.favorites.retain(|p| p != path);
    save_state(&st);
}

// ---------- índice FTS5 (Fase 4): descartable en ~/.cache/emax-notes/search-index/ ----------
// rusqlite bundled trae FTS5. V1: `unicode61 remove_diacritics 2`, SIN porter,
// SIN trigram, SIN prefix=. Los .md son la fuente de verdad; borrar el cache
// nunca pierde notas (rebuild por escaneo). Incremental: save_now + watcher.

fn cache_index_path() -> PathBuf {
    let base = directories::BaseDirs::new()
        .map(|b| b.cache_dir().to_path_buf())
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())).join(".cache")
        });
    base.join("emax-notes/search-index/index.db")
}

fn file_mtime_secs(p: &PathBuf) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn init_db(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS docs(
           id INTEGER PRIMARY KEY,
           path TEXT UNIQUE NOT NULL,
           title TEXT NOT NULL,
           content TEXT NOT NULL,
           updated_at INTEGER NOT NULL);
         CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
           title, content, content='docs', content_rowid='id',
           tokenize='unicode61 remove_diacritics 2');
         CREATE TRIGGER IF NOT EXISTS docs_ai AFTER INSERT ON docs BEGIN
           INSERT INTO notes_fts(rowid, title, content)
           VALUES (new.id, new.title, new.content);
         END;
         CREATE TRIGGER IF NOT EXISTS docs_ad AFTER DELETE ON docs BEGIN
           INSERT INTO notes_fts(notes_fts, rowid, title, content)
           VALUES ('delete', old.id, old.title, old.content);
         END;
         CREATE TRIGGER IF NOT EXISTS docs_au AFTER UPDATE ON docs BEGIN
           INSERT INTO notes_fts(notes_fts, rowid, title, content)
           VALUES ('delete', old.id, old.title, old.content);
           INSERT INTO notes_fts(rowid, title, content)
           VALUES (new.id, new.title, new.content);
         END;",
    )
}

fn upsert_doc(
    conn: &rusqlite::Connection,
    path: &str,
    title: &str,
    content: &str,
    updated: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO docs(path, title, content, updated_at) VALUES (?, ?, ?, ?)
         ON CONFLICT(path) DO UPDATE SET
           title = excluded.title, content = excluded.content,
           updated_at = excluded.updated_at",
        rusqlite::params![path, title, content, updated],
    )
    .map(|_| ())
}

fn delete_doc(conn: &rusqlite::Connection, path: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM docs WHERE path = ?", [path])
        .map(|_| ())
}

/// Escaneo plano de ~/Notes (ignora subdirs, como Fase 2-3).
fn scan_notes() -> Vec<(String, i64)> {
    std::fs::read_dir(notes_dir())
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().map(|x| x == "md").unwrap_or(false))
        .map(|p| {
            let mt = file_mtime_secs(&p);
            (p.to_string_lossy().to_string(), mt)
        })
        .collect()
}

/// Arranque: si la DB no existe se crea; si hay incongruencia con el disco
/// (faltan/sobran docs o cambió un mtime), rebuild completo por escaneo.
fn sync_from_disk(conn: &rusqlite::Connection) {
    use std::collections::HashMap;
    let files = scan_notes();
    let db_rows: Option<HashMap<String, i64>> = conn
        .prepare("SELECT path, updated_at FROM docs")
        .ok()
        .and_then(|mut q| {
            q.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .ok()
            .map(|it| it.filter_map(|r| r.ok()).collect())
        });
    let same = db_rows.as_ref().is_some_and(|db| {
        files.len() == db.len() && files.iter().all(|(p, m)| db.get(p) == Some(m))
    });
    if same {
        return;
    }
    if conn.execute("DELETE FROM docs", []).is_err() {
        return; // DB rota: la palette sigue con títulos; el próximo arranque reintenta
    }
    let mut n = 0;
    for (p, m) in &files {
        let txt = std::fs::read_to_string(p).unwrap_or_default();
        if upsert_doc(conn, p, &derive_title(&txt), &txt, *m).is_ok() {
            n += 1;
        }
    }
    eprintln!("[emax-notes] índice FTS rebuild: {n} docs");
}

fn open_index() -> Option<rusqlite::Connection> {
    let db = cache_index_path();
    if let Some(parent) = db.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return None;
        }
    }
    let conn = rusqlite::Connection::open(&db).ok()?;
    init_db(&conn).ok()?;
    sync_from_disk(&conn);
    Some(conn)
}

/// `docker redis puerto` → `"docker"* AND "redis"* AND "puerto"*`
/// (cada término con sufijo `*`, `"` escapada como `""`).
/// Vacía → None (no toca FTS: recents). `>` no llega acá (cmd_query).
fn build_match(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" AND "))
    }
}

struct ContentHit {
    path: PathBuf,
    title_hl: String,
    snippet: String,
}

fn search_content(
    conn: &rusqlite::Connection,
    m: &str,
    favs_json: &str,
    now: i64,
) -> rusqlite::Result<Vec<ContentHit>> {
    let mut stmt = conn.prepare(
        "SELECT d.path,
                highlight(notes_fts, 0, '<b>', '</b>'),
                snippet(notes_fts, 1, '<b>', '</b>', '…', 30)
         FROM notes_fts JOIN docs d ON d.id = notes_fts.rowid
         WHERE notes_fts MATCH :q
         ORDER BY bm25(notes_fts, 10.0, 5.0)
                  - ((:now - d.updated_at) / 86400.0) * 0.05
                  - CASE WHEN d.path IN (SELECT value FROM json_each(:favs))
                         THEN 2.0 ELSE 0.0 END
         LIMIT 8",
    )?;
    let rows = stmt.query_map(
        rusqlite::named_params! { ":q": m, ":now": now, ":favs": favs_json },
        |row| {
            let p: String = row.get(0)?;
            Ok(ContentHit {
                path: PathBuf::from(p),
                title_hl: row.get(1)?,
                snippet: row.get(2)?,
            })
        },
    )?;
    rows.collect()
}

fn favs_json() -> String {
    let mut out = String::from("[");
    for (i, p) in load_state().favorites.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(
            &p.to_string_lossy()
                .replace('\\', "\\\\")
                .replace('"', "\\\""),
        );
        out.push('"');
    }
    out.push(']');
    out
}

fn search_hits(s: &Shared, m: &str) -> Vec<ContentHit> {
    let idx = s.index.borrow();
    let Some(conn) = idx.as_ref() else {
        return vec![];
    };
    search_content(conn, m, &favs_json(), now_secs()).unwrap_or_else(|e| {
        eprintln!("[emax-notes] fts error: {e}");
        vec![]
    })
}

/// Upsert tras persist OK en save_now.
fn index_upsert_path(s: &Shared, path: &PathBuf, text: &str) {
    let mut idx = s.index.borrow_mut();
    let Some(conn) = idx.as_mut() else { return };
    let key = path.to_string_lossy().to_string();
    if let Err(e) = upsert_doc(conn, &key, &derive_title(text), text, file_mtime_secs(path)) {
        eprintln!("[emax-notes] index upsert error: {e}");
    }
}

/// Upsert/delete incremental por evento del watcher (solo ese doc).
fn index_paths(s: &Shared, paths: &[PathBuf]) {
    let dir = notes_dir();
    let mut idx = s.index.borrow_mut();
    let Some(conn) = idx.as_mut() else { return };
    for p in paths {
        if p.parent() != Some(dir.as_path()) {
            continue;
        }
        if p.extension().map(|x| x != "md").unwrap_or(true) {
            continue;
        }
        let key = p.to_string_lossy().to_string();
        match std::fs::read_to_string(p) {
            Ok(txt) => {
                let mt = file_mtime_secs(p);
                if let Err(e) = upsert_doc(conn, &key, &derive_title(&txt), &txt, mt) {
                    eprintln!("[emax-notes] index upsert error: {e}");
                }
            }
            Err(_) => {
                if let Err(e) = delete_doc(conn, &key) {
                    eprintln!("[emax-notes] index delete error: {e}");
                }
            }
        }
    }
}

/// Snippet FTS (marcas `<b>`) → markup Pango seguro: escapa el texto
/// y restaura solo nuestras marcas.
fn fts_markup(s: &str) -> String {
    glib::markup_escape_text(&s.replace("<b>", "\u{1}").replace("</b>", "\u{2}"))
        .replace('\u{1}', "<b>")
        .replace('\u{2}', "</b>")
}

// ---------- filtro puro de la palette (Fase 3, sin FTS) ----------

fn recent_rank(idx: &Option<usize>) -> usize {
    idx.unwrap_or(usize::MAX)
}

/// Substring case-insensitive sobre títulos (sin contenido: eso es Fase 4).
/// Orden: prefix-match > substring; en cada grupo, más reciente primero;
/// desempate por título. Vacía → recents primero. Con `>` → vacío
/// (a nivel palette eso deja solo comandos).
fn filter_notes(
    query: &str,
    notes: &[(PathBuf, String, Option<usize>)],
) -> Vec<(PathBuf, String, Option<usize>)> {
    let q = query.trim().to_lowercase();
    if q.starts_with('>') {
        return vec![];
    }
    if q.is_empty() {
        let mut v = notes.to_vec();
        v.sort_by(|a, b| {
            recent_rank(&a.2)
                .cmp(&recent_rank(&b.2))
                .then_with(|| a.1.cmp(&b.1))
        });
        return v;
    }
    let mut hit: Vec<(u8, usize, &(PathBuf, String, Option<usize>))> = notes
        .iter()
        .filter_map(|n| {
            let t = n.1.to_lowercase();
            if t.starts_with(&q) {
                Some((0, recent_rank(&n.2), n))
            } else if t.contains(&q) {
                Some((1, recent_rank(&n.2), n))
            } else {
                None
            }
        })
        .collect();
    hit.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2 .1.cmp(&b.2 .1))
    });
    hit.into_iter().map(|(_, _, n)| n.clone()).collect()
}

// ---------- comandos (lista cerrada Fase 3) ----------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Cmd {
    NewNote,
    ToggleFavorite,
    ShowFavorites,
    ShowRecent,
    DeleteCurrent,
}

impl Cmd {
    const ALL: [Cmd; 5] = [
        Cmd::NewNote,
        Cmd::ToggleFavorite,
        Cmd::ShowFavorites,
        Cmd::ShowRecent,
        Cmd::DeleteCurrent,
    ];
    fn name(self) -> &'static str {
        match self {
            Cmd::NewNote => "New note",
            Cmd::ToggleFavorite => "Toggle favorite",
            Cmd::ShowFavorites => "Show favorites",
            Cmd::ShowRecent => "Show recent",
            Cmd::DeleteCurrent => "Delete current note",
        }
    }
    fn keywords(self) -> &'static str {
        match self {
            Cmd::NewNote => "new create",
            Cmd::ToggleFavorite => "fav favorite star",
            Cmd::ShowFavorites => "fav favorites list",
            Cmd::ShowRecent => "recent history",
            Cmd::DeleteCurrent => "delete remove trash",
        }
    }
    fn matches(self, q: &str) -> bool {
        q.is_empty() || self.name().to_lowercase().contains(q) || self.keywords().contains(q)
    }
}

/// `>foo` → comandos con `foo`; sin `>` → None (notas + comandos).
fn cmd_query(text: &str) -> Option<String> {
    text.strip_prefix('>')
        .map(|rest| rest.trim().to_lowercase())
}

// ---------- Fase 5: fns puras (checkbox, links, preview spans) ----------

/// `- [ ]` ↔ `- [x]` preservando indentación. Vale `-`, `*`, `+`;
/// `X` mayúscula cuenta como tildado. No-checklist → None.
fn toggle_checkbox_line(line: &str) -> Option<String> {
    let indent_len = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_len);
    let r = rest.as_bytes();
    if r.len() < 5
        || !(r[0] == b'-' || r[0] == b'*' || r[0] == b'+')
        || r[1] != b' '
        || r[2] != b'['
        || r[4] != b']'
    {
        return None;
    }
    let toggled = match r[3] {
        b' ' => 'x',
        b'x' | b'X' => ' ',
        _ => return None,
    };
    let mut out = String::with_capacity(line.len());
    out.push_str(indent);
    out.push(r[0] as char);
    out.push_str(" [");
    out.push(toggled);
    out.push(']');
    out.push_str(&rest[5..]); // seguro: los 5 primeros bytes son ASCII
    Some(out)
}

/// Solo http(s)/mailto; resto se ignora. Corta en whitespace (`"title"`).
fn valid_link_url(url: &str) -> Option<String> {
    let u = url.split_whitespace().next().unwrap_or("");
    if u.starts_with("http://") || u.starts_with("https://") || u.starts_with("mailto:") {
        Some(u.to_string())
    } else {
        None
    }
}

/// URL si `col` (chars) cae dentro de un `[texto](url)` de la línea.
fn link_url_at(line: &str, col: usize) -> Option<String> {
    let cs: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        if cs[i] != '[' {
            i += 1;
            continue;
        }
        let start = i;
        let Some(rel_close) = cs[i..].iter().position(|&c| c == ']') else {
            i += 1;
            continue;
        };
        let close = i + rel_close;
        if cs.get(close + 1) != Some(&'(') {
            i += 1;
            continue;
        }
        let url_start = close + 2;
        let Some(rel_end) = cs[url_start..].iter().position(|&c| c == ')') else {
            i += 1;
            continue;
        };
        let end = url_start + rel_end; // char idx de ')'
        if col >= start && col <= end {
            let url: String = cs[url_start..end].iter().collect();
            return valid_link_url(&url);
        }
        i = end + 1;
    }
    None
}

// ---------- preview: pulldown-cmark → spans (puro, testeable) ----------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SpanKind {
    Text,
    Heading(u8),
    Bold,
    Italic,
    Strike,
    Code,
    Link,
}

#[derive(Clone, Debug)]
struct Span {
    text: String,
    tags: Vec<SpanKind>,
}

/// Markdown → spans con tags. Sin WebView: el caller los vuelca a un
/// TextBuffer read-only con TextTags (headings/bold/italic/listas
/// tasklists/code/links). No hace panic con input arbitrario.
fn preview_spans(md: &str) -> Vec<Span> {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let mut spans: Vec<Span> = vec![];
    let mut stack: Vec<SpanKind> = vec![];
    let mut lists: Vec<Option<u64>> = vec![]; // None = bullets; Some(n) = ordered counter
    let mut item_prefix_pending = false;

    let mut push = |text: &str, tags: &[SpanKind], extra: Option<SpanKind>| {
        if text.is_empty() {
            return;
        }
        let mut t = tags.to_vec();
        if let Some(k) = extra {
            t.push(k);
        }
        if t.is_empty() {
            t.push(SpanKind::Text);
        }
        spans.push(Span {
            text: text.to_string(),
            tags: t,
        });
    };
    for ev in Parser::new_ext(md, opts) {
        match ev {
            Event::Start(Tag::Heading { level, .. }) => {
                stack.push(SpanKind::Heading(level as u8));
            }
            Event::Start(Tag::Strong) => stack.push(SpanKind::Bold),
            Event::Start(Tag::Emphasis) => stack.push(SpanKind::Italic),
            Event::Start(Tag::Strikethrough) => stack.push(SpanKind::Strike),
            Event::Start(Tag::Link { .. }) => stack.push(SpanKind::Link),
            Event::Start(Tag::CodeBlock(_)) => stack.push(SpanKind::Code),
            Event::Start(Tag::List(n)) => lists.push(n),
            Event::Start(Tag::Item) => item_prefix_pending = true,
            Event::Start(_) => {}
            Event::End(TagEnd::Paragraph) => {
                push("\n\n", &stack, None);
            }
            Event::End(TagEnd::Heading(_)) => {
                stack.retain(|k| !matches!(k, SpanKind::Heading(_)));
                push("\n\n", &stack, None);
            }
            Event::End(TagEnd::Item) => {
                item_prefix_pending = false;
                push("\n", &stack, None);
            }
            Event::End(TagEnd::CodeBlock) => {
                stack.retain(|k| *k != SpanKind::Code);
                push("\n", &stack, None);
            }
            Event::End(TagEnd::Link) => {
                stack.retain(|k| *k != SpanKind::Link);
            }
            Event::End(TagEnd::Strong) => {
                stack.retain(|k| *k != SpanKind::Bold);
            }
            Event::End(TagEnd::Emphasis) => {
                stack.retain(|k| *k != SpanKind::Italic);
            }
            Event::End(TagEnd::Strikethrough) => {
                stack.retain(|k| *k != SpanKind::Strike);
            }
            Event::End(_) => {}
            Event::Text(t) => {
                if item_prefix_pending {
                    item_prefix_pending = false;
                    match lists.last().copied() {
                        Some(None) => push("• ", &stack, None),
                        Some(Some(n)) => {
                            let s = format!("{n}. ");
                            lists.pop();
                            lists.push(Some(n + 1));
                            push(&s, &stack, None);
                        }
                        None => {}
                    }
                }
                push(&t, &stack, None);
            }
            Event::Code(t) => {
                push(&t, &stack, Some(SpanKind::Code));
            }
            Event::TaskListMarker(done) => {
                item_prefix_pending = false;
                push(if done { "☑ " } else { "☐ " }, &stack, None);
            }
            Event::SoftBreak | Event::HardBreak => push("\n", &stack, None),
            Event::Rule => push("———\n", &stack, None),
            Event::Html(_) | Event::InlineHtml(_) | Event::FootnoteReference(_) => {}
            Event::InlineMath(t) | Event::DisplayMath(t) => push(&t, &stack, Some(SpanKind::Code)),
        }
    }
    spans
}

// ---------- palette Ctrl+K (overlay temporal, sin estado permanente) ----------

#[derive(Clone, Copy, PartialEq, Eq)]
enum PalMode {
    Normal,
    Favorites,
    Recent,
}

#[derive(Clone)]
enum PalItem {
    Note(PathBuf),
    Cmd(Cmd),
}

struct PalRow {
    row: gtk4::ListBoxRow,
    item: PalItem,
}

struct PaletteUi {
    win: gtk4::Window,
    entry: gtk4::SearchEntry,
    list: gtk4::ListBox,
    rows: RefCell<Vec<PalRow>>,
    selected: Cell<usize>,
    mode: Cell<PalMode>,
    snapshot: Vec<(PathBuf, String)>, // títulos al abrir (search-as-you-type en memoria)
    recents: Vec<PathBuf>,
    favorites: RefCell<Vec<PathBuf>>,
}

fn snapshot_notes() -> Vec<(PathBuf, String)> {
    let mut v: Vec<(PathBuf, String)> = std::fs::read_dir(notes_dir())
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().map(|x| x == "md").unwrap_or(false))
        .map(|p| {
            let t = std::fs::read_to_string(&p)
                .map(|txt| derive_title(&txt))
                .unwrap_or_else(|_| "Untitled".into());
            (p, t)
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0)); // determinista; el orden final lo pone filter_notes
    v
}

fn toggle_palette(s: &Shared) {
    if s.palette.borrow().is_some() {
        close_palette(s);
    } else {
        open_palette(s);
    }
}

fn close_palette(s: &Shared) {
    if let Some(pal) = s.palette.borrow_mut().take() {
        pal.win.close();
    }
    s.view.grab_focus();
}

fn open_palette(s: &Shared) {
    if s.palette.borrow().is_some() {
        return;
    }
    let st = load_state();
    let win = gtk4::Window::builder()
        .transient_for(&s.window)
        .modal(true)
        .title("palette")
        .default_width(480)
        .resizable(false)
        .build();
    win.add_css_class("emax-palette");
    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    vbox.set_margin_top(12);
    vbox.set_margin_bottom(12);
    vbox.set_margin_start(12);
    vbox.set_margin_end(12);
    let entry = gtk4::SearchEntry::new();
    entry.add_css_class("emax-entry");
    entry.set_placeholder_text(Some("Filter notes · > for commands"));
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_min_content_height(320);
    let list = gtk4::ListBox::new();
    list.set_selection_mode(gtk4::SelectionMode::Single);
    scroll.set_child(Some(&list));
    vbox.append(&entry);
    vbox.append(&scroll);
    win.set_child(Some(&vbox));

    *s.palette.borrow_mut() = Some(PaletteUi {
        win: win.clone(),
        entry: entry.clone(),
        list: list.clone(),
        rows: RefCell::new(vec![]),
        selected: Cell::new(0),
        mode: Cell::new(PalMode::Normal),
        snapshot: snapshot_notes(),
        recents: st.recent,
        favorites: RefCell::new(st.favorites),
    });

    // Search-as-you-type: filtra en cada keystroke, sin Enter.
    {
        let w = Rc::downgrade(s);
        entry.connect_changed(move |_| {
            if let Some(st) = w.upgrade() {
                refresh_palette(&st);
            }
        });
    }
    // Enter abre lo seleccionado.
    {
        let w = Rc::downgrade(s);
        entry.connect_activate(move |_| {
            if let Some(st) = w.upgrade() {
                let i = st
                    .palette
                    .borrow()
                    .as_ref()
                    .map(|p| p.selected.get())
                    .unwrap_or(0);
                pal_activate(&st, i);
            }
        });
    }
    // Click abre. Esc/Ctrl+K cierra (stack: sin palette, Esc oculta la app).
    {
        let w = Rc::downgrade(s);
        list.connect_row_activated(move |_, row| {
            if let Some(st) = w.upgrade() {
                let idx = st
                    .palette
                    .borrow()
                    .as_ref()
                    .and_then(|pal| pal.rows.borrow().iter().position(|r| &r.row == row));
                if let Some(i) = idx {
                    pal_activate(&st, i);
                }
            }
        });
    }
    // Un solo controller en la ventana (Capture): vale con foco en entry o lista.
    {
        let key = gtk4::EventControllerKey::new();
        key.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let w = Rc::downgrade(s);
        key.connect_key_pressed(move |_, keyval, _, mods| {
            use gtk4::gdk::{Key, ModifierType};
            let Some(st) = w.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if keyval == Key::Escape
                || (mods.contains(ModifierType::CONTROL_MASK)
                    && (keyval == Key::k || keyval == Key::K))
            {
                close_palette(&st);
                return glib::Propagation::Stop;
            }
            if keyval == Key::Up {
                pal_move(&st, -1);
                return glib::Propagation::Stop;
            }
            if keyval == Key::Down {
                pal_move(&st, 1);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        win.add_controller(key);
    }

    refresh_palette(s);
    win.present();
    entry.grab_focus();
}

fn pal_header(list: &gtk4::ListBox, text: &str) {
    let lbl = gtk4::Label::new(Some(text));
    lbl.set_xalign(0.0);
    lbl.add_css_class("emax-section");
    let row = gtk4::ListBoxRow::new();
    row.set_child(Some(&lbl));
    row.set_selectable(false);
    row.set_activatable(false);
    list.append(&row);
}

fn pal_add_note(pal: &PaletteUi, path: &PathBuf, title: &str, favorites: &[PathBuf]) {
    let label = if favorites.iter().any(|f| f == path) {
        format!("{title} ★")
    } else {
        title.to_string()
    };
    let lbl = gtk4::Label::new(Some(&label));
    lbl.set_xalign(0.0);
    lbl.set_hexpand(true);
    lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    lbl.add_css_class("emax-title");
    let row = gtk4::ListBoxRow::new();
    row.set_child(Some(&lbl));
    pal.list.append(&row);
    pal.rows.borrow_mut().push(PalRow {
        row,
        item: PalItem::Note(path.clone()),
    });
}

fn pal_add_cmd(pal: &PaletteUi, cmd: Cmd) {
    let lbl = gtk4::Label::new(Some(cmd.name()));
    lbl.set_xalign(0.0);
    lbl.add_css_class("emax-cmd");
    let row = gtk4::ListBoxRow::new();
    row.set_child(Some(&lbl));
    pal.list.append(&row);
    pal.rows.borrow_mut().push(PalRow {
        row,
        item: PalItem::Cmd(cmd),
    });
}

fn pal_add_content(pal: &PaletteUi, hit: &ContentHit, favorites: &[PathBuf]) {
    let mut title = fts_markup(&hit.title_hl);
    if favorites.iter().any(|f| f == &hit.path) {
        title.push_str(" ★");
    }
    let title_lbl = gtk4::Label::new(None);
    title_lbl.set_markup(&title);
    title_lbl.set_xalign(0.0);
    title_lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    let snip_lbl = gtk4::Label::new(None);
    snip_lbl.set_markup(&fts_markup(&hit.snippet));
    snip_lbl.set_xalign(0.0);
    snip_lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    snip_lbl.add_css_class("emax-snippet");
    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    vbox.append(&title_lbl);
    vbox.append(&snip_lbl);
    let row = gtk4::ListBoxRow::new();
    row.set_child(Some(&vbox));
    pal.list.append(&row);
    pal.rows.borrow_mut().push(PalRow {
        row,
        item: PalItem::Note(hit.path.clone()),
    });
}

fn pal_title_of(snap: &[(PathBuf, String)], path: &PathBuf) -> String {
    snap.iter()
        .find(|(p, _)| p == path)
        .map(|(_, t)| t.clone())
        .unwrap_or_else(|| "Untitled".into())
}

fn refresh_palette(s: &Shared) {
    let pal_ref = s.palette.borrow();
    let Some(pal) = pal_ref.as_ref() else { return };
    let text = pal.entry.text().to_string();
    let mode = pal.mode.get();
    let snap = pal.snapshot.clone();
    let recents = pal.recents.clone();
    let favorites = pal.favorites.borrow().clone();

    while let Some(ch) = pal.list.first_child() {
        pal.list.remove(&ch);
    }
    pal.rows.borrow_mut().clear();

    let rank = |p: &PathBuf| recents.iter().position(|r| r == p);
    let with_rank: Vec<(PathBuf, String, Option<usize>)> = snap
        .iter()
        .map(|(p, t)| (p.clone(), t.clone(), rank(p)))
        .collect();

    match mode {
        PalMode::Favorites => {
            pal_header(&pal.list, "Favorites");
            let mut n = 0;
            for f in &favorites {
                if f.is_file() {
                    pal_add_note(pal, f, &pal_title_of(&snap, f), &favorites);
                    n += 1;
                }
            }
            if n == 0 {
                pal_header(&pal.list, "No favorites yet");
            }
        }
        PalMode::Recent => {
            pal_header(&pal.list, "Recent");
            for r in &recents {
                if r.is_file() {
                    pal_add_note(pal, r, &pal_title_of(&snap, r), &favorites);
                }
            }
        }
        PalMode::Normal => {
            if let Some(cq) = cmd_query(&text) {
                // `>foo`: solo comandos.
                pal_header(&pal.list, "Commands");
                for c in Cmd::ALL {
                    if c.matches(&cq) {
                        pal_add_cmd(pal, c);
                    }
                }
            } else if text.trim().is_empty() {
                pal_header(&pal.list, "Recent");
                for (p, t, _) in with_rank.iter().filter(|n| n.2.is_some()).take(8) {
                    pal_add_note(pal, p, t, &favorites);
                }
                pal_header(&pal.list, "Commands");
                for c in Cmd::ALL {
                    pal_add_cmd(pal, c);
                }
            } else {
                let notes = filter_notes(&text, &with_rank);
                if !notes.is_empty() {
                    pal_header(&pal.list, "Notes");
                    for (p, t, _) in &notes {
                        pal_add_note(pal, p, t, &favorites);
                    }
                }
                // Debajo: matches de contenido FTS con snippet (dedupe títulos).
                if let Some(m) = build_match(&text) {
                    let shown: Vec<&PathBuf> = notes.iter().map(|(p, _, _)| p).collect();
                    let fresh: Vec<ContentHit> = search_hits(s, &m)
                        .into_iter()
                        .filter(|h| !shown.contains(&&h.path))
                        .collect();
                    if !fresh.is_empty() {
                        pal_header(&pal.list, "Content");
                        for h in &fresh {
                            pal_add_content(pal, h, &favorites);
                        }
                    }
                }
                let q = text.trim().to_lowercase();
                let cmds: Vec<Cmd> = Cmd::ALL.into_iter().filter(|c| c.matches(&q)).collect();
                if !cmds.is_empty() {
                    pal_header(&pal.list, "Commands");
                    for c in cmds {
                        pal_add_cmd(pal, c);
                    }
                }
            }
        }
    }

    pal.selected.set(0);
    let rows = pal.rows.borrow();
    if let Some(first) = rows.first() {
        pal.list.select_row(Some(&first.row));
    }
}

fn pal_move(s: &Shared, delta: isize) {
    let pal_ref = s.palette.borrow();
    let Some(pal) = pal_ref.as_ref() else { return };
    let rows = pal.rows.borrow();
    if rows.is_empty() {
        return;
    }
    let next = (pal.selected.get() as isize + delta).clamp(0, rows.len() as isize - 1) as usize;
    pal.selected.set(next);
    pal.list.select_row(Some(&rows[next].row));
}

fn pal_activate(s: &Shared, idx: usize) {
    let item = s
        .palette
        .borrow()
        .as_ref()
        .and_then(|pal| pal.rows.borrow().get(idx).map(|r| r.item.clone()));
    match item {
        Some(PalItem::Note(p)) => open_note_path(s, &p),
        Some(PalItem::Cmd(c)) => run_command(s, c),
        None => {}
    }
}

fn open_note_path(s: &Shared, path: &PathBuf) {
    close_palette(s);
    match std::fs::read_to_string(path) {
        Ok(txt) => set_text_silent(s, &txt, Some(path.clone())), // touch_recent adentro
        Err(e) => {
            eprintln!("[emax-notes] no se pudo abrir {}: {e}", path.display());
            remove_from_state(path);
            set_text_silent(s, "", None);
        }
    }
    s.window.present();
    s.view.grab_focus();
}

fn run_command(s: &Shared, cmd: Cmd) {
    match cmd {
        Cmd::NewNote => {
            close_palette(s);
            flush_or_cancel(s);
            if !buffer_text(s).is_empty() {
                save_now(s);
            }
            set_text_silent(s, "", None);
            s.window.present();
            s.view.grab_focus();
        }
        Cmd::ToggleFavorite => {
            if let Some(p) = s.path.borrow().clone() {
                let fav = toggle_favorite(&p);
                eprintln!(
                    "[emax-notes] {} favorito: {}",
                    if fav { "★" } else { "☆" },
                    p.display()
                );
                if let Some(pal) = s.palette.borrow().as_ref() {
                    *pal.favorites.borrow_mut() = load_state().favorites;
                }
                refresh_palette(s); // actualiza ★ sin cerrar
            }
        }
        Cmd::ShowFavorites | Cmd::ShowRecent => {
            let mode = if cmd == Cmd::ShowFavorites {
                PalMode::Favorites
            } else {
                PalMode::Recent
            };
            if let Some(pal) = s.palette.borrow().as_ref() {
                pal.mode.set(mode);
                pal.entry.set_text(""); // dispara changed → refresh
            }
        }
        Cmd::DeleteCurrent => {
            let Some(p) = s.path.borrow().clone() else {
                return;
            };
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| p.display().to_string());
            let dlg = gtk4::AlertDialog::builder()
                .message("Delete this note?")
                .detail(&format!("{name} se borra del disco."))
                .buttons(["Delete", "Cancel"])
                .build();
            let parent = s.palette.borrow().as_ref().map(|pal| pal.win.clone());
            let st = s.clone();
            dlg.choose(
                parent.as_ref(),
                None::<&gtk4::gio::Cancellable>,
                move |resp: Result<i32, glib::Error>| {
                    if resp == Ok(0) {
                        if let Err(e) = std::fs::remove_file(&p) {
                            eprintln!("[emax-notes] delete error {}: {e}", p.display());
                        } else {
                            eprintln!("[emax-notes] borrada: {}", p.display());
                        }
                        remove_from_state(&p);
                        close_palette(&st);
                        set_text_silent(&st, "", None);
                        st.window.present();
                        st.view.grab_focus();
                    }
                },
            );
        }
    }
}

// ---------- atajos propios: matching puro keyval/mods (testeable sin GUI) ----------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shortcut {
    ToggleCheckbox, // Ctrl+Enter
    FindOpen,       // Ctrl+F
    TogglePreview,  // Ctrl+Shift+P
    NewNote,        // Ctrl+N (bubble, como hoy: vale con Shift)
    TogglePalette,  // Ctrl+K (bubble, como hoy: vale con Shift)
}

/// Única fuente de verdad para los atajos con Ctrl. Causa raíz del bugfix:
/// el Enter llega como Return, KP_Enter o ISO_Enter según layout/IME
/// (Wayland); solo matchear Return dejaba pasar el evento al TextView,
/// que insertaba el salto de línea. Para Ctrl+Shift+P se acepta `p`/`P`
/// (con Shift exigido): en algunas configs el keysym llega en minúscula.
fn ctrl_shortcut(keyval: gtk4::gdk::Key, mods: gtk4::gdk::ModifierType) -> Option<Shortcut> {
    use gtk4::gdk::{Key, ModifierType};
    if !mods.contains(ModifierType::CONTROL_MASK) {
        return None;
    }
    let shift = mods.contains(ModifierType::SHIFT_MASK);
    match keyval {
        Key::Return | Key::KP_Enter | Key::ISO_Enter => Some(Shortcut::ToggleCheckbox),
        Key::f | Key::F if !shift => Some(Shortcut::FindOpen),
        Key::p | Key::P if shift => Some(Shortcut::TogglePreview),
        Key::n | Key::N => Some(Shortcut::NewNote),
        Key::k | Key::K => Some(Shortcut::TogglePalette),
        _ => None,
    }
}

// ---------- Fase 5 UI: find overlay, preview ----------

struct FindUi {
    bar: gtk4::Box,
    entry: gtk4::SearchEntry,
    count: gtk4::Label,
    ctx: sourceview5::SearchContext,
}

fn find_rebuild_ctx(s: &Shared, text: &str) -> sourceview5::SearchContext {
    let settings = sourceview5::SearchSettings::builder()
        .wrap_around(true)
        .search_text(text)
        .build();
    let ctx = sourceview5::SearchContext::new(&s.src, Some(&settings));
    ctx.set_highlight(true);
    ctx
}

fn find_refresh_count(s: &Shared) {
    let f = s.find.borrow();
    let Some(f) = f.as_ref() else { return };
    let q = f.entry.text();
    if q.is_empty() {
        f.count.set_text("");
        return;
    }
    let n = f.ctx.occurrences_count();
    if n == 0 {
        f.count.set_text("no match");
    } else {
        f.count.set_text(&format!("{n} matches"));
    }
}

/// Salta al siguiente/anterior match desde la selección (o cursor).
fn find_step(s: &Shared, forward: bool) {
    let f = s.find.borrow();
    let Some(f) = f.as_ref() else { return };
    if f.entry.text().is_empty() {
        return;
    }
    let buf = s.view.buffer();
    let it = buf
        .selection_bounds()
        .map(|(a, b)| if forward { b } else { a })
        .unwrap_or_else(|| buf.iter_at_mark(&buf.get_insert()));
    let hit = if forward {
        f.ctx.forward(&it)
    } else {
        f.ctx.backward(&it)
    };
    if let Some((mut ms, me, _)) = hit {
        buf.select_range(&ms, &me);
        s.view.scroll_to_iter(&mut ms, 0.0, false, 0.0, 0.0);
    }
    find_refresh_count(s);
}

fn find_open(s: &Shared) {
    if let Some(f) = s.find.borrow().as_ref() {
        f.entry.grab_focus();
        return;
    }
    let buf = s.view.buffer();
    let init = buf
        .selection_bounds()
        .map(|(a, b)| buf.text(&a, &b, false).to_string())
        .filter(|t| !t.is_empty() && !t.contains('\n'))
        .unwrap_or_default();
    let ctx = find_rebuild_ctx(s, &init);

    let bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    bar.set_halign(gtk4::Align::Center);
    bar.set_valign(gtk4::Align::Start);
    bar.set_margin_top(8);
    bar.add_css_class("osd");
    bar.add_css_class("emax-find");
    let entry = gtk4::SearchEntry::new();
    entry.add_css_class("emax-entry");
    entry.set_width_chars(36);
    entry.set_placeholder_text(Some("Find in note"));
    if !init.is_empty() {
        entry.set_text(&init);
    }
    let count = gtk4::Label::new(None);
    count.add_css_class("emax-count");
    bar.append(&entry);
    bar.append(&count);
    s.overlay.add_overlay(&bar);

    *s.find.borrow_mut() = Some(FindUi {
        bar,
        entry: entry.clone(),
        count,
        ctx,
    });

    // Search-as-you-type: reconstruye contexto (highlight rota con el nuevo).
    {
        let w = Rc::downgrade(s);
        entry.connect_changed(move |e| {
            if let Some(st) = w.upgrade() {
                let text = e.text().to_string();
                if let Some(f) = st.find.borrow_mut().as_mut() {
                    f.ctx.set_highlight(false);
                    f.ctx = find_rebuild_ctx(&st, &text);
                }
                if !text.is_empty() {
                    find_step(&st, true);
                } else {
                    find_refresh_count(&st);
                }
            }
        });
    }
    // Enter = siguiente, Shift+Enter = anterior, Esc = cerrar (stack overlay).
    {
        let key = gtk4::EventControllerKey::new();
        key.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let w = Rc::downgrade(s);
        key.connect_key_pressed(move |_, keyval, _, mods| {
            use gtk4::gdk::{Key, ModifierType};
            let Some(st) = w.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if keyval == Key::Escape {
                find_close(&st);
                return glib::Propagation::Stop;
            }
            if keyval == Key::Return || keyval == Key::KP_Enter {
                find_step(&st, !mods.contains(ModifierType::SHIFT_MASK));
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        entry.add_controller(key);
    }

    find_refresh_count(s);
    if !init.is_empty() {
        find_step(s, true);
    }
    entry.grab_focus();
}

fn find_close(s: &Shared) {
    if let Some(f) = s.find.borrow_mut().take() {
        f.ctx.set_highlight(false);
        s.overlay.remove_overlay(&f.bar);
    }
    if !s.preview_on.get() {
        s.view.grab_focus();
    }
}

/// Ctrl+Enter sobre la línea actual: `- [ ]` ↔ `- [x]` en un solo undo.
/// No-checklist o preview visible → nada.
fn toggle_checkbox_current(s: &Shared) {
    if s.preview_on.get() {
        return;
    }
    let buf = s.view.buffer();
    let ins = buf.iter_at_mark(&buf.get_insert());
    let mut ls = ins.clone();
    ls.set_line_offset(0);
    let mut le = ls.clone();
    le.forward_to_line_end();
    let line = buf.text(&ls, &le, false).to_string();
    if let Some(nl) = toggle_checkbox_line(&line) {
        buf.begin_user_action();
        buf.delete(&mut ls, &mut le);
        buf.insert(&mut ls, &nl);
        buf.end_user_action();
    }
}

// ---------- preview Ctrl+Shift+P: render sin WebView ----------

fn preview_tag_for<'a>(kind: SpanKind, t: &'a PreviewTags) -> Option<&'a gtk4::TextTag> {
    match kind {
        SpanKind::Text => None,
        SpanKind::Heading(1) => t.h1.as_ref(),
        SpanKind::Heading(2) => t.h2.as_ref(),
        SpanKind::Heading(_) => t.h3.as_ref(),
        SpanKind::Bold => t.bold.as_ref(),
        SpanKind::Italic => t.italic.as_ref(),
        SpanKind::Strike => t.strike.as_ref(),
        SpanKind::Code => t.mono.as_ref(),
        SpanKind::Link => t.link.as_ref(),
    }
}

struct PreviewTags {
    h1: Option<gtk4::TextTag>,
    h2: Option<gtk4::TextTag>,
    h3: Option<gtk4::TextTag>,
    bold: Option<gtk4::TextTag>,
    italic: Option<gtk4::TextTag>,
    strike: Option<gtk4::TextTag>,
    mono: Option<gtk4::TextTag>,
    link: Option<gtk4::TextTag>,
}

/// Crea los tags sin panics: las props `weight`/`style` de GtkTextTag son
/// gint (pasar el enum PangoWeight/PangoStyle crasheaba create_tag → NULL
/// → expect → muerte del proceso). Un tag que falle se degrada a texto
/// plano: ningún contenido/tema puede tumbar el preview.
fn preview_make_tags(buf: &gtk4::TextBuffer, accent: &str, muted: &str) -> PreviewTags {
    let tag =
        |name: &str, props: &[(&str, &dyn glib::value::ToValue)]| buf.create_tag(Some(name), props);
    PreviewTags {
        h1: tag("h1", &[("scale", &1.5f64), ("weight", &700i32)]), // PANGO_WEIGHT_BOLD
        h2: tag("h2", &[("scale", &1.3f64), ("weight", &700i32)]),
        h3: tag("h3", &[("scale", &1.15f64), ("weight", &700i32)]),
        bold: tag("bold", &[("weight", &700i32)]),
        italic: tag("italic", &[("style", &gtk4::pango::Style::Italic)]),
        strike: tag("strike", &[("strikethrough", &true)]),
        mono: tag("mono", &[("family", &"monospace"), ("background", &muted)]),
        link: tag("link", &[("foreground", &accent)]),
    }
}

/// Vuelca spans a un buffer read-only. El preview NO toca el source buffer:
/// no dispara autosave ni watcher-loop.
fn render_preview(buf: &gtk4::TextBuffer, md: &str, accent: &str, muted: &str) {
    let t = preview_make_tags(buf, accent, muted);
    for sp in preview_spans(md) {
        let tags: Vec<&gtk4::TextTag> = sp
            .tags
            .iter()
            .filter_map(|k| preview_tag_for(*k, &t))
            .collect();
        buf.insert_with_tags(&mut buf.end_iter(), &sp.text, &tags);
    }
}

fn show_preview(s: &Shared) {
    let theme = load_theme();
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    let tv = gtk4::TextView::new();
    tv.set_editable(false);
    tv.set_cursor_visible(false);
    tv.set_wrap_mode(gtk4::WrapMode::Word);
    tv.set_top_margin(12);
    tv.set_left_margin(12);
    tv.set_right_margin(12);
    tv.set_bottom_margin(12);
    render_preview(&tv.buffer(), &buffer_text(s), &theme.accent, &theme.muted);
    scroll.set_child(Some(&tv));
    s.overlay.set_child(Some(&scroll));
}

fn toggle_preview(s: &Shared) {
    if s.preview_on.get() {
        s.preview_on.set(false);
        s.overlay.set_child(Some(&s.view));
        s.view.grab_focus();
    } else {
        find_close(s);
        show_preview(s);
        s.preview_on.set(true);
    }
}

// ---------- links: Ctrl+click abre http(s)/mailto ----------

fn link_at_point(view: &sourceview5::View, x: f64, y: f64) -> Option<String> {
    let (bx, by) = view.window_to_buffer_coords(gtk4::TextWindowType::Widget, x as i32, y as i32);
    let it = view.iter_at_location(bx, by)?;
    let buf = view.buffer();
    let mut ls = it.clone();
    ls.set_line_offset(0);
    let mut le = ls.clone();
    le.forward_to_line_end();
    let line = buf.text(&ls, &le, false).to_string();
    link_url_at(&line, it.line_offset() as usize)
}

fn show_new(s: &Shared) {
    maybe_refresh_theme(s);
    set_text_silent(s, "", None);
    s.window.present();
    s.view.grab_focus();
}

fn show_last(s: &Shared) {
    maybe_refresh_theme(s);
    // recent[0] si existe (purga en load), fallback a max filename.
    let target = load_state()
        .recent
        .into_iter()
        .next()
        .filter(|p| p.is_file())
        .or_else(last_note);
    match target {
        Some(p) => {
            let txt = std::fs::read_to_string(&p).unwrap_or_default();
            set_text_silent(s, &txt, Some(p));
        }
        None => set_text_silent(s, "", None),
    }
    s.window.present();
    s.view.grab_focus();
}

fn toggle(s: &Shared, want_last: bool) {
    if s.window.is_visible() {
        flush_or_cancel(s);
        s.window.set_visible(false);
    } else if want_last {
        show_last(s);
    } else {
        show_new(s);
    }
}

fn build_ui(app: &gtk4::Application) -> Shared {
    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .title("emax-notes")
        .default_width(560)
        .default_height(780)
        .build();

    let buf = sourceview5::Buffer::new(None);
    buf.set_highlight_syntax(true);
    if let Some(lang) = sourceview5::LanguageManager::default().language("markdown") {
        buf.set_language(Some(&lang));
    }
    let view = sourceview5::View::with_buffer(&buf);
    view.set_wrap_mode(gtk4::WrapMode::Word);
    view.set_show_line_numbers(false);
    view.set_highlight_current_line(false);
    view.set_top_margin(12);
    view.set_left_margin(12);
    view.set_right_margin(12);
    view.set_bottom_margin(12);
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&view)); // hijo principal: editor (o preview en Fase 5)
    window.set_child(Some(&overlay));

    // Tema mínimo aplicado al display.
    let css = gtk4::CssProvider::new();
    gtk4::style_context_add_provider_for_display(&WidgetExt::display(&window), &css, 800);
    apply_theme(&css, &load_theme());

    // File watcher Fase 2: inotify sobre ~/Notes (debouncer 200 ms).
    // El handler de notify corre en otro thread → canal futures (ya en el
    // árbol vía glib, sin polling) consumido por un future en el main loop.
    let (watch_tx, watch_rx) = futures_channel::mpsc::unbounded::<Vec<PathBuf>>();
    let keep_watcher = match notify_debouncer_mini::new_debouncer(
        Duration::from_millis(200),
        move |res: notify_debouncer_mini::DebounceEventResult| match res {
            Ok(evs) => {
                let paths: Vec<PathBuf> = evs.into_iter().map(|e| e.path).collect();
                let _ = watch_tx.unbounded_send(paths);
            }
            Err(e) => eprintln!("[emax-notes] watch error: {e:?}"),
        },
    ) {
        Ok(mut d) => {
            if let Err(e) = d
                .watcher()
                .watch(&notes_dir(), notify::RecursiveMode::Recursive)
            {
                eprintln!("[emax-notes] no se pudo vigilar ~/Notes: {e}");
            }
            Some(d)
        }
        Err(e) => {
            eprintln!("[emax-notes] watcher no disponible: {e}");
            None
        }
    };
    // El watcher debe vivir lo que el proceso (app en fondo tras hide):
    // se olvida a propósito, sin Drop. `ponytail: leak intencional de 1 watcher`.
    if let Some(d) = keep_watcher {
        std::mem::forget(d);
    }

    let st = Rc::new(State {
        window,
        view,
        src: buf,
        path: RefCell::new(None),
        save_src: RefCell::new(None),
        suppress: Cell::new(false),
        css,
        theme_mtime: RefCell::new(colors_mtime()),
        last_synced: RefCell::new(None),
        dialog_open: Cell::new(false),
        palette: RefCell::new(None),
        index: RefCell::new(None),
        overlay,
        preview_on: Cell::new(false),
        find: RefCell::new(None),
    });

    // Índice FTS: abre/crea + sync; si falla, la palette sigue con títulos.
    match open_index() {
        Some(conn) => *st.index.borrow_mut() = Some(conn),
        None => eprintln!("[emax-notes] índice FTS no disponible; palette solo títulos"),
    }

    // Eventos del watcher → solo importan para la nota abierta.
    // Future en el main thread: puede tocar GTK/Rc sin Send.
    {
        let weak = Rc::downgrade(&st);
        glib::spawn_future_local(async move {
            use futures_core::Stream as _;
            let mut rx = watch_rx;
            loop {
                let next =
                    std::future::poll_fn(|cx| std::pin::Pin::new(&mut rx).poll_next(cx)).await;
                let Some(paths) = next else { break }; // watcher caído: termina
                if let Some(s) = weak.upgrade() {
                    index_paths(&s, &paths); // incremental FTS solo de esos docs
                    on_watch_event(&s, &paths);
                } else {
                    break;
                }
            }
        });
    }
    refresh_title(&st);

    // Autosave con debounce.
    {
        let s = st.clone();
        st.view.buffer().connect_changed(move |_| {
            schedule_save(&s);
        });
    }
    // Ctrl+click sobre [texto](url) → browser (solo http(s)/mailto).
    // En Capture para que el click no mueva el cursor/selección al abrir link.
    {
        let click = gtk4::GestureClick::new();
        click.set_button(1);
        click.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let w = Rc::downgrade(&st);
        click.connect_pressed(move |g, _, x, y| {
            use gtk4::gdk::ModifierType;
            let Some(s) = w.upgrade() else { return };
            if s.preview_on.get() {
                return;
            }
            if !g.current_event_state().contains(ModifierType::CONTROL_MASK) {
                return;
            }
            if let Some(url) = link_at_point(&s.view, x, y) {
                if gtk4::gio::AppInfo::launch_default_for_uri(
                    &url,
                    None::<&gtk4::gio::AppLaunchContext>,
                )
                .is_ok()
                {
                    g.set_state(gtk4::EventSequenceState::Claimed);
                }
            }
        });
        st.view.add_controller(click);
    }
    // Atajos propios: el controller va en Capture (el TextView consumiría
    // Return en bubble). Fuente de verdad: ctrl_shortcut (testeado).
    {
        let cap = gtk4::EventControllerKey::new();
        cap.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let s = st.clone();
        cap.connect_key_pressed(move |_, keyval, _, mods| {
            match ctrl_shortcut(keyval, mods) {
                Some(Shortcut::ToggleCheckbox) => {
                    toggle_checkbox_current(&s);
                    glib::Propagation::Stop
                }
                Some(Shortcut::FindOpen) => {
                    find_open(&s);
                    glib::Propagation::Stop
                }
                Some(Shortcut::TogglePreview) => {
                    toggle_preview(&s);
                    glib::Propagation::Stop
                }
                // NewNote/TogglePalette los resuelve el controller bubble (sin cambios).
                _ => glib::Propagation::Proceed,
            }
        });
        st.window.add_controller(cap);
    }
    // Esc → hide (descarta vacío sin guardar); Ctrl+N → buffer vacío nuevo.
    {
        let key = gtk4::EventControllerKey::new();
        let s = st.clone();
        key.connect_key_pressed(move |_, keyval, _, mods| {
            use gtk4::gdk::Key;
            if keyval == Key::Escape {
                if s.find.borrow().is_some() {
                    find_close(&s); // stack: overlay > palette > hide app
                    return glib::Propagation::Stop;
                }
                flush_or_cancel(&s);
                s.window.set_visible(false);
                return glib::Propagation::Stop;
            }
            if matches!(ctrl_shortcut(keyval, mods), Some(Shortcut::NewNote)) {
                flush_or_cancel(&s);
                if !buffer_text(&s).is_empty() {
                    save_now(&s);
                }
                set_text_silent(&s, "", None);
                s.view.grab_focus();
                return glib::Propagation::Stop;
            }
            if matches!(ctrl_shortcut(keyval, mods), Some(Shortcut::TogglePalette)) {
                toggle_palette(&s); // abierta → la cierra
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        st.window.add_controller(key);
    }
    // Cerrar ventana = ocultar, nunca quit (single-instance en fondo).
    {
        let s = st.clone();
        st.window.connect_close_request(move |_| {
            flush_or_cancel(&s);
            s.window.set_visible(false);
            glib::Propagation::Stop
        });
    }
    // No present() aquí: lo controla toggle()/show_new()/show_last().
    // Si se presenta al construir, el primer toggle-new lo oculta al instante.
    st
}

fn main() {
    let t0 = Instant::now();
    let app = gtk4::Application::builder()
        .application_id("dev.emax.notes")
        .flags(gtk4::gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    let _hold = app.hold(); // seguir vivo tras hide()

    let ui: Rc<RefCell<Option<Shared>>> = Rc::new(RefCell::new(None));
    let ensure_ui: Rc<dyn Fn(&gtk4::Application) -> Shared> =
        Rc::new(move |app: &gtk4::Application| {
            if ui.borrow().is_none() {
                *ui.borrow_mut() = Some(build_ui(app));
                println!(
                    "[emax-notes] window.present en {:?} (objetivo <400ms cold)",
                    t0.elapsed()
                );
            }
            ui.borrow().clone().unwrap()
        });

    // Activación sin args (incl. 2ª instancia sin args): solo present().
    {
        let ensure_ui = ensure_ui.clone();
        app.connect_activate(move |app| {
            let s = ensure_ui(app);
            maybe_refresh_theme(&s);
            s.window.present();
            s.view.grab_focus();
        });
    }
    // CLI: `emax-notes toggle-new | toggle-last`
    {
        let ensure_ui = ensure_ui.clone();
        app.connect_command_line(move |app, cl| {
            let args: Vec<String> = cl
                .arguments()
                .iter()
                .map(|a| a.to_string_lossy().to_string())
                .collect();
            let want = args
                .iter()
                .find(|a| *a == "toggle-new" || *a == "toggle-last")
                .cloned();
            match want.as_deref() {
                Some("toggle-last") => toggle(&ensure_ui(app), true),
                Some(_) => toggle(&ensure_ui(app), false), // toggle-new
                None => app.activate(),                    // 2ª instancia pelada → solo present()
            }
            0.into()
        });
    }

    // GAcciones del mismo nombre para D-Bus (`gapplication action dev.emax.notes toggle-new`).
    for name in ["toggle-new", "toggle-last"] {
        let ensure_ui = ensure_ui.clone();
        let app_ref = app.clone();
        let want_last = name == "toggle-last";
        let a = gtk4::gio::SimpleAction::new(name, None);
        a.connect_activate(move |_, _| toggle(&ensure_ui(&app_ref), want_last));
        app.add_action(&a);
    }

    std::process::exit(app.run().into());
}

#[cfg(test)]
mod tests {
    use super::derive_title;

    #[test]
    fn heading() {
        assert_eq!(derive_title("# Hola\ncuerpo de la nota"), "Hola");
    }

    #[test]
    fn sub_heading() {
        assert_eq!(derive_title("intro\n## Subtítulo\nmás texto"), "Subtítulo");
    }

    #[test]
    fn heading_con_espacios() {
        assert_eq!(derive_title("#    Espaciado   \nx"), "Espaciado");
    }

    #[test]
    fn linea_simple() {
        assert_eq!(derive_title("comprar pan"), "comprar pan");
    }

    #[test]
    fn multiline_con_blancos() {
        assert_eq!(derive_title("\n  \n  primera  \nsegunda"), "primera");
    }

    #[test]
    fn vacio() {
        assert_eq!(derive_title(""), "Untitled");
        assert_eq!(derive_title("  \n \n\t\n"), "Untitled");
    }

    #[test]
    fn linea_larga_truncada() {
        let larga = "x".repeat(70);
        assert_eq!(derive_title(&larga), format!("{}…", "x".repeat(60)));
    }

    #[test]
    fn exacta_60_no_trunca() {
        let justa = "y".repeat(60);
        assert_eq!(derive_title(&justa), justa);
    }

    #[test]
    fn heading_gana_a_parrafo_previo() {
        assert_eq!(
            derive_title("párrafo primero\n# Título real"),
            "Título real"
        );
    }

    #[test]
    fn sin_frontmatter_en_fase_2() {
        // Literal: el `---` inicial cuenta como primera línea no vacía…
        assert_eq!(derive_title("---\ntitle: x"), "---");
        // …pero un heading posterior sigue teniendo prioridad.
        assert_eq!(derive_title("---\ntitle: x\n---\n# Real"), "Real");
    }

    // ---------- Fase 3: filter_notes ----------

    use super::{cmd_query, filter_notes, Cmd};
    use std::path::PathBuf;

    fn n(name: &str, title: &str, recent: Option<usize>) -> (PathBuf, String, Option<usize>) {
        (PathBuf::from(name), title.to_string(), recent)
    }

    fn titles(v: &[(PathBuf, String, Option<usize>)]) -> Vec<&str> {
        v.iter().map(|(_, t, _)| t.as_str()).collect()
    }

    #[test]
    fn vacia_da_recents() {
        let notes = vec![
            n("b.md", "Bravo", Some(1)),
            n("a.md", "Alpha", Some(0)),
            n("c.md", "Charlie", None),
        ];
        assert_eq!(
            titles(&filter_notes("", &notes)),
            ["Alpha", "Bravo", "Charlie"]
        );
    }

    #[test]
    fn prefix_gana_a_recency() {
        let notes = vec![
            n("old.md", "my docker notes", Some(0)),
            n("new.md", "docker setup", Some(5)),
        ];
        assert_eq!(
            titles(&filter_notes("doc", &notes)),
            ["docker setup", "my docker notes"]
        );
    }

    #[test]
    fn case_insensitive() {
        let notes = vec![n("d.md", "Docker Compose", Some(0))];
        assert_eq!(titles(&filter_notes("DOCKER", &notes)).len(), 1);
        assert_eq!(titles(&filter_notes(" compose ", &notes)).len(), 1);
    }

    #[test]
    fn mayor_solo_comandos() {
        let notes = vec![n("f.md", "fav things", Some(0))];
        assert!(filter_notes(">fav", &notes).is_empty());
        assert_eq!(cmd_query(">fav"), Some("fav".into()));
        assert_eq!(cmd_query("fav"), None);
    }

    #[test]
    fn sin_match() {
        let notes = vec![n("a.md", "Alpha", Some(0))];
        assert!(filter_notes("zzz", &notes).is_empty());
    }

    #[test]
    fn desempate_por_recency_en_substring() {
        let notes = vec![
            n("old.md", "notas de docker viejas", Some(3)),
            n("new.md", "más docker acá", Some(0)),
        ];
        assert_eq!(
            titles(&filter_notes("docker", &notes)),
            ["más docker acá", "notas de docker viejas"]
        );
    }

    #[test]
    fn comandos_matchean() {
        assert!(Cmd::ToggleFavorite.matches("fav"));
        assert!(Cmd::ShowFavorites.matches("fav"));
        assert!(!Cmd::NewNote.matches("fav"));
        assert!(Cmd::NewNote.matches(""));
        assert!(Cmd::DeleteCurrent.matches("trash"));
    }

    // ---------- Fase 4: builder + recall FTS ----------

    use super::{build_match, delete_doc, init_db, search_content, upsert_doc, ContentHit};
    fn mem_index(docs: &[(&str, &str, &str)]) -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        for (path, title, content) in docs {
            upsert_doc(&conn, path, title, content, 1_700_000_000).unwrap();
        }
        conn
    }

    fn search_paths(conn: &rusqlite::Connection, q: &str) -> Vec<String> {
        let m = build_match(q).unwrap();
        search_content(conn, &m, "[]", 1_700_000_100)
            .unwrap()
            .into_iter()
            .map(|h: ContentHit| h.path.to_string_lossy().to_string())
            .collect()
    }

    #[test]
    fn builder_multi_termino() {
        assert_eq!(
            build_match("docker redis puerto"),
            Some("\"docker\"* AND \"redis\"* AND \"puerto\"*".into())
        );
    }

    #[test]
    fn builder_escape_comillas() {
        assert_eq!(build_match("di\"ce"), Some("\"di\"\"ce\"*".into()));
    }

    #[test]
    fn builder_vacio() {
        assert_eq!(build_match(""), None);
        assert_eq!(build_match("   "), None);
    }

    #[test]
    fn builder_un_termino() {
        assert_eq!(build_match("docker"), Some("\"docker\"*".into()));
    }

    #[test]
    fn fts_diacritics() {
        let conn = mem_index(&[(
            "/n/recetas.md",
            "Recetas",
            "La configuración del horno a 180 grados.",
        )]);
        assert_eq!(search_paths(&conn, "configuracion"), ["/n/recetas.md"]);
    }

    #[test]
    fn fts_prefix() {
        let conn = mem_index(&[("/n/d.md", "Docker", "Contenedores y compose.")]);
        assert_eq!(search_paths(&conn, "dock"), ["/n/d.md"]);
    }

    #[test]
    fn fts_multi_termino_sin_substring_exacto() {
        let conn = mem_index(&[(
            "/n/infra.md",
            "Infra",
            "Redis está expuesto en el puerto 6379 del compose.",
        )]);
        // "redis 6379" no aparece contiguo, pero ambos términos sí.
        assert_eq!(search_paths(&conn, "redis 6379"), ["/n/infra.md"]);
        assert!(search_paths(&conn, "redis mongo").is_empty());
    }

    #[test]
    fn fts_update_incremental() {
        let conn = mem_index(&[("/n/a.md", "A", "texto original")]);
        assert_eq!(search_paths(&conn, "original"), ["/n/a.md"]);
        upsert_doc(&conn, "/n/a.md", "A", "texto editado", 1_700_000_200).unwrap();
        assert!(search_paths(&conn, "original").is_empty());
        assert_eq!(search_paths(&conn, "editado"), ["/n/a.md"]);
    }

    #[test]
    fn fts_delete_purga() {
        let conn = mem_index(&[("/n/a.md", "A", "texto borrable")]);
        assert_eq!(search_paths(&conn, "borrable"), ["/n/a.md"]);
        delete_doc(&conn, "/n/a.md").unwrap();
        assert!(search_paths(&conn, "borrable").is_empty());
    }

    #[test]
    fn fts_snippet_marca_match() {
        let conn = mem_index(&[(
            "/n/a.md",
            "Título",
            "El puerto 6379 expone redis en docker.",
        )]);
        let m = build_match("redis").unwrap();
        let hits = search_content(&conn, &m, "[]", 1_700_000_100).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(
            hits[0].snippet.contains("<b>redis</b>"),
            "{}",
            hits[0].snippet
        );
    }

    // ---------- Fase 5: checkbox, links, preview ----------

    use super::{link_url_at, preview_spans, toggle_checkbox_line, SpanKind};

    #[test]
    fn checkbox_ida() {
        assert_eq!(
            toggle_checkbox_line("- [ ] tarea"),
            Some("- [x] tarea".into())
        );
    }

    #[test]
    fn checkbox_vuelta() {
        assert_eq!(
            toggle_checkbox_line("- [x] tarea"),
            Some("- [ ] tarea".into())
        );
        assert_eq!(
            toggle_checkbox_line("- [X] tarea"),
            Some("- [ ] tarea".into())
        );
    }

    #[test]
    fn checkbox_indent_y_variantes() {
        assert_eq!(
            toggle_checkbox_line("  * [ ] ind"),
            Some("  * [x] ind".into())
        );
        assert_eq!(toggle_checkbox_line("+ [x] y"), Some("+ [ ] y".into()));
        assert_eq!(
            toggle_checkbox_line("- [ ] con resto [bracket] ok"),
            Some("- [x] con resto [bracket] ok".into())
        );
    }

    #[test]
    fn checkbox_no_checklist_intacta() {
        assert_eq!(toggle_checkbox_line("texto plano"), None);
        assert_eq!(toggle_checkbox_line("- item sin box"), None);
        assert_eq!(toggle_checkbox_line("- [y] letra rara"), None);
        assert_eq!(toggle_checkbox_line("- [ ]"), Some("- [x]".into()));
    }

    // ---------- bugfix atajos: ctrl_shortcut ----------

    use super::{ctrl_shortcut, Shortcut};

    fn mods(ctrl: bool, shift: bool) -> gtk4::gdk::ModifierType {
        use gtk4::gdk::ModifierType;
        let mut m = ModifierType::empty();
        if ctrl {
            m.insert(ModifierType::CONTROL_MASK);
        }
        if shift {
            m.insert(ModifierType::SHIFT_MASK);
        }
        m
    }

    #[test]
    fn shortcut_enter_todas_variantes() {
        use gtk4::gdk::Key;
        let c = mods(true, false);
        assert_eq!(
            ctrl_shortcut(Key::Return, c),
            Some(Shortcut::ToggleCheckbox)
        );
        assert_eq!(
            ctrl_shortcut(Key::KP_Enter, c),
            Some(Shortcut::ToggleCheckbox)
        );
        assert_eq!(
            ctrl_shortcut(Key::ISO_Enter, c),
            Some(Shortcut::ToggleCheckbox)
        );
        assert_eq!(ctrl_shortcut(Key::Return, mods(false, false)), None);
    }

    #[test]
    fn shortcut_preview_ambos_cases() {
        use gtk4::gdk::Key;
        let cs = mods(true, true);
        assert_eq!(ctrl_shortcut(Key::P, cs), Some(Shortcut::TogglePreview));
        assert_eq!(ctrl_shortcut(Key::p, cs), Some(Shortcut::TogglePreview));
        assert_eq!(ctrl_shortcut(Key::p, mods(true, false)), None);
        assert_eq!(ctrl_shortcut(Key::P, mods(false, true)), None);
    }

    #[test]
    fn shortcut_resto_intacto() {
        use gtk4::gdk::Key;
        assert_eq!(
            ctrl_shortcut(Key::f, mods(true, false)),
            Some(Shortcut::FindOpen)
        );
        assert_eq!(
            ctrl_shortcut(Key::F, mods(true, false)),
            Some(Shortcut::FindOpen)
        );
        assert_eq!(ctrl_shortcut(Key::F, mods(true, true)), None);
        assert_eq!(
            ctrl_shortcut(Key::n, mods(true, false)),
            Some(Shortcut::NewNote)
        );
        assert_eq!(
            ctrl_shortcut(Key::N, mods(true, true)),
            Some(Shortcut::NewNote)
        );
        assert_eq!(
            ctrl_shortcut(Key::k, mods(true, false)),
            Some(Shortcut::TogglePalette)
        );
        assert_eq!(ctrl_shortcut(Key::x, mods(true, false)), None);
        assert_eq!(ctrl_shortcut(Key::a, mods(false, false)), None);
    }

    #[test]
    fn link_validos() {
        let line = "ver [docs](https://example.com/a) por favor";
        assert_eq!(link_url_at(line, 6), Some("https://example.com/a".into()));
        assert_eq!(link_url_at(line, 27), Some("https://example.com/a".into()));
        assert_eq!(
            link_url_at("mail [yo](mailto:yo@x.com) fin", 8),
            Some("mailto:yo@x.com".into())
        );
    }

    #[test]
    fn link_invalidos() {
        let line = "ver [docs](https://example.com/a) fin";
        assert_eq!(link_url_at(line, 0), None);
        assert_eq!(link_url_at(line, 33), None);
        assert_eq!(link_url_at("[ftp](ftp://x.com/f)", 3), None);
        assert_eq!(link_url_at("[rel](/notas/otra)", 3), None);
        assert_eq!(link_url_at("sin links acá", 4), None);
        assert_eq!(link_url_at("[roto](https://x.com", 3), None);
    }

    #[test]
    fn preview_heading_bold() {
        let spans = preview_spans("# Hola\n\nun **bold** y *itálica*.\n");
        let h = spans.iter().find(|s| s.text.contains("Hola")).unwrap();
        assert!(h.tags.contains(&SpanKind::Heading(1)));
        let b = spans.iter().find(|s| s.text == "bold").unwrap();
        assert!(b.tags.contains(&SpanKind::Bold));
        let i = spans.iter().find(|s| s.text == "itálica").unwrap();
        assert!(i.tags.contains(&SpanKind::Italic));
    }

    #[test]
    fn preview_tasklist_y_code() {
        let spans = preview_spans("- [ ] pendiente\n- [x] hecha\n\n`code` fin\n");
        assert!(spans.iter().any(|s| s.text.contains("☐")));
        assert!(spans.iter().any(|s| s.text.contains("☑")));
        let c = spans.iter().find(|s| s.text == "code").unwrap();
        assert!(c.tags.contains(&SpanKind::Code));
    }

    #[test]
    fn preview_no_panic() {
        for md in [
            "",
            "   \n",
            "#",
            "[[[",
            "]((",
            "- [ ]",
            "```\ncode\n```",
            "> quote\n\n| a |\n|---|\n| b |",
        ] {
            let _ = preview_spans(md);
        }
    }

    // ---------- bugfix crash preview: adversariales ----------

    use super::render_preview;
    use gtk4::prelude::TextBufferExt as _;

    #[test]
    fn preview_adversarial_spans() {
        let cases = [
            "",
            "   \n",
            "#",
            "## ",
            "# Hola, ¿cómo estás? ñandú",
            "emoji 🎉🚀 ñ á é í ó ú ü 中文",
            "a & b <c> \"q\" 's' &amp; &lt;",
            "```\ncode sin cerrar",
            "```rust\nfn main() {}\n```\n",
            "````\nfence largo sin cerrar",
            "| a | b |\n|---|---|\n| 1 | 2 |\n| rota |",
            "| sin cierre",
            "- [ ]",
            "> quote",
            "***",
            "---",
            "[a](b)",
            "![i](u)",
            "[t](https://x.com \"título\")",
            "texto con \t tabs",
            "#eading sin espacio",
            "####### siete niveles",
            "para1\nlínea suelta sin blank\npara2?",
        ];
        for md in cases {
            let spans = preview_spans(md);
            let _: Vec<(&str, usize)> = spans
                .iter()
                .map(|s| (s.text.as_str(), s.tags.len()))
                .collect();
        }
    }

    #[test]
    fn preview_nota_grande() {
        let big = "# Título con tildes: configuración\n\n".to_string()
            + &"párrafo con ñ y emoji 🎉 repetido. ".repeat(20000);
        assert!(big.len() > 500_000);
        let spans = preview_spans(&big);
        assert!(!spans.is_empty());
    }

    #[test]
    fn preview_render_headless() {
        if gtk4::init().is_err() {
            eprintln!("SKIP preview_render_headless: sin display");
            return;
        }
        // TextBuffer/TextTag son GObjects sin display: funciona headless.
        let buf = gtk4::TextBuffer::new(None);
        render_preview(
            &buf,
            "# Hola ñ 🎉\n\n**bold** `code` [l](https://x.com)\n",
            "#888888",
            "#4B4E55",
        );
        let (a, b) = (buf.start_iter(), buf.end_iter());
        assert!(buf.text(&a, &b, false).contains("Hola"));
        // Nota nueva sin guardar: render vacío sin panic.
        let buf = gtk4::TextBuffer::new(None);
        render_preview(&buf, "", "#888888", "#4B4E55");
        let (a, b) = (buf.start_iter(), buf.end_iter());
        assert!(buf.text(&a, &b, false).is_empty());
    }

    #[test]
    fn preview_render_colores_basura() {
        if gtk4::init().is_err() {
            eprintln!("SKIP preview_render_colores_basura: sin display");
            return;
        }
        // colors.toml arbitrario no puede tumbar el proceso.
        let buf = gtk4::TextBuffer::new(None);
        render_preview(&buf, "# Hola\ntexto **bold**\n", "notacolor!!!", "");
        let buf = gtk4::TextBuffer::new(None);
        render_preview(&buf, "# Hola\n", "literal raro", "también-malo");
    }

    #[test]
    fn theme_css_usa_tokens() {
        use super::{theme_css, Theme};
        let t = Theme {
            background: "#000000".into(),
            foreground: "#FFFFFF".into(),
            accent: "#888888".into(),
            selection: "#343D41".into(),
            muted: "#4B4E55".into(),
            font_size: 12,
        };
        let css = theme_css(&t);
        for tok in [
            "#000000", "#FFFFFF", "#888888", "#343D41", "#4B4E55", "12pt",
        ] {
            assert!(css.contains(tok), "falta {tok}");
        }
        for cls in [
            ".emax-palette",
            ".emax-entry",
            ".emax-section",
            ".emax-title",
            ".emax-snippet",
            ".emax-cmd",
            ".emax-find",
            ".emax-count",
            "row:selected",
        ] {
            assert!(css.contains(cls), "falta {cls}");
        }
    }

    #[test]
    fn fts_latencia_query_tipica() {
        let mut docs: Vec<(String, String, String)> = vec![];
        for i in 0..50 {
            docs.push((
                format!("/n/{i:03}.md"),
                format!("Nota {i}"),
                format!("Contenido de relleno número {i} con palabras comunes."),
            ));
        }
        docs.push((
            "/n/infra.md".into(),
            "Infra".into(),
            "Redis en puerto 6379 dentro del compose de docker.".into(),
        ));
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        for (p, t, c) in &docs {
            upsert_doc(&conn, p, t, c, 1_700_000_000).unwrap();
        }
        let m = build_match("redis docker").unwrap();
        let t0 = std::time::Instant::now();
        let hits = search_content(&conn, &m, "[]", 1_700_000_100).unwrap();
        let dt = t0.elapsed();
        eprintln!("[fts latency] 51 docs, query 'redis docker': {dt:?}");
        assert_eq!(hits.len(), 1);
        assert!(dt.as_millis() < 500, "query FTS lenta: {dt:?}");
    }
}
