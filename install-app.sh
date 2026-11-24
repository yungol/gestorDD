#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_SRC="${PROJECT_DIR}/target/release/gestordd"
ICON_SRC="${PROJECT_DIR}/assets/gestordd.svg"
LAUNCH_SRC="${PROJECT_DIR}/assets/gestordd-app.sh"

if [[ ! -x "$BIN_SRC" ]]; then
  echo "Error: No se encontró el binario compilado en $BIN_SRC"
  exit 1
fi

BIN_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"
ICON_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"

mkdir -p "$BIN_DIR" "$APP_DIR" "$ICON_DIR"

cp -f "$BIN_SRC" "${BIN_DIR}/gestordd"
chmod +x "${BIN_DIR}/gestordd"

cp -f "$LAUNCH_SRC" "${BIN_DIR}/gestordd-app"
chmod +x "${BIN_DIR}/gestordd-app"

cp -f "$ICON_SRC" "${ICON_DIR}/gestordd.svg"

DESKTOP_CONTENT="[Desktop Entry]
Version=1.0
Type=Application
Name=GestorDD
GenericName=Gestor de Espacio en Disco
Comment=Analiza en qué se gasta el disco duro y limpia archivos fácilmente
Exec=${BIN_DIR}/gestordd-app
Icon=${ICON_DIR}/gestordd.svg
Terminal=false
Categories=System;Utility;FileTools;
Keywords=disco;espacio;limpiar;memoria;almacenamiento;disk;cleaner;
StartupNotify=true
"

printf "%s" "$DESKTOP_CONTENT" > "${APP_DIR}/gestordd.desktop"
chmod +x "${APP_DIR}/gestordd.desktop"

# Detect Desktop directory (Spanish 'Escritorio' or English 'Desktop')
DESKTOP_DIR=""
if command -v xdg-user-dir >/dev/null 2>&1; then
  DESKTOP_DIR="$(xdg-user-dir DESKTOP 2>/dev/null || true)"
fi
if [[ -z "$DESKTOP_DIR" || ! -d "$DESKTOP_DIR" ]]; then
  if [[ -d "${HOME}/Escritorio" ]]; then
    DESKTOP_DIR="${HOME}/Escritorio"
  elif [[ -d "${HOME}/Desktop" ]]; then
    DESKTOP_DIR="${HOME}/Desktop"
  fi
fi

DESKTOP_SHORTCUT=""
if [[ -n "$DESKTOP_DIR" && -d "$DESKTOP_DIR" ]]; then
  DESKTOP_SHORTCUT="${DESKTOP_DIR}/GestorDD.desktop"
  printf "%s" "$DESKTOP_CONTENT" > "$DESKTOP_SHORTCUT"
  chmod +x "$DESKTOP_SHORTCUT"
  if command -v gio >/dev/null 2>&1; then
    gio set "$DESKTOP_SHORTCUT" metadata::trusted true 2>/dev/null || true
  fi
fi

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$APP_DIR" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
fi

echo "Instalado correctamente:"
echo "  Binario:   ${BIN_DIR}/gestordd"
echo "  Lanzador:  ${BIN_DIR}/gestordd-app"
echo "  Icono:     ${ICON_DIR}/gestordd.svg"
echo "  Menú apps: ${APP_DIR}/gestordd.desktop"
if [[ -n "$DESKTOP_SHORTCUT" ]]; then
  echo "  Escritorio: ${DESKTOP_SHORTCUT}"
fi
