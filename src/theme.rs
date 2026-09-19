//! Omarchy theme loading and CSS generation.

use std::path::PathBuf;
use std::time::SystemTime;

use directories::UserDirs;

#[derive(Clone)]
pub(crate) struct Theme {
    pub(crate) background: String,
    pub(crate) foreground: String,
    pub(crate) accent: String,
    pub(crate) selection: String,
    pub(crate) muted: String,
    pub(crate) font_size: i64,
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
        .map(|dirs| dirs.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())));
    (
        home.join(".local/state/omarchy/current/theme/colors.toml"),
        home.join(".local/state/omarchy/current/theme/shell.toml"),
    )
}

pub(crate) fn colors_mtime() -> Option<SystemTime> {
    let (colors, _) = theme_files();
    std::fs::metadata(colors)
        .and_then(|metadata| metadata.modified())
        .ok()
}

pub(crate) fn load_theme() -> Theme {
    let mut theme = Theme::default();
    let (colors_path, shell_path) = theme_files();
    if let Ok(text) = std::fs::read_to_string(&colors_path) {
        if let Ok(value) = text.parse::<toml::Value>() {
            let get = |key: &str| {
                value
                    .get(key)
                    .and_then(|item| item.as_str())
                    .map(str::to_owned)
            };
            if let Some(color) = get("background") {
                theme.background = color;
            }
            if let Some(color) = get("foreground").or_else(|| get("fg")) {
                theme.foreground = color;
            }
            if let Some(color) = get("accent") {
                theme.accent = color;
            }
            if let Some(color) = get("selection").or_else(|| get("selection_background")) {
                theme.selection = color;
            }
            if let Some(color) = get("muted") {
                theme.muted = color;
            }
        }
    }
    if let Ok(text) = std::fs::read_to_string(&shell_path) {
        if let Ok(value) = text.parse::<toml::Value>() {
            if let Some(size) = value
                .get("font")
                .and_then(|font| font.get("base-size"))
                .and_then(toml::Value::as_integer)
            {
                if (8..=32).contains(&size) {
                    theme.font_size = size;
                }
            }
        }
    }
    theme
}

pub(crate) fn theme_css(theme: &Theme) -> String {
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
        theme.background,
        theme.foreground,
        theme.accent,
        theme.font_size,
        theme.selection,
        theme.foreground,
        theme.background,
        theme.selection,
        theme.foreground,
        theme.muted,
        theme.foreground,
        theme.accent,
        theme.accent,
        theme.background,
        theme.foreground,
        theme.accent,
        theme.font_size,
        theme.muted,
        theme.accent,
        theme.background,
        theme.muted,
        theme.foreground,
        theme.muted,
    )
}

pub(crate) fn apply_theme(provider: &gtk4::CssProvider, theme: &Theme) {
    provider.load_from_data(&theme_css(theme));
}
