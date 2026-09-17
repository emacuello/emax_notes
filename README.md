# emax-notes — Fase 6

Bloc de notas local-first, keyboard-first para Omarchy + Hyprland + Wayland.
Fase 1: ventana única `sourceview5` + markdown, lazy + autosave atómico,
tema Omarchy mínimo, single-instance con `toggle-new` / `toggle-last`.
Ventana 560x780 vertical estilo libreta, flotante centrada, opaca,
con `dim_around` (efecto spotlight: atenúa el resto).
Fase 2: título derivado del contenido + file watcher inotify con diálogo
de conflicto mínimo.
Fase 3: palette Ctrl+K + estado XDG (recents/favorites).
Fase 4: búsqueda full-text FTS5 (títulos + contenido con snippets).
Fase 5: find en nota, preview renderizado, checklists, links.

## Requisito del sistema

Lib nativa (ya instalada en este host):

```bash
sudo pacman -S gtksourceview5   # extra/gtksourceview5 5.20.0-1
# Equivalente Debian: sudo apt install libgtksourceview-5-dev
```

```bash
cargo build
cargo run -- toggle-new
```

`Cargo.lock` fija versiones (117 paquetes: `gtk4 0.11.4`, `sourceview5 0.11.2`,
`notify 8.2.0`, `notify-debouncer-mini 0.7.0`, …). `futures-channel`/`futures-core`
0.3.34 ya venían en el árbol (transitivos de glib): cero crates nuevos por el canal.

> Nota de versiones: el alcance pedía `gtk4 0.11 + sourceview5 0.10`,
> pero ese par **no resuelve** (`sourceview5 0.10` depende de `gtk4-sys 0.10`
> y cargo aborta con `only one package may specify links = "gtk-4"`).
> Se usó `sourceview5 0.11` (par oficial de `gtk4 0.11`, última compatible
> `0.11.2`). Nada más cambió.

## Hyprland (Lua real de este host, ya aplicado)

En `~/.config/hypr/bindings.lua` (`unbind` primero para pisar el default):

```lua
hl.unbind("SUPER + N")
hl.unbind("SUPER + SHIFT + N")
o.bind("SUPER + N", "Notas: nueva / mostrar-ocultar", "/home/emacuello/personal/emacuello/emax_notes/target/debug/emax-notes toggle-new")
o.bind("SUPER + SHIFT + N", "Notas: última / mostrar-ocultar", "/home/emacuello/personal/emacuello/emax_notes/target/debug/emax-notes toggle-last")
```

Variante con binario instalado (paquete o install manual, § Cierre): misma
idea contra el binario en `PATH`, sin path absoluto:

```lua
o.bind("SUPER + N", "Notas: nueva / mostrar-ocultar", "emax-notes toggle-new")
o.bind("SUPER + SHIFT + N", "Notas: última / mostrar-ocultar", "emax-notes toggle-last")
```

En `~/.config/hypr/hyprland.lua` (el tamaño 560x780 lo pone la app;
`o.window()` matchea por `class` = `app_id` Wayland, no por título):

```lua
o.window("^dev.emax.notes$", {
  float = true,
  center = true,
  opacity = "1.0 1.0",
  tag = "-default-opacity",
  dim_around = true,
})
o.window("^emax-notes$", {  -- fallback por si reporta el nombre del binario
  float = true,
  center = true,
  opacity = "1.0 1.0",
  tag = "-default-opacity",
  dim_around = true,
})
```

En `~/.config/hypr/looknfeel.lua` (fuerza del dim alrededor, default ~0.4):

```lua
hl.config({
  decoration = {
    rounding = 12,
    dim_around = 0.65,
  },
})
```

Nota: Hyprland no tiene "blur del resto" — `dim_around` (dim, no blur) es lo
nativo más cercano y es lo que usan los launchers para el efecto spotlight.

## Cómo probar hide/show (Fase 1)

```bash
# 1. Lanzar (crea ~/Notes si no existe, no crea ningún .md todavía):
cargo run -- toggle-new     # ventana 560x780, cursor listo, buffer vacío

# 2. Escribir un caracter → esperar 400 ms → aparece ~/Notes/YYYYMMDD-HHMMSS.md
ls -lt ~/Notes | head

# 3. Toggle con app visible → oculta sin crear otra ventana:
cargo run -- toggle-new     # hide() (segunda instancia solo reenvía a la primaria vía D-Bus y sale)

# 4. Última nota:
cargo run -- toggle-last    # muestra + carga la .md más reciente; sin notas → buffer vacío

# 5. Dentro de la app:
#    Esc        → hide() (vacío: descarta sin guardar; con texto: flush + hide). Nunca quit.
#    Ctrl+N     → nuevo buffer vacío (descarta el vacío actual sin guardar).
#    X de ventana → hide(), el proceso sigue (app.hold(), CPU ≈ 0 % en reposo).

# 6. Single-instance:
cargo run -- toggle-new & cargo run -- toggle-new   # una sola ventana; la 2ª hace present()/toggle y sale
```

## Medición cold start + RSS

La app imprime a stdout en el primer `present`:

```text
[emax-notes] window.present en 118ms (objetivo <400ms cold)
```

Medir (con la lib del sistema ya instalada):

```bash
/usr/bin/time -v ./target/debug/emax-notes toggle-new 2>&1 | grep -E "wall|Maximum resident|Elapsed"
/usr/bin/time -v ./target/release/emax-notes toggle-new
ps -o rss=,comm= -C emax-notes   # RSS en KiB en reposo tras Esc (oculta)
```

Objetivo Fase 1: `window.present < 400 ms` en cold. RSS se reporta en KiB
(`ps -o rss`) sin objetivo numérico fijado en esta fase.

## Cierre (Fase 6): métricas release + packaging

Cero cambios de conducta o visual: solo medición y packaging
(`packaging/PKGBUILD` + `packaging/dev.emax.notes.desktop`).

### Métricas medidas (mismo host Omarchy, 2026-09-16)

| Métrica | debug | release | gate |
|---|---|---|---|
| cold start (`window.present`, lo imprime la app) | 84.4 ms | 81.5–85.9 ms | <400 ms ✓ |
| RSS en reposo tras ocultar (`ps -o rss`) | 82 144 KiB (~80.2 MiB) | 79 444–80 676 KiB (~77.6–78.8 MiB) | <70 MB ✗ |
| tamaño binario | 95.8 MB | 4.8 MB | — |
| query FTS típica (`cargo test fts_latencia`, 51 docs) | ~484 µs | ~156 µs | <50 ms ✓✓ |

Notas:

- `/usr/bin/time -v` no disponible en este host (paquete `time` sin
  instalar): cold start lo mide la propia app (`t0` en `main` → primer
  `present`) y RSS con `ps -o rss` tras ocultar.
- Release promedia igual cold que debug (~84 ms): domina carga de libs +
  display, no el código.
- **RSS idle supera el gate de 70 MB en ambos perfiles (~+11 % en
  release). Se reporta, no se optimiza** (alcance de la fase).
- `cargo build` (debug) 0 warnings, `cargo test` 45/45,
  `cargo fmt --check` limpio, `cargo build --release` limpio.
  Sin dependencias nuevas.

### Instalación

Manual:

```bash
cargo build --release --locked
sudo install -Dm755 target/release/emax-notes /usr/bin/emax-notes
sudo install -Dm644 packaging/dev.emax.notes.desktop /usr/share/applications/dev.emax.notes.desktop
```

Vía makepkg (desde `packaging/`, compila y empaqueta):

```bash
cd packaging && makepkg -si   # -s resuelve makedepends: cargo, pkgconf, git
```

Detalles Arch (spec #48, sin AppImage/Flatpak):

- `depends=(gtk4 gtksourceview5)`, **sin `sqlite`**: rusqlite va con feature
  `bundled` (sqlite estático compilado por `libsqlite3-sys`; `readelf -d`
  no muestra NEEDED directo a `libsqlite3.so.0` — el `.so` que ve `ldd`
  entra vía `libgtk-4`/`libgtksourceview-5`, ya cubiertas).
- `options=('!lto')`: los CFLAGS de makepkg con `-flto=auto` generan
  objetos LTO de GCC para el sqlite bundled que `rust-lld` no enlaza
  (`undefined symbol: sqlite3_*`); el resto de flags de distro se conserva.
- `.desktop` con `StartupWMClass=dev.emax.notes` (= `application_id`);
  sin icono propio en el repo → stock `accessories-text-editor`
  (provisto por `adwaita-icon-theme`, que ya entra vía `gtk4`).
- `namcap` no disponible en este host: validación manual de campos +
  `desktop-file-validate` OK + `makepkg` construye el paquete
  (`/usr/bin/emax-notes` + `.desktop`) y el binario empaquetado arranca
  (cold ~84 ms verificado).

### Qué NO hay (estado final)

Sin AppImage/Flatpak, sin repo AUR publicado, sin icono propio, sin
`sqlite` en depends (bundled), sin fuzzy/porter/trigram en FTS (V1),
sin rename/move/settings (ver `bloc_de_notas.md`). Para AUR faltaría:
elegir licencia (el repo no tiene archivo `LICENSE`; el PKGBUILD usa
`license=('custom')`), taggear la versión, apuntar `source=` al tarball
del tag con `sha256sums` real y mantener el PKGBUILD en su propio repo
AUR. No se publicó nada.

## Fase 6: polish visual (solo CSS/tags, sin cambios de conducta)

La palette (`Ctrl+K`) ahora usa los tokens del tema Omarchy
(`background/foreground/accent/muted/selection` + `font-size` de
`colors.toml`/`shell.toml`): ventana sobre el fondo de la app, filas con
relleno y esquinas suaves, selección en `selection`, secciones
Recent/Content/Commands en `muted`, títulos en `foreground`, snippets y
comandos en `accent`. Misma receta para el entry de búsqueda y la barra
de find (`Ctrl+F`). Textos visibles sobrios, sin atajos ni flujos nuevos.

## Alcance explícito Fase 5 (qué NO hay)

Sin cambios visuales de diseño (eso es Fase 6), sin rename/move/settings,
sin spellcheck (removido por innecesario), sin popups
de link. FTS sigue V1: sin fuzzy Levenshtein, sin trigram, sin Tantivy.
Ver `bloc_de_notas.md` (spec completa).

## Fase 5: find, preview, checklists, links

- `Ctrl+F`: barra flotante (`SearchEntry` + contador) sobre el editor, solo
  la nota actual (`SearchContext`/`SearchSettings`, highlight de matches).
  Enter = siguiente, Shift+Enter = anterior, Esc cierra (stack: find >
  palette > hide app). Sin panel permanente.
- `Ctrl+Shift+P`: toggle Editor ↔ preview renderizado con `pulldown-cmark`
  (sin WebView) a `TextView` read-only con `TextTag`s: headings con escala,
  bold/italic/strike, listas `•`/`1.`, tasklists `☐`/`☑`, code monoespaciado,
  links en color accent del tema. Reemplaza al editor (no split); el preview
  nunca dispara autosave (buffer separado) ni escribe archivos.
- `Ctrl+Enter`: toggle `- [ ]` ↔ `- [x]` en la línea actual (vale `*`/`+`,
  un solo undo; no-checklist no hace nada).
- `Ctrl+click` en `[texto](url)` **dentro del editor**: abre con el handler
  default vía `gio::AppInfo`. Solo http(s)/mailto; resto ignorado, sin
  subrayados custom ni popups. En el preview los links solo se muestran en
  accent, sin click.

Probar manual:

```bash
cargo run -- toggle-new        # Ctrl+F + Enter navega matches de la nota
# Ctrl+Shift+P alterna render; Ctrl+Enter en `- [ ]` lo tilda
# Ctrl+click en link http abre el browser
```

## Fase 4: búsqueda full-text FTS5

Índice en `~/.cache/emax-notes/search-index/index.db` (`rusqlite` bundled):
tabla `docs` + `notes_fts USING fts5(title, content, …, tokenize='unicode61
remove_diacritics 2')` + triggers INSERT/UPDATE/DELETE. **Descartable**:
borrar la carpeta no pierde notas (rebuild por escaneo al arrancar si hay
incongruencia). Incremental vía `save_now` + eventos del watcher.

Sintaxis de query: `docker redis puerto` → `"docker"* AND "redis"* AND
"puerto"*` (AND por whitespace, `"` escapada). Vacía → recents (sin FTS);
`>foo` → solo comandos. Ranking: `bm25(…, 10.0, 5.0)` (título x2) −
recency lineal − favorito ×2.0 (joineado por path con el state XDG).
La palette muestra títulos primero (lógica intacta) y debajo contenido con
snippet (`<b>` + `…`, 30 tokens) a markup Pango seguro.

Latencia medida (`cargo test fts_latencia -- --nocapture`, 51 docs):

```text
[fts latency] 51 docs, query 'redis docker': 361.994µs
```

Sub-ms a esta escala: sin debounce en search-as-you-type (si alguna vez
mide >50 ms, se mete debounce entonces, no antes).

Probar manual:

```bash
cargo run -- toggle-new        # Ctrl+K + palabra que solo está en el cuerpo
# → la nota aparece bajo "Content" con 1 línea de contexto
# `configuracion` encuentra `configuración`
```

## Fase 3: palette Ctrl+K + estado XDG

Overlay temporal centrado (ventana modal hija, se destruye al cerrar):
`SearchEntry` arriba + `ListBox` abajo. Vacía = Recent (máx 8) + Commands;
con texto filtra títulos (`derive_title`, substring case-insensitive) +
comandos que matcheen; `>foo` solo comandos. Search-as-you-type, Enter abre
en la misma ventana, Esc/Ctrl+K cierra sin ocultar la app. Up/Down navega.
Comandos (lista cerrada): New note, Toggle favorite (★ en la lista),
Show favorites, Show recent, Delete current note (confirma con AlertDialog;
borra del disco + buffer vacío). Rename/Move/Settings/Preview: pendientes.

Estado en `~/.local/state/emax-notes/state.toml` (`recent` máx 20 al
abrir/mostrar/guardar, `favorites`): purga paths inexistentes al cargar.
`toggle-last` usa `recent[0]` con fallback a max filename.

Probar manual:

```bash
cargo run -- toggle-new        # Ctrl+K → vacía muestra recents
# escribir 3 letras → filtra al instante; Enter cambia de nota
# `>fav` → solo comandos; Esc cierra palette sin ocultar app
```

## Fase 2: título derivado + watcher

Título de ventana derivado del contenido (`derive_title`, spec #20): primer
heading Markdown (strip `#` + trim) → si no hay, primera línea no vacía
(trim, máx 60 + `…`) → si vacío, `Untitled`. Sin frontmatter en Fase 2
(`---` inicial cuenta como línea normal). Se actualiza con el mismo debounce
de 400 ms del autosave, más inmediato al mostrar/cargar/recargar nota.
Nota: la regla Hyprland primaria matchea por `class` (`^dev.emax.notes$`),
así que el título dinámico no la rompe.

Watcher (`notify 8` + `notify-debouncer-mini 0.7`, backend inotify, debounce
200 ms) sobre `~/Notes` recursivo pero plano en Fase 2: subdirectorios se
ignoran (`last_note` solo `is_file`, eventos solo del path abierto).
Solo importa la nota abierta: cambio externo sin edición local pendiente
→ reload silencioso (un solo paso de undo) + aviso a stderr; con edición
local pendiente → `AlertDialog` modal con 2 botones, `Reload external` /
`Keep mine` (descartar el diálogo conserva lo tuyo y guarda). Delete externo
→ se conserva el buffer y al guardar se recrea el archivo. Creates externos
→ nada.

Probar manual (reload <500 ms):

```bash
cargo run -- toggle-last          # abre la nota más reciente
nvim ~/Notes/<id>.md              # editar + :w fuera → la app recarga sola
# Conflicto: escribir en la app (sin esperar 400 ms) + :w en nvim
# → aparece el diálogo de 2 opciones
```
