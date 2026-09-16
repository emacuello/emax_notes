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
    font_size: i64,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: "#000000".into(),
            foreground: "#FFFFFF".into(),
            accent: "#888888".into(),
            selection: "#343D41".into(),
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

fn apply_theme(provider: &gtk4::CssProvider, t: &Theme) {
    let css = format!(
        "window, textview, textview text {{ background-color: {}; color: {}; caret-color: {}; font-size: {}pt; }}\n\
         textview selection {{ background-color: {}; color: {}; }}\n",
        t.background, t.foreground, t.accent, t.font_size, t.selection, t.foreground,
    );
    provider.load_from_data(&css);
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
        *s.last_synced.borrow_mut() = Some(text);
        touch_recent(&path);
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
    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    vbox.set_margin_top(12);
    vbox.set_margin_bottom(12);
    vbox.set_margin_start(12);
    vbox.set_margin_end(12);
    let entry = gtk4::SearchEntry::new();
    entry.set_placeholder_text(Some("Type to filter · `>` commands"));
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
    lbl.add_css_class("dim-label");
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
    lbl.add_css_class("dim-label");
    let row = gtk4::ListBoxRow::new();
    row.set_child(Some(&lbl));
    pal.list.append(&row);
    pal.rows.borrow_mut().push(PalRow {
        row,
        item: PalItem::Cmd(cmd),
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
                pal_header(&pal.list, "No favorites yet — use `Toggle favorite`");
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
    window.set_child(Some(&view));

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
        path: RefCell::new(None),
        save_src: RefCell::new(None),
        suppress: Cell::new(false),
        css,
        theme_mtime: RefCell::new(colors_mtime()),
        last_synced: RefCell::new(None),
        dialog_open: Cell::new(false),
        palette: RefCell::new(None),
    });

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
        st.view.buffer().connect_changed(move |_| schedule_save(&s));
    }
    // Esc → hide (descarta vacío sin guardar); Ctrl+N → buffer vacío nuevo.
    {
        let key = gtk4::EventControllerKey::new();
        let s = st.clone();
        key.connect_key_pressed(move |_, keyval, _, mods| {
            use gtk4::gdk::{Key, ModifierType};
            if keyval == Key::Escape {
                flush_or_cancel(&s);
                s.window.set_visible(false);
                return glib::Propagation::Stop;
            }
            if mods.contains(ModifierType::CONTROL_MASK) && (keyval == Key::n || keyval == Key::N) {
                flush_or_cancel(&s);
                if !buffer_text(&s).is_empty() {
                    save_now(&s);
                }
                set_text_silent(&s, "", None);
                s.view.grab_focus();
                return glib::Propagation::Stop;
            }
            if mods.contains(ModifierType::CONTROL_MASK) && (keyval == Key::k || keyval == Key::K) {
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
}
