//! Pure command-palette filtering and command definitions.

use std::path::PathBuf;

pub(crate) type NoteEntry = (PathBuf, String, Option<usize>);

fn recent_rank(index: Option<usize>) -> usize {
    index.unwrap_or(usize::MAX)
}

/// Filters note titles by prefix/substring and preserves recent-note ordering.
pub(crate) fn filter_notes(query: &str, notes: &[NoteEntry]) -> Vec<NoteEntry> {
    let query = query.trim().to_lowercase();
    if query.starts_with('>') {
        return vec![];
    }
    if query.is_empty() {
        let mut entries = notes.to_vec();
        entries.sort_by(|a, b| {
            recent_rank(a.2)
                .cmp(&recent_rank(b.2))
                .then_with(|| a.1.cmp(&b.1))
        });
        return entries;
    }

    let mut hits: Vec<(u8, usize, &NoteEntry)> = notes
        .iter()
        .filter_map(|note| {
            let title = note.1.to_lowercase();
            if title.starts_with(&query) {
                Some((0, recent_rank(note.2), note))
            } else if title.contains(&query) {
                Some((1, recent_rank(note.2), note))
            } else {
                None
            }
        })
        .collect();
    hits.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2 .1.cmp(&b.2 .1))
    });
    hits.into_iter().map(|(_, _, note)| note.clone()).collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cmd {
    NewNote,
    ToggleFavorite,
    ShowFavorites,
    ShowRecent,
    DeleteCurrent,
}

impl Cmd {
    pub(crate) const ALL: [Self; 5] = [
        Self::NewNote,
        Self::ToggleFavorite,
        Self::ShowFavorites,
        Self::ShowRecent,
        Self::DeleteCurrent,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::NewNote => "New note",
            Self::ToggleFavorite => "Toggle favorite",
            Self::ShowFavorites => "Show favorites",
            Self::ShowRecent => "Show recent",
            Self::DeleteCurrent => "Delete current note",
        }
    }

    fn keywords(self) -> &'static str {
        match self {
            Self::NewNote => "new create",
            Self::ToggleFavorite => "fav favorite star",
            Self::ShowFavorites => "fav favorites list",
            Self::ShowRecent => "recent history",
            Self::DeleteCurrent => "delete remove trash",
        }
    }

    pub(crate) fn matches(self, query: &str) -> bool {
        query.is_empty()
            || self.name().to_lowercase().contains(query)
            || self.keywords().contains(query)
    }
}

/// Parses the command-palette `>query` convention.
pub(crate) fn cmd_query(text: &str) -> Option<String> {
    text.strip_prefix('>')
        .map(|rest| rest.trim().to_lowercase())
}
