# snip

Press **Ctrl+Shift+S**, drag a rectangle, and the screenshot is on your clipboard. Esc or right-click cancels.

## Windows

A ~120 KB tray app written in Rust, with `windows-sys` as its only dependency. It adds itself to startup each time it launches; you can turn that off in Task Manager → Startup apps.

```
cargo build --release
target\release\snip.exe
```

Left-click the tray icon to take a capture. Right-click it and choose Exit to quit.

## Linux (Omarchy / Hyprland)

On Hyprland the compositor already handles global hotkeys, and `grim`, `slurp` and `wl-copy` cover capture, selection and clipboard. Snip on Linux is therefore one keybinding with no daemon:

```
sh linux/omarchy.sh
```

The script installs any missing packages and adds the binding to `~/.config/hypr/bindings.conf`. Running it again won't duplicate the binding.

## License

MIT
