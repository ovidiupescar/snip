#!/bin/sh
# snip for Omarchy (Hyprland): Ctrl+Shift+S -> drag a region -> PNG on the clipboard. Esc cancels.
# No daemon or tray needed: Hyprland owns the hotkey, grim/slurp/wl-copy do the work.
set -e

pacman -Q grim slurp wl-clipboard >/dev/null 2>&1 || sudo pacman -S --needed grim slurp wl-clipboard

conf="$HOME/.config/hypr/bindings.conf"          # Omarchy's user bindings
[ -f "$conf" ] || conf="$HOME/.config/hypr/hyprland.conf"
line='bind = CTRL SHIFT, S, exec, grim -g "$(slurp)" - | wl-copy'
grep -qxF "$line" "$conf" 2>/dev/null || printf '\n%s\n' "$line" >> "$conf"
hyprctl reload >/dev/null
echo "snip: Ctrl+Shift+S bound in $conf"
