# emax-notes — Fase 3

Bloc de notas local-first, keyboard-first para Omarchy + Hyprland + Wayland.
Fase 1: ventana única `sourceview5` + markdown, lazy + autosave atómico,
tema Omarchy mínimo, single-instance con `toggle-new` / `toggle-last`.
Ventana 560x780 vertical estilo libreta, flotante centrada, opaca,
con `dim_around` (efecto spotlight: atenúa el resto).
Fase 2: título derivado del contenido + file watcher inotify con diálogo
de conflicto mínimo.
Fase 3: palette Ctrl+K + estado XDG (recents/favorites).

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

## Alcance explícito Fase 3 (qué NO hay)

Sin FTS de contenido hasta Fase 4 (la palette solo filtra títulos),
sin preview markdown (Fase 5), sin rename/move/settings (después),
sin spellcheck, sin packaging. El diálogo de conflicto sigue sin Compare.
Ver `bloc_de_notas.md` (spec completa).

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
