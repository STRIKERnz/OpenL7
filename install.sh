#!/bin/sh
# Installs OpenL7 for the current user: the binary, an app-menu entry, the tray at
# login, and (with sudo) the udev rule that gives your user access to the mouse.
#   ./install.sh             install (builds first when run from a source checkout)
#   ./install.sh --uninstall remove everything it installed
set -eu
cd "$(dirname "$0")"

bin="$HOME/.local/bin/openl7"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
autostart="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
rule=/etc/udev/rules.d/70-openl7.rules

if [ "${1:-}" = "--uninstall" ]; then
    pkill -x openl7 || true
    rm -f "$bin" "$apps/openl7.desktop" "$autostart/openl7.desktop"
    [ -e "$rule" ] && sudo rm -f "$rule" && sudo udevadm control --reload
    echo "OpenL7 removed. Settings in ~/.config/openl7 were kept."
    exit 0
fi

# a release archive ships the binary next to this script; a checkout builds it
if [ -x ./openl7 ]; then
    src=./openl7
elif command -v cargo >/dev/null; then
    cargo build --release --locked
    src=target/release/openl7
else
    echo "Rust is needed to build OpenL7: https://rustup.rs" >&2
    exit 1
fi

mkdir -p "$(dirname "$bin")" "$apps" "$autostart"
# replace rather than overwrite, so a running tray keeps its old copy until restarted
install -m755 "$src" "$bin.new" && mv "$bin.new" "$bin"
sed "s|@BIN@|$bin|" packaging/openl7.desktop > "$apps/openl7.desktop"
sed "s|@BIN@|$bin|" packaging/openl7-tray.desktop > "$autostart/openl7.desktop"

if [ ! -e "$rule" ] || ! cmp -s packaging/70-openl7.rules "$rule"; then
    echo "Installing the udev rule needs sudo:"
    sudo install -m644 packaging/70-openl7.rules "$rule"
    sudo udevadm control --reload
    sudo udevadm trigger --subsystem-match=hidraw
fi

# restart the tray so it runs the new version
pkill -x openl7 || true
nohup "$bin" >/dev/null 2>&1 &
echo "OpenL7 installed. Find it in your app menu; the tray icon starts at login."
