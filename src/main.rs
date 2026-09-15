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
        .filter(|p| p.extension().map(|x| x == "md").unwrap_or(false))
        .max() // YYYYMMDD-HHMMSS.md ordena lexicográficamente
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
}

type Shared = Rc<State>;

fn buffer_text(s: &Shared) -> String {
    let buf = s.view.buffer();
    let (a, b) = (buf.start_iter(), buf.end_iter());
    buf.text(&a, &b, false).to_string()
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
    *s.path.borrow_mut() = path;
    s.suppress.set(false);
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

fn show_new(s: &Shared) {
    maybe_refresh_theme(s);
    set_text_silent(s, "", None);
    s.window.present();
    s.view.grab_focus();
}

fn show_last(s: &Shared) {
    maybe_refresh_theme(s);
    match last_note() {
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
        s.window.hide();
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

    let st = Rc::new(State {
        window,
        view,
        path: RefCell::new(None),
        save_src: RefCell::new(None),
        suppress: Cell::new(false),
        css,
        theme_mtime: RefCell::new(colors_mtime()),
    });

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
                s.window.hide();
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
            glib::Propagation::Proceed
        });
        st.window.add_controller(key);
    }
    // Cerrar ventana = ocultar, nunca quit (single-instance en fondo).
    {
        let s = st.clone();
        st.window.connect_close_request(move |_| {
            flush_or_cancel(&s);
            s.window.hide();
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
