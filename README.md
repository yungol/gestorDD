# ⚡ GestorDD

Gestor visual e interactivo de espacio en disco para terminal hecho en **Rust**, con **soporte completo de mouse** (clic, doble clic, hover y rueda) y colores modernos.

## Qué hace

- **▸ [1] Explorador interactivo:** Explora tus carpetas ordenadas por tamaño con barras de color proporcionales, ruta superior clickeable (breadcrumbs), mapa visual de distribución de la carpeta actual, filtro en tiempo real (`/`) y botones clickeables para abrir, ver en el gestor de archivos, enviar a la Papelera o borrar definitivamente.
- **✦ [2] Limpieza Rápida:** Detecta automáticamente espacio recuperable en tu equipo (Papelera, miniaturas, cachés de navegadores, `pip`/`uv`, `npm`/`yarn`/`pnpm`, `cargo`, descargas antiguas >30 días y carpetas pesadas `node_modules` / `target`) clasificadas como **SEGURA**, **REVISAR** o **SISTEMA**, con casillas clickeables y vista previa de cuánto espacio liberarás.
- **▲ [3] Top Pesados:** Lista instantánea de los archivos individuales más grandes con filtros clickeables por categoría (Videos, Comprimidos/ISO, Imágenes, Audio, Código, Docs, Otros) y salto directo a su carpeta.

## Cómo ejecutarlo

```bash
# Analizar tu carpeta personal ($HOME)
./target/release/gestordd

# Analizar una carpeta específica
./target/release/gestordd .
./target/release/gestordd /
```

## Compilar e instalar como app de escritorio (Linux)

```bash
cargo build --release
./install-app.sh
```

Esto instala `gestordd` en `~/.local/bin`, crea el acceso directo con icono en el menú de aplicaciones y en tu Escritorio para abrirlo con doble clic.

## Controles principales

- **Mouse:** Clic en pestañas, botones, categorías y filas; doble clic para entrar a carpetas; clic derecho para subir de carpeta; rueda del mouse para desplazarte.
- **Teclado:** `1`/`2`/`3`/`Tab` (pestañas), `↑`/`↓` (mover), `Enter` (abrir/marcar), `Backspace` (subir), `/` (filtrar), `d` (Papelera), `Shift+D` (borrar definitivo), `s` (ordenar), `r` (re-escanear), `?` (ayuda), `q` (salir).
