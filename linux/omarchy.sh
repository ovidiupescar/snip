#!/bin/sh
# snip for Omarchy (Hyprland): Super+Shift+S -> drag a region -> PNG on the clipboard. Esc cancels.
# No daemon or tray needed: Hyprland owns the hotkey, grim/slurp/wl-copy do the work.
set -e

pacman -Q grim slurp wl-clipboard >/dev/null 2>&1 || sudo pacman -S --needed grim slurp wl-clipboard

conf="$HOME/.config/hypr/bindings.conf"          # Omarchy's user bindings
[ -f "$conf" ] || conf="$HOME/.config/hypr/hyprland.conf"
line='bind = SUPER SHIFT, S, exec, grim -g "$(slurp)" - | wl-copy'
# unbind first so ours replaces any default on the same keys
grep -qxF "$line" "$conf" 2>/dev/null || printf '\nunbind = SUPER SHIFT, S\n%s\n' "$line" >> "$conf"
hyprctl reload >/dev/null
echo "snip: Super+Shift+S bound in $conf"
