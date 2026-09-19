//! SQLite/FTS5 cache operations kept independent from GTK widgets.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use directories::BaseDirs;

use crate::markdown::derive_title;
use crate::notes::{is_markdown_file, load_state, notes_dir};

fn cache_index_path() -> PathBuf {
    let base = BaseDirs::new()
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())).join(".cache")
        });
    base.join("emax-notes/search-index/index.db")
}

pub(crate) fn file_mtime_secs(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs() as i64)
}

fn db_mtime(conn: &rusqlite::Connection, path: &str) -> Option<i64> {
    conn.query_row(
        "SELECT updated_at FROM docs WHERE path = ?",
        [path],
        |row| row.get(0),
    )
    .ok()
}

/// Reports whether the index already reflects a file's current mtime.
pub(crate) fn index_fresh(conn: &rusqlite::Connection, path: &Path) -> bool {
    path.is_file() && db_mtime(conn, &path.to_string_lossy()) == Some(file_mtime_secs(path))
}

pub(crate) fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

pub(crate) fn init_db(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
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

pub(crate) fn upsert_doc(
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

pub(crate) fn delete_doc(conn: &rusqlite::Connection, path: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM docs WHERE path = ?", [path])
        .map(|_| ())
}

fn scan_notes() -> Vec<(String, i64)> {
    std::fs::read_dir(notes_dir())
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_markdown_file(path))
        .map(|path| {
            let modified = file_mtime_secs(&path);
            (path.to_string_lossy().into_owned(), modified)
        })
        .collect()
}

fn sync_from_disk(conn: &rusqlite::Connection) {
    use std::collections::HashMap;

    let files = scan_notes();
    let db_rows: Option<HashMap<String, i64>> = conn
        .prepare("SELECT path, updated_at FROM docs")
        .ok()
        .and_then(|mut query| {
            query
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })
                .ok()
                .map(|rows| rows.filter_map(Result::ok).collect())
        });
    let same = db_rows.as_ref().is_some_and(|db| {
        files.len() == db.len()
            && files
                .iter()
                .all(|(path, mtime)| db.get(path) == Some(mtime))
    });
    if same {
        return;
    }
    if conn.execute("DELETE FROM docs", []).is_err() {
        return;
    }
    let mut indexed = 0;
    for (path, mtime) in &files {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        if upsert_doc(conn, path, &derive_title(&text), &text, *mtime).is_ok() {
            indexed += 1;
        }
    }
    eprintln!("[emax-notes] índice FTS rebuild: {indexed} docs");
}

pub(crate) fn open_index() -> Option<rusqlite::Connection> {
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

/// Builds an FTS5 prefix query from whitespace-separated terms.
pub(crate) fn build_match(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" AND "))
}

pub(crate) struct ContentHit {
    pub(crate) path: PathBuf,
    pub(crate) title_hl: String,
    pub(crate) snippet: String,
}

pub(crate) fn search_content(
    conn: &rusqlite::Connection,
    query: &str,
    favorites_json: &str,
    now: i64,
) -> rusqlite::Result<Vec<ContentHit>> {
    let mut statement = conn.prepare(
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
    let rows = statement.query_map(
        rusqlite::named_params! { ":q": query, ":now": now, ":favs": favorites_json },
        |row| {
            let path: String = row.get(0)?;
            Ok(ContentHit {
                path: PathBuf::from(path),
                title_hl: row.get(1)?,
                snippet: row.get(2)?,
            })
        },
    )?;
    rows.collect()
}

pub(crate) fn favs_json() -> String {
    let mut output = String::from("[");
    for (index, path) in load_state().favorites.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push('"');
        output.push_str(
            &path
                .to_string_lossy()
                .replace('\\', "\\\\")
                .replace('"', "\\\""),
        );
        output.push('"');
    }
    output.push(']');
    output
}
