# emax-notes — Fase 1

Bloc de notas local-first, keyboard-first para Omarchy + Hyprland + Wayland.
Fase 1: ventana única `sourceview5` + markdown, lazy + autosave atómico,
tema Omarchy mínimo, single-instance con `toggle-new` / `toggle-last`.
Ventana 560x780 vertical estilo libreta, flotante centrada, opaca,
con `dim_around` (efecto spotlight: atenúa el resto).

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

`Cargo.lock` ya fija versiones (93 paquetes: `gtk4 0.11.4`, `sourceview5 0.11.2`, …).

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

## Alcance explícito Fase 1 (qué NO hay)

Sin palette Ctrl+K, sin FTS/search, sin watcher permanente (solo re-lectura de
tema por mtime en cada `activate`), sin preview markdown, sin spellcheck,
sin settings UI, sin packaging. Ver `bloc_de_notas.md` (spec completa, 1958 líneas).
