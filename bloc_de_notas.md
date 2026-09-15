Quiero que diseñes e implementes una aplicación de notas extremadamente rápida, simple, local-first y pensada específicamente para Omarchy + Hyprland + Wayland.
-
El objetivo NO es crear otro Obsidian, Notion, Joplin o QOwnNotes.

La aplicación debe sentirse como una herramienta integrada al sistema y prácticamente desaparecer mientras no la necesito.

La experiencia fundamental debe ser:

**Super+N → escribir → Esc → continuar trabajando.**

La prioridad absoluta es:

1. velocidad;
2. consumo mínimo;
3. interfaz extremadamente limpia;
4. integración visual perfecta con Omarchy;
5. uso keyboard-first;
6. archivos Markdown estándar;
7. mínima cantidad de atajos;
8. cero fricción para crear una nota;
9. búsqueda extremadamente rápida;
10. cero Electron, Chromium o runtimes pesados.

---

# 1. Principio central de UX

La aplicación NO debe tener una interfaz tradicional de gestor de notas.

No quiero:

- navbar permanente;
- sidebar;
- árbol de notas visible;
- lista de recientes visible;
- lista de favoritos visible;
- barra superior llena de botones;
- paneles auxiliares;
- navegación permanente.

Al ejecutar:

```text
Super + N
```

quiero encontrarme inmediatamente con esto:

```text
╭──────────────────────────────────────────────╮
│                                              │
│  ▌                                           │
│                                              │
│                                              │
│                                              │
╰──────────────────────────────────────────────╯
```

Nada más.

El cursor debe estar preparado para escribir.

No debe ser necesario:

- crear una nota manualmente antes de escribir;
- elegir una carpeta;
- elegir un nombre;
- escribir un título;
- presionar un botón;
- seleccionar un archivo.

La aplicación debe comportarse prácticamente como una hoja de papel instantánea.

La interfaz principal debe ser:

> el texto y yo.

---

# 2. Filosofía de interacción

La aplicación tendrá dos elementos fundamentales:

```text
Editor
   +
Command Palette
```

Todo lo que no necesite estar permanentemente visible debe vivir dentro de la Command Palette.

Ejemplos:

```text
buscar notas
abrir notas
recientes
favoritos
crear nota
eliminar nota
renombrar
mover
abrir directorio
configuración
cambiar carpeta
estadísticas simples
```

No crear elementos visuales permanentes para funcionalidades que puedan resolverse desde la paleta.

La Command Palette será el centro de navegación de la aplicación.

---

# 3. Flujo principal

La experiencia más común debe ser:

```text
Super + N
    ↓
aparece el editor
    ↓
cursor listo
    ↓
escribo inmediatamente
    ↓
autosave
    ↓
Esc
    ↓
la aplicación desaparece
```

No cambiar de workspace.

No alterar el tiling.

No reorganizar otras ventanas.

No mostrar pantallas intermedias.

No preguntar dónde guardar.

---

# 4. Plataforma

Objetivo principal:

```text
Linux
Arch Linux
Omarchy
Hyprland
Wayland
```

No priorizar multiplataforma inicialmente.

La integración nativa con Omarchy y Hyprland tiene prioridad sobre la portabilidad.

---

# 5. Stack preferido

Priorizar:

```text
Rust
GTK4 / gtk-rs
GtkSourceView 5
```

Evitar:

```text
Electron
Chromium
WebView
Node.js
Bun
Python como runtime principal
```

Quiero un binario nativo ligero.

Antes de implementar, verificar si Rust + GTK4 + GtkSourceView sigue siendo la mejor combinación considerando:

- startup;
- RAM;
- Wayland;
- Omarchy;
- mantenimiento;
- rendering de Markdown;
- integración con temas.

Si encontrás una alternativa claramente superior, justificarla técnicamente antes de cambiar el stack.

---

# 6. Integración con el tema actual de Omarchy

Este requisito es fundamental.

La aplicación NO debe utilizar una paleta fija.

Debe tomar automáticamente la paleta del tema activo de Omarchy.

Si cambio:

```text
Solicitude
→ Aether
→ otro tema
```

la aplicación debe cambiar con él.

Antes de implementar esta parte:

1. investigar cómo funciona actualmente el sistema de temas de Omarchy;
2. encontrar la verdadera fuente de la paleta activa;
3. determinar cómo Omarchy comunica o aplica un cambio de tema;
4. identificar colores, opacidad, bordes y demás tokens disponibles.

No asumir rutas.

Investigar el sistema instalado realmente.

Puede ser necesario analizar elementos como:

```text
~/.config/omarchy/
~/.local/share/omarchy/
~/.local/state/omarchy/
GTK CSS
Hyprland configs
Waybar configs
theme symlinks
archivos de paleta
```

pero no asumir que esas rutas son correctas.

Crear una abstracción:

```text
ThemeProvider
    │
    └── OmarchyThemeProvider
```

El resto de la aplicación NO debe conocer directamente cómo Omarchy guarda sus themes.

Arquitectura conceptual:

```text
Omarchy active theme
        │
        ▼
OmarchyThemeProvider
        │
        ▼
Internal Design Tokens
        │
        ▼
GTK CSS
        │
        ▼
Application
```

Tokens internos aproximados:

```text
background
surface
surface_alt

foreground
foreground_muted

primary
secondary
accent

border
selection

success
warning
error

radius_small
radius_medium
radius_large

opacity
```

También investigar si Omarchy proporciona:

- font;
- opacity;
- blur;
- border radius;
- border color;
- GTK dark/light preference;
- icon theme.

La aplicación debe sentirse como parte de Omarchy y no como una aplicación con colores similares.

Si el tema cambia mientras la aplicación está ejecutándose, intentar actualizarlo dinámicamente mediante eventos/file watchers.

Evitar polling.

---

# 7. Editor como interfaz completa

No quiero una MainWindow compleja.

Conceptualmente:

```text
ApplicationWindow
        │
        └── Editor
```

Opcionalmente pueden aparecer elementos temporales como:

```text
Command Palette
Find overlay
Markdown preview
Context menus
Notifications
```

Pero desaparecen cuando dejan de utilizarse.

No debe existir permanentemente:

```text
Sidebar
Navbar
File explorer
Recent panel
Favorites panel
Toolbar
Status bar innecesaria
```

Cada píxel debe justificar su existencia.

---

# 8. Command Palette

Atajo:

```text
Ctrl + K
```

Esta será una de las funcionalidades más importantes de toda la aplicación.

Debe abrirse instantáneamente.

Ejemplo:

```text
╭──────────────────────────────────────────────╮
│ 🔎 docker                                    │
├──────────────────────────────────────────────┤
│ Docker compose                              │
│   ...docker compose up -d...                 │
│                                              │
│ Comandos Linux                               │
│   ...reiniciar docker daemon...              │
│                                              │
│ Proyecto backend                             │
│   ...Dockerfile utilizado para...            │
╰──────────────────────────────────────────────╯
```

La paleta debe combinar:

```text
navegación
+
búsqueda
+
acciones
```

No quiero aprender múltiples atajos para navegar diferentes partes de la aplicación.

---

# 9. Búsqueda global

La búsqueda debe estar integrada directamente dentro de:

```text
Ctrl + K
```

NO crear un shortcut separado para:

```text
buscar en todas las notas
```

No quiero:

```text
Ctrl+Shift+F
Ctrl+P
Ctrl+R
etc.
```

La paleta debe resolver todo eso.

---

# 10. Búsqueda basada principalmente en contenido

No quiero que la búsqueda dependa principalmente del nombre del archivo.

Cuando escribo:

```text
docker redis puerto
```

quiero encontrar cualquier nota relevante aunque:

- esas palabras no estén en el filename;
- no exista un título explícito;
- aparezcan solamente dentro del cuerpo.

Por lo tanto:

```text
CONTENT SEARCH
```

debe ser una funcionalidad de primera clase.

No limitarse conceptualmente a:

```bash
grep
```

o una búsqueda literal básica.

Investigar una solución de full-text search eficiente.

Evaluar alternativas como:

```text
SQLite FTS5
Tantivy
índice invertido propio
trigram indexing
fuzzy matching
otras alternativas apropiadas en Rust
```

Evaluar:

- RAM;
- tamaño del índice;
- tiempo de indexación;
- búsqueda incremental;
- fuzzy search;
- ranking;
- snippets;
- Unicode;
- español;
- nombres técnicos;
- code snippets;
- actualización incremental.

No incorporar una dependencia enorme sin justificarla.

---

# 11. Search ranking

La búsqueda debe considerar diferentes señales.

Por ejemplo:

```text
match exacto
match aproximado
frecuencia
proximidad entre palabras
posición dentro de la nota
título derivado
contenido
recencia
favorito
```

No necesariamente todas deben implementarse en V1.

Primero diseñar una estrategia simple y medible.

Una posibilidad:

```text
title-derived match     → peso alto
content exact match     → peso alto
content fuzzy match     → peso medio
recently opened         → pequeño boost
favorite                → pequeño boost
```

Pero el contenido siempre debe ser protagonista.

---

# 12. Resultados con contexto

No quiero recibir únicamente:

```text
docker.md
linux.md
backend.md
```

Los resultados deben mostrar contexto.

Por ejemplo:

```text
Docker compose

"...para levantar postgres y redis ejecutar
docker compose up -d..."
```

Esto permite saber por qué apareció el resultado.

Idealmente resaltar el match.

---

# 13. Search-as-you-type

Dentro de:

```text
Ctrl + K
```

la búsqueda debe actualizar resultados a medida que escribo.

Objetivo:

```text
Ctrl+K
↓
"doc"
↓
resultados
↓
"docker"
↓
resultados refinados
```

Sin necesidad de Enter para ejecutar la búsqueda.

Medir latencia.

La sensación debe ser prácticamente instantánea.

---

# 14. Command Palette y comandos

La paleta también debe ofrecer comandos.

Por ejemplo escribiendo:

```text
>
```

podrían aparecer:

```text
New note
Favorites
Recent
Delete current note
Rename current note
Move current note
Open notes directory
Settings
Markdown preview
```

El mecanismo exacto puede variar.

Lo importante es mantener:

```text
una única interfaz de comandos
```

en lugar de crear menús y shortcuts diferentes para todo.

---

# 15. Favoritos

Los favoritos NO deben tener una sección permanente.

Se accede:

```text
Ctrl+K
→ Favorites
```

y aparecen allí.

Opcionalmente:

```text
> favorite current
```

o una acción contextual.

No crear:

```text
sidebar Favorites
```

---

# 16. Recientes

Exactamente lo mismo.

No mostrar permanentemente:

```text
Recent notes
```

Acceso:

```text
Ctrl+K
→ Recent
```

Puede ser también el estado inicial de la paleta cuando se abre vacía.

Por ejemplo:

```text
Ctrl+K

Recent
────────────────
Comandos Linux
Idea Omarchy
Backend Redis

Commands
────────────────
New note
Favorites
Settings
```

Esto deberá evaluarse mediante UX.

---

# 17. Navegación entre notas

La navegación también ocurre mediante la paleta.

Ejemplo:

```text
Ctrl+K
docker
↓
selecciono resultado
↓
la nota reemplaza el contenido del editor
```

No abrir ventanas adicionales.

No necesitar un file browser.

No sidebar.

---

# 18. Nueva nota

Atajo directo permitido:

```text
Ctrl + N
```

Es uno de los pocos shortcuts que quiero recordar.

También debe estar disponible desde:

```text
Ctrl+K
→ New note
```

Al crearla:

```text
Ctrl+N
↓
editor vacío
↓
cursor listo
```

Nada más.

---

# 19. No pedir nombre de archivo

Crear una nota nunca debe mostrar:

```text
Nombre:
[________________]
```

Eso rompe el flujo.

El usuario debe simplemente escribir.

La aplicación se ocupa del almacenamiento.

---

# 20. Título visual de una nota

Separar conceptualmente:

```text
nombre físico del archivo
```

de:

```text
nombre/título mostrado al usuario
```

El usuario no debería necesitar preocuparse por filenames.

El título visual puede derivarse automáticamente del contenido.

Prioridad posible:

```text
primer heading Markdown
        ↓
primera línea no vacía
        ↓
fragmento inicial del contenido
        ↓
"Untitled"
```

Ejemplo:

```markdown
# Docker Compose

Para levantar...
```

Título mostrado:

```text
Docker Compose
```

Si la nota contiene:

```text
recordar revisar mañana el compose de backend
```

el título puede derivarse de esa primera línea.

No obligar a utilizar:

```text
# Título
```

---

# 21. Filename físico

Investigar una estrategia que NO introduzca fricción.

El filename puede ser un detalle de implementación.

Opciones a evaluar:

```text
timestamp estable
UUID corto
slug generado una única vez
hash parcial
```

Ejemplo:

```text
~/Notes/20260914-221523.md
```

El usuario nunca necesita verlo normalmente.

Evitar renombrar constantemente el archivo según cambia la primera línea, porque podría:

- generar ruido en Git;
- romper enlaces;
- complicar file watchers;
- complicar sincronización;
- generar renames innecesarios.

Si se decide utilizar slugs derivados del contenido, hacerlo solamente bajo reglas muy claras.

La prioridad es:

```text
cero fricción
+
filesystem robusto
```

---

# 22. Fuente de verdad

Los `.md` siguen siendo la fuente absoluta de verdad.

Ejemplo:

```text
~/Notes/

├── 20260914-221523.md
├── 20260914-225102.md
├── ideas/
│   └── 20260913-142211.md
└── trabajo/
    └── 20260912-091531.md
```

Esto debe permitir:

```bash
cat ~/Notes/...
nvim ~/Notes/...
rg "docker" ~/Notes
git init ~/Notes
```

La aplicación nunca debe secuestrar los datos dentro de una DB propietaria.

---

# 23. Metadata

Mantener metadata mínima.

Por ejemplo:

```yaml
---
tags: [linux, omarchy]
favorite: true
---
```

Pero incluso eso debe ser opcional.

Una nota perfectamente válida:

```markdown
recordar revisar el servicio redis mañana
```

No insertar metadata innecesaria simplemente por abrir un archivo.

---

# 24. Autosave

Autosave obligatorio.

Flujo:

```text
typing
   │
   └── debounce ~300-500ms
           │
           ▼
        atomic save
```

No obligar al usuario a presionar:

```text
Ctrl+S
```

aunque GTK pueda seguir soportándolo por convención.

Utilizar escritura segura:

```text
temp file
   ↓
flush
   ↓
fsync
   ↓
atomic rename
```

Evitar corrupción.

---

# 25. Scratchpad

Feature principal.

```text
Super + N → siempre nueva nota (captura rápida)
Super + Shift + N → última nota (continuar)
```

Super+N debe:

```text
si app oculta
   → mostrar + buffer nuevo vacío (sin crear archivo aún)

si app visible
   → ocultar, no crea otra
```

Ctrl+N (con app visible) → nuevo buffer vacío.

Regla lazy: no crear archivo en disco hasta el primer caracter.
Si Esc con buffer vacío, descartar sin guardar, sin efectos.
Si el último buffer auto-creado sigue vacío, reutilizarlo en el próximo Super+N.

Super+Shift+N → mostrar + última nota del recent stack
(última abierta/modificada persistida, excluyendo vacío actual).

Al aparecer:

- floating;
- centrada;
- encima del workspace;
- foco inmediato;
- editor activo;
- cursor listo;
- sin elementos visuales innecesarios.

Al presionar:

```text
Esc
```

se oculta.

No destruir necesariamente el proceso.

En reposo:

```text
CPU ≈ 0 %
```

---

# 26. Single instance

Investigar:

```text
GApplication
D-Bus
Unix socket
Hyprland IPC
```

Preferir la solución más pequeña y robusta.

Arquitectura:

```text
Super+N
   │
   ▼
app activation
   │
   ├─ first activation → startup
   │
   └─ running          → toggle
```

---

# 27. File watcher

Detectar cambios externos.

Eventos:

```text
create
modify
delete
rename
```

Utilizar inotify o abstracción apropiada.

Ejemplo:

```bash
nvim ~/Notes/20260914-221523.md
```

Al volver a la app, el cambio debe aparecer.

No hacer polling permanente.

---

# 28. Conflictos

Caso:

```text
versión cargada en aplicación
       +
archivo modificado externamente
       +
cambios locales sin guardar
```

No sobrescribir silenciosamente.

Resolver mediante una interacción mínima:

```text
Reload external
Keep mine
Compare
```

No crear un sistema de versionado complejo.

---

# 29. Markdown

V1:

```markdown
# headings

**bold**

*italic*

- list

- [ ] task
- [x] task

`inline code`

```rust
fn main() {}
```

[link](...)
```

La edición debe continuar sintiéndose como texto plano.

No implementar WYSIWYG complejo.

---

# 30. Markdown preview

Uno de los pocos shortcuts adicionales permitidos:

```text
Ctrl + Shift + P
```

Toggle:

```text
Editor
  ↕
Rendered Markdown
```

No mantener permanentemente:

```text
Editor | Preview
```

El preview aparece cuando lo necesito y desaparece cuando termino.

Evitar WebView si existe una alternativa nativa suficientemente buena.

---

# 31. Buscar dentro de la nota actual

Shortcut:

```text
Ctrl + F
```

Este sí debe ser directo porque sigue una convención universal.

Debe buscar únicamente dentro de la nota actual.

UI temporal:

```text
╭─────────────────────────────╮
│ Find: docker                │
╰─────────────────────────────╯
```

No crear panel permanente.

---

# 32. Atajos mínimos

Quiero deliberadamente pocos.

Los shortcuts principales de la aplicación serán:

```text
Super+N          nueva nota (mostrar + buffer vacío / ocultar si visible)

Super+Shift+N    última nota (mostrar + continuar)

Ctrl+K           command palette

Ctrl+N           new note (con app visible)

Ctrl+Shift+P     Markdown preview

Ctrl+F           find in current note

Esc              close overlay / hide application
```

Y nada más como shortcut propio salvo que exista una razón extremadamente fuerte.

No agregar:

```text
Ctrl+P
Ctrl+Shift+F
Ctrl+L
Ctrl+R
Alt+1
Alt+2
etc.
```

Las acciones menos frecuentes pertenecen a:

```text
Ctrl+K
```

No quiero memorizar 30 atajos para un bloc de notas simple.

Los shortcuts estándar del editor:

```text
Ctrl+C
Ctrl+V
Ctrl+X
Ctrl+A
Ctrl+Z
Ctrl+Shift+Z
```

pueden mantenerse naturalmente.

---

# 33. Search engine

La búsqueda es suficientemente importante como para tratarla como un subsistema independiente.

Arquitectura:

```text
MarkdownRepository
       │
       ▼
 SearchIndexer
       │
       ▼
   SearchIndex
       │
       ▼
 SearchService
       │
       ▼
 CommandPalette
```

Los archivos `.md` siguen siendo la fuente de verdad.

El índice es descartable.

Ejemplo:

```text
~/.cache/<app>/search-index
```

Eliminar ese directorio no debe perder ninguna nota.

La aplicación simplemente reconstruye el índice.

---

# 34. Índice incremental

No reconstruir todo el índice ante cada tecla.

Cuando una nota cambia:

```text
note modified
     ↓
debounce
     ↓
save
     ↓
update only this document
```

Lo mismo con cambios detectados mediante file watcher.

---

# 35. Búsqueda inteligente pero pequeña

Quiero algo mejor que un grep literal, pero tampoco quiero incorporar un Elasticsearch dentro de mi bloc de notas.

Buscar un punto de equilibrio.

Debe soportar bien consultas como:

```text
docker redis puerto
```

y encontrar:

```text
"Redis está expuesto mediante el puerto 6379 dentro del docker compose..."
```

aunque la consulta no aparezca exactamente como una única cadena.

También sería bueno tolerar diferencias pequeñas:

```text
configuracion
configuración
configure
```

si puede hacerse eficientemente.

Investigar:

- tokenización;
- stemming si tiene sentido;
- accent folding;
- fuzzy matching;
- trigram;
- BM25;
- prefix matching.

No implementar todo por defecto.

Elegir la combinación con mejor relación:

```text
calidad / complejidad / consumo
```

---

# 36. Posible estrategia híbrida

Evaluar algo como:

```text
FTS index
    +
fuzzy title matching
    +
content snippets
```

La búsqueda podría usar:

```text
BM25 → ranking de contenido

fuzzy matcher → navegación/títulos

metadata → favorite/recent boosts
```

Pero medir antes de complicarlo.

---

# 37. Performance

Objetivos fundamentales.

Idle:

```text
CPU ≈ 0 %
```

No loops constantes.

No timers de polling innecesarios.

Startup:

```text
medir cold start
medir warm activation
```

Search:

```text
medir query latency
```

Memory:

```text
medir idle RSS
medir RSS con 100 notas
medir RSS con 10.000 notas
```

Index:

```text
medir build time
medir index size
```

No decir:

```text
"es ultrarrápido"
```

sin benchmarks.

---

# 38. Organización mediante la paleta

Carpetas, tags, recientes y favoritos pueden existir.

Pero son información, no UI permanente.

Ejemplo:

```text
Ctrl+K
> favorites
```

```text
Ctrl+K
> recent
```

```text
Ctrl+K
> move
```

```text
Ctrl+K
> tags
```

No crear un panel especial para cada cosa.

---

# 39. Settings

La configuración también puede abrirse desde:

```text
Ctrl+K
→ Settings
```

Mantener pocas opciones:

```toml
notes_dir = "~/Notes"

font_size = 14
spellcheck = true
autosave_ms = 400
```

Más integración automática y menos configuración manual.

Respetar XDG:

```text
$XDG_CONFIG_HOME
$XDG_CACHE_HOME
$XDG_STATE_HOME
```

---

# 40. Spellcheck

Agregar solamente si puede hacerse de manera razonablemente liviana.

Debe ser:

```text
enable
disable
language
```

No bloquear la escritura.

---

# 41. Arquitectura conceptual

Mantener módulos pequeños:

```text
Application
│
├── UI
│   ├── EditorWindow
│   ├── Editor
│   ├── CommandPalette
│   ├── FindOverlay
│   └── MarkdownPreview
│
├── Core
│   ├── Note
│   ├── NoteStore
│   ├── RecentNotes
│   ├── Favorites
│   └── Settings
│
├── Search
│   ├── SearchIndexer
│   ├── SearchIndex
│   ├── SearchService
│   └── Ranking
│
├── Filesystem
│   ├── MarkdownRepository
│   ├── FileWatcher
│   └── AtomicWriter
│
├── Platform
│   ├── WindowManagerAdapter
│   │      └── HyprlandAdapter
│   │
│   └── ThemeProvider
│          └── OmarchyThemeProvider
│
└── Infrastructure
    └── AppActivation
```

No es obligatorio copiarla exactamente.

Mantener separación conceptual.

---

# 42. Dependencias

Para cada dependencia:

```text
¿qué problema resuelve?
¿es realmente necesaria?
¿cuánto pesa?
¿está mantenida?
¿agrega runtime?
¿agrega procesos?
¿puede resolverse simplemente?
```

No sacrificar mantenibilidad por eliminar una dependencia útil.

Pero tampoco importar frameworks gigantes por comodidad.

---

# 43. Lo que NO quiero

No implementar por iniciativa propia:

```text
IA
LLMs
MCP

cloud propio
login
cuentas

telemetría
analytics

plugin system
plugin marketplace

collaboration
real-time editing

calendar
Kanban
task manager avanzado

database tables
canvas
graph view

permanent sidebar
permanent navbar
permanent toolbar

Electron
Chromium
embedded browser
```

La aplicación es un bloc de notas.

Mantenerla como tal.

---

# 44. Sincronización

No implementar sync propio.

El directorio:

```text
~/Notes
```

puede utilizar:

```text
Syncthing
Nextcloud
Git
rsync
```

externamente.

La aplicación simplemente trabaja con archivos.

---

# 45. Git-friendly

No modificar una nota simplemente por abrirla.

No reordenar metadata.

No insertar timestamps innecesarios dentro del contenido.

No generar ruido constante.

La carpeta debería poder ser:

```bash
cd ~/Notes
git init
git add .
git commit
```

y producir diffs razonables.

---

# 46. Desarrollo incremental

NO implementar todo de una vez.

## Fase 0 — Research

Investigar:

```text
Omarchy themes
active palette
theme switching

Hyprland
window rules
scratchpad behavior

GTK4
GtkSourceView

full-text search options
SQLite FTS5
Tantivy
alternative Rust search engines

file naming strategy
single instance
```

Entregar conclusiones antes de construir demasiado.

---

## Fase 1 — Core UX prototype

Implementar solamente:

```text
Super+N

floating editor

zero permanent UI

cursor ready immediately

typing

autosave

Esc hide

Omarchy colors

single instance
```

Esta fase es crítica.

Si esto no se siente excelente, no avanzar agregando funciones.

---

## Fase 2 — Notes

Agregar:

```text
Ctrl+N

multiple notes

stable storage

derived display title

file watcher

atomic saves
```

---

## Fase 3 — Command Palette

Agregar:

```text
Ctrl+K

open note
recent
favorites
commands
```

Mantener UX extremadamente rápida.

---

## Fase 4 — Full-text search

Implementar el motor elegido.

Agregar:

```text
incremental index

content search

ranking

snippets

search-as-you-type
```

Medir performance.

---

## Fase 5 — Markdown

Agregar:

```text
syntax highlighting

Ctrl+F

Markdown preview

checklists

links

spellcheck
```

---

## Fase 6 — Polish

Trabajar:

```text
Omarchy visual integration

animations mínimas

focus

window positioning

startup performance

memory

search latency

packaging
```

---

# 47. Testing

Priorizar tests en:

```text
AtomicWriter

MarkdownRepository

FileWatcher

SearchIndexer

Search ranking

external conflicts

metadata

theme parser

file naming

note title derivation
```

La prioridad es:

```text
no perder datos
+
resultados de búsqueda correctos
```

---

# 48. Packaging

Objetivo inicial:

```text
Arch Linux
```

Preparar eventualmente:

```text
PKGBUILD
```

No priorizar AppImage/Flatpak antes de que la aplicación funcione bien en Omarchy.

---

# 49. Métrica principal de UX

La prueba más importante será esta:

```text
estoy programando
      ↓
recuerdo algo
      ↓
Super+N
      ↓
escribo inmediatamente
      ↓
Esc
      ↓
sigo programando
```

Ese proceso debe tener prácticamente cero fricción.

Segunda prueba:

```text
recuerdo algo que anoté
      ↓
Super+N
      ↓
Ctrl+K
      ↓
escribo dos o tres palabras que recuerdo
      ↓
aparece la nota correcta
      ↓
Enter
```

No debería necesitar recordar:

```text
cómo se llamaba
en qué carpeta estaba
qué filename tenía
cuándo la escribí
```

El buscador debe resolver eso.

---

# 50. Principio rector

Cada feature debe pasar esta pregunta:

> ¿Hace más rápido capturar o recuperar una nota sin agregar complejidad visible?

Si la respuesta es no:

no agregarla.

Y cada elemento visual debe responder:

> ¿Necesito ver esto permanentemente mientras escribo?

Si la respuesta es no:

debe desaparecer detrás de `Ctrl+K` o de una interacción temporal.

La aplicación ideal debe sentirse menos como un gestor de notas y más como:

**una extensión instantánea de mi memoria dentro de Omarchy.**
