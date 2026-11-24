#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_SRC="${PROJECT_DIR}/target/release/gestordd"
ICON_SRC="${PROJECT_DIR}/assets/gestordd.svg"

if [[ ! -x "$BIN_SRC" ]]; then
  echo "Error: No se encontró el binario compilado en $BIN_SRC"
  echo "Ejecuta primero: cargo build --release"
  exit 1
fi

# 1. Instalar binario CLI en ~/.local/bin (igual que en Linux)
BIN_DIR="${HOME}/.local/bin"
mkdir -p "$BIN_DIR"
cp -f "$BIN_SRC" "${BIN_DIR}/gestordd"
chmod +x "${BIN_DIR}/gestordd"

# 2. Elegir carpeta de Aplicaciones (/Applications si hay permisos, si no ~/Applications)
if [[ -d "/Applications" && -w "/Applications" ]]; then
  APPS_DIR="/Applications"
else
  APPS_DIR="${HOME}/Applications"
  mkdir -p "$APPS_DIR"
fi

APP_BUNDLE="${APPS_DIR}/GestorDD.app"
CONTENTS_DIR="${APP_BUNDLE}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"

rm -rf "$APP_BUNDLE"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

# 3. Copiar binario dentro del bundle para que la app sea autocontenida
cp -f "$BIN_SRC" "${RESOURCES_DIR}/gestordd"
chmod +x "${RESOURCES_DIR}/gestordd"

# 4. Script .command que abre GestorDD en una ventana de terminal con buen tamaño y colores
cat > "${RESOURCES_DIR}/run-gestordd.command" <<'EOF'
#!/usr/bin/env bash
export TERM=xterm-256color
export COLORTERM=truecolor
# Ajustar título y tamaño de ventana (132x38) si el terminal lo soporta
printf '\033]0;⚡ GestorDD — Gestor de Disco\007'
printf '\033[8;38;132t'
clear
RESOURCES_DIR="$(cd "$(dirname "$0")" && pwd)"
exec "${RESOURCES_DIR}/gestordd" "${1:-$HOME}"
EOF
chmod +x "${RESOURCES_DIR}/run-gestordd.command"

# 5. Lanzador principal de la .app (al hacer doble clic o abrir desde Spotlight/Aplicaciones)
cat > "${MACOS_DIR}/GestorDD" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
RESOURCES_DIR="$(cd "$(dirname "$0")/../Resources" && pwd)"
RUNNER="${RESOURCES_DIR}/run-gestordd.command"

if [[ -d "/Applications/iTerm.app" || -d "${HOME}/Applications/iTerm.app" ]]; then
  open -a iTerm "$RUNNER"
else
  open -a Terminal "$RUNNER"
fi
EOF
chmod +x "${MACOS_DIR}/GestorDD"

# 6. Convertir assets/gestordd.svg a GestorDD.icns usando herramientas nativas de macOS
ICON_BUILT=0
if [[ -f "$ICON_SRC" ]] && command -v qlmanage >/dev/null 2>&1 && command -v sips >/dev/null 2>&1 && command -v iconutil >/dev/null 2>&1; then
  TMP_ICON_DIR="$(mktemp -d)"
  trap 'rm -rf "$TMP_ICON_DIR"' EXIT
  if qlmanage -t -s 1024 -o "$TMP_ICON_DIR" "$ICON_SRC" >/dev/null 2>&1; then
    PNG_SRC="${TMP_ICON_DIR}/$(basename "$ICON_SRC").png"
    if [[ -f "$PNG_SRC" ]]; then
      ICONSET_DIR="${TMP_ICON_DIR}/GestorDD.iconset"
      mkdir -p "$ICONSET_DIR"
      for sz in 16 32 128 256 512; do
        sips -z "$sz" "$sz" "$PNG_SRC" --out "${ICONSET_DIR}/icon_${sz}x${sz}.png" >/dev/null 2>&1 || true
        sz2=$((sz * 2))
        sips -z "$sz2" "$sz2" "$PNG_SRC" --out "${ICONSET_DIR}/icon_${sz}x${sz}@2x.png" >/dev/null 2>&1 || true
      done
      if iconutil -c icns "$ICONSET_DIR" -o "${RESOURCES_DIR}/GestorDD.icns" >/dev/null 2>&1; then
        ICON_BUILT=1
      fi
    fi
  fi
fi

# 7. Info.plist del bundle macOS
ICON_PLIST=""
if [[ "$ICON_BUILT" -eq 1 ]]; then
  ICON_PLIST="  <key>CFBundleIconFile</key>
  <string>GestorDD</string>"
fi

cat > "${CONTENTS_DIR}/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>GestorDD</string>
  <key>CFBundleDisplayName</key>
  <string>GestorDD</string>
  <key>CFBundleIdentifier</key>
  <string>com.yungol.gestordd</string>
  <key>CFBundleVersion</key>
  <string>0.1.0</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleExecutable</key>
  <string>GestorDD</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>NSHighResolutionCapable</key>
  <true/>
${ICON_PLIST}
</dict>
</plist>
EOF

# Refrescar caché de icono en Finder/Spotlight
touch "$APP_BUNDLE"

echo "Instalado correctamente en macOS:"
echo "  App:     ${APP_BUNDLE}"
echo "  Binario: ${BIN_DIR}/gestordd"
if [[ "$ICON_BUILT" -eq 1 ]]; then
  echo "  Icono:   ${RESOURCES_DIR}/GestorDD.icns"
fi
echo ""
echo "Ya puedes abrir 'GestorDD' desde Aplicaciones, Launchpad o Spotlight (Cmd+Espacio)."
