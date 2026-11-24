#!/usr/bin/env bash
set -euo pipefail

BIN="${HOME}/.local/bin/gestordd"
if [[ ! -x "$BIN" ]]; then
  BIN="$(command -v gestordd || true)"
fi
if [[ -z "$BIN" || ! -x "$BIN" ]]; then
  SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  BIN="${SCRIPT_DIR}/../target/release/gestordd"
fi

TARGET="${1:-$HOME}"
export COLORTERM=truecolor
export TERM=xterm-256color

if command -v gnome-terminal >/dev/null 2>&1; then
  exec gnome-terminal \
    --title="⚡ GestorDD — Gestor de Disco" \
    --geometry=132x38 \
    -- "$BIN" "$TARGET"
elif command -v xfce4-terminal >/dev/null 2>&1; then
  exec xfce4-terminal \
    --title="⚡ GestorDD — Gestor de Disco" \
    --geometry=132x38 \
    -e "$BIN \"$TARGET\""
elif command -v konsole >/dev/null 2>&1; then
  exec konsole -e "$BIN" "$TARGET"
elif command -v x-terminal-emulator >/dev/null 2>&1; then
  exec x-terminal-emulator -e "$BIN" "$TARGET"
else
  exec "$BIN" "$TARGET"
fi
