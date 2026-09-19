//! Note paths and small persistent state stored outside the Markdown files.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use directories::{BaseDirs, UserDirs};
use serde::{Deserialize, Serialize};

pub(crate) fn notes_dir() -> PathBuf {
    let home = UserDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())));
    let dir = home.join("Notes");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn stamp() -> String {
    glib::DateTime::now_local()
        .ok()
        .and_then(|date_time| date_time.format("%Y%m%d-%H%M%S").ok())
        .map_or_else(|| "00000000-000000".into(), |value| value.to_string())
}

pub(crate) fn new_note_path() -> PathBuf {
    let dir = notes_dir();
    let stem = stamp();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    unique_note_path(&dir, &stem, unique)
}

fn unique_note_path(dir: &Path, stem: &str, unique: u128) -> PathBuf {
    let base = dir.join(format!("{stem}.md"));
    if !base.exists() {
        return base;
    }

    let mut suffix = 0u32;
    loop {
        let suffix_text = if suffix == 0 {
            unique.to_string()
        } else {
            format!("{unique}-{suffix}")
        };
        let candidate = dir.join(format!("{stem}-{suffix_text}.md"));
        if !candidate.exists() {
            return candidate;
        }
        suffix = suffix.saturating_add(1);
    }
}

pub(crate) fn is_markdown_file(path: &Path) -> bool {
    path.is_file() && path.extension().is_some_and(|extension| extension == "md")
}

pub(crate) fn last_note() -> Option<PathBuf> {
    std::fs::read_dir(notes_dir())
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_markdown_file(path))
        .max()
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct NotesState {
    pub(crate) recent: Vec<PathBuf>,
    pub(crate) favorites: Vec<PathBuf>,
}

fn state_path() -> PathBuf {
    let base = BaseDirs::new()
        .and_then(|dirs| dirs.state_dir().map(Path::to_path_buf))
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
                .join(".local/state")
        });
    base.join("emax-notes/state.toml")
}

pub(crate) fn load_state() -> NotesState {
    let mut state: NotesState = std::fs::read_to_string(state_path())
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    state.recent.retain(|path| path.is_file());
    state.favorites.retain(|path| path.is_file());
    state.recent.truncate(20);
    state
}

pub(crate) fn save_state(state: &NotesState) {
    let path = state_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match toml::to_string(state) {
        Ok(text) => {
            if let Err(error) = std::fs::write(&path, text) {
                eprintln!("[emax-notes] state write error: {error}");
            }
        }
        Err(error) => eprintln!("[emax-notes] state serialize error: {error}"),
    }
}

pub(crate) fn touch_recent(path: &Path) {
    let mut state = load_state();
    if state.recent.first().is_some_and(|recent| recent == path) {
        return;
    }
    state.recent.retain(|recent| recent != path);
    state.recent.insert(0, path.to_path_buf());
    state.recent.truncate(20);
    save_state(&state);
}

pub(crate) fn toggle_favorite(path: &Path) -> bool {
    let mut state = load_state();
    let favorite = if state.favorites.iter().any(|favorite| favorite == path) {
        state.favorites.retain(|favorite| favorite != path);
        false
    } else {
        state.favorites.push(path.to_path_buf());
        true
    };
    save_state(&state);
    favorite
}

pub(crate) fn remove_from_state(path: &Path) {
    let mut state = load_state();
    state.recent.retain(|recent| recent != path);
    state.favorites.retain(|favorite| favorite != path);
    save_state(&state);
}

#[cfg(test)]
mod tests {
    use super::unique_note_path;

    #[test]
    fn unique_note_path_never_overwrites_a_note() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("stamp.md"), "first").unwrap();

        let first = unique_note_path(dir.path(), "stamp", 42);
        assert_eq!(first.file_name().unwrap(), "stamp-42.md");
        std::fs::write(&first, "second").unwrap();

        let second = unique_note_path(dir.path(), "stamp", 42);
        assert_eq!(second.file_name().unwrap(), "stamp-42-1.md");
    }
}
