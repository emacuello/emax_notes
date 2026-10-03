# emax-notes

Bloc de notas local. Una ventana GTK4, Markdown en disco y atajos de teclado. Cerrar la ventana la oculta y el proceso sigue en segundo plano.

Está pensado para [Omarchy](https://omarchy.org) sobre Hyprland: de ahí salen los colores, y la ventana está medida para quedar flotante y centrada. El programa en sí no depende de Omarchy ni de Hyprland. Corre en cualquier Linux con GTK 4 y GtkSourceView 5. En otro escritorio se lanzan los mismos comandos, con los atajos de ese entorno. Los colores de Omarchy se leen si existen; si no, usa un tema oscuro fijo.

Las notas son archivos `.md` en `~/Notes`. El título de la ventana sale del primer encabezado; si no hay, de la primera línea.

## Características

- Editor Markdown (GtkSourceView) con ajuste de línea y guardado automático.
- Una sola instancia. La segunda llamada le habla a la que ya corre y termina.
- Paleta para abrir notas, buscar en el contenido y lanzar comandos.
- Búsqueda dentro de la nota abierta, vista previa renderizada y checklists.
- Colores de Omarchy cuando ese tema está instalado. En otro escritorio, tema oscuro fijo.
- Si otro programa edita la nota abierta, se recarga. Con cambios locales sin guardar, pregunta cuál versión conservar.

## Requisitos

Rust (cargo), GTK 4 y GtkSourceView 5.

Arch Linux:

```bash
sudo pacman -S rust gtk4 gtksourceview5
```

Debian y Ubuntu:

```bash
sudo apt install cargo rustc pkg-config libgtk-4-dev libgtksourceview-5-dev
```

## Compilar

```bash
cargo build --release --locked
./target/release/emax-notes toggle-new
```

Durante el desarrollo, `cargo run -- toggle-new` alcanza.

## Instalar

A mano:

```bash
cargo build --release --locked
sudo install -Dm755 target/release/emax-notes /usr/bin/emax-notes
sudo install -Dm644 packaging/dev.emax.notes.desktop /usr/share/applications/dev.emax.notes.desktop
```

En Arch, el `PKGBUILD` de `packaging/` empaqueta el binario y el `.desktop`:

```bash
cd packaging && makepkg -si
```

Depende de `gtk4` y `gtksourceview5`. SQLite va compilado dentro del binario, así que no hace falta el paquete `sqlite`. El `PKGBUILD` desactiva LTO (`options=('!lto')`): si no, el enlace del SQLite embebido falla con los flags de makepkg.

El lanzador usa el icono de sistema `accessories-text-editor` y `StartupWMClass=dev.emax.notes`.

## Uso

```bash
emax-notes toggle-new     # nota nueva; si la ventana ya se ve, la oculta
emax-notes toggle-last    # última nota; mismo toggle si ya se ve
emax-notes                # muestra la ventana
```

Lo mismo por D-Bus, contra la instancia que ya está corriendo:

```bash
gapplication action dev.emax.notes toggle-new
gapplication action dev.emax.notes toggle-last
```

El archivo aparece unos 400 ms después de dejar de escribir, como `~/Notes/YYYYMMDD-HHMMSS.md`. El guardado escribe un temporal en el mismo directorio, hace `fsync` y reemplaza el archivo. Un buffer que sigue vacío no crea archivo.

`Esc` y el botón de cerrar ocultan la ventana. El proceso sigue vivo, así que la próxima llamada no arranca de cero.

### Atajos

| Atajo | Acción |
| --- | --- |
| `Esc` | Cierra la búsqueda o la paleta. Si no hay nada encima, oculta la ventana |
| `Ctrl+N` | Nota nueva |
| `Ctrl+K` | Abre o cierra la paleta |
| `Ctrl+F` | Buscar en la nota actual |
| `Enter` / `Shift+Enter` | Siguiente / anterior coincidencia, con la búsqueda abierta |
| `Ctrl+Shift+P` | Alternar editor y vista previa |
| `Ctrl+Enter` | Marcar o desmarcar la checklist de la línea (`-`, `*` o `+`) |
| `Ctrl+clic` | Abrir un enlace `http`, `https` o `mailto` del editor |

La búsqueda es una barra sobre el editor, solo de la nota actual, y resalta las coincidencias. Si hay una selección de una sola línea, esa selección es el texto inicial.

La vista previa reemplaza al editor y no escribe el archivo. Muestra encabezados, negrita, cursiva, tachado, listas, checklists, código y enlaces. Esos enlaces se ven en el color de acento; el `Ctrl+clic` que abre el navegador vale en el editor.

### Paleta

Vacía muestra hasta 8 notas recientes y la lista de comandos. Con texto filtra títulos (ignora mayúsculas) y, en una sección aparte, el cuerpo de las notas. Un prefijo `>` deja solo comandos: `>fav`, `>del`.

`Enter` abre la fila, `↑` y `↓` se mueven, clic también abre. `Esc` o `Ctrl+K` cierran la paleta y dejan la ventana donde estaba.

Comandos:

- **New note**
- **Toggle favorite** marca la nota abierta con ★
- **Show favorites** / **Show recent**
- **Delete current note** pide confirmación y borra el `.md`

En el cuerpo, cada palabra es un prefijo y se combinan con AND. Las tildes no importan: `configuracion` encuentra `configuración`.

### Notas en disco

Entran los `.md` que están directamente en `~/Notes`. El título es el primer encabezado Markdown. Sin encabezado, la primera línea no vacía, recortada a 60 caracteres. Buffer vacío: `Untitled`.

Si la nota abierta cambia en disco y no hay edición local pendiente, se recarga sola. Si la hay, el diálogo ofrece **Reload external** o **Keep mine**. Si el archivo desaparece, el texto del buffer se queda y al guardar se vuelve a crear.

## Dónde queda cada cosa

| Qué | Dónde |
| --- | --- |
| Notas | `~/Notes/*.md` |
| Recientes y favoritos | `~/.local/state/emax-notes/state.toml` (hasta 20 recientes) |
| Índice de búsqueda | `~/.cache/emax-notes/search-index/index.db` |

El índice es un caché. Al arrancar se reconstruye si no coincide con los archivos, y borrarlo deja las notas intactas.

Los colores salen de `~/.local/state/omarchy/current/theme/colors.toml` (`background`, `foreground`, `accent`, `selection`, `muted`). El tamaño de fuente sale de `shell.toml` (`font.base-size`, entre 8 y 32). Sin esos archivos hay un tema oscuro de respaldo. Al volver a mostrar la ventana se relee el tema si `colors.toml` cambió.

## Omarchy

La ventana mide 560×780. En Wayland la clase es el id de la aplicación, `dev.emax.notes`, así que la regla sigue valiendo cuando el título cambia. En otro compositor alcanza con lanzar `emax-notes toggle-new` y `emax-notes toggle-last` desde sus atajos; flotar y centrar la ventana es opcional.

En Omarchy, para pisar el atajo que ya ocupa `Super+N`:

```lua
-- ~/.config/hypr/bindings.lua
hl.unbind("SUPER + N")
hl.unbind("SUPER + SHIFT + N")
o.bind("SUPER + N", "Notas: nueva / mostrar-ocultar", "emax-notes toggle-new")
o.bind("SUPER + SHIFT + N", "Notas: última / mostrar-ocultar", "emax-notes toggle-last")
```

```lua
-- ~/.config/hypr/hyprland.lua
o.window("^dev.emax.notes$", {
  float = true,
  center = true,
  opacity = "1.0 1.0",
  tag = "-default-opacity",
  dim_around = true,
})
```

`dim_around` atenúa el resto del escritorio. La intensidad está en `decoration.dim_around` (por ejemplo `0.65` en `~/.config/hypr/looknfeel.lua`).

## Tests

```bash
cargo test
```
