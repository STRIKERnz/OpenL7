# openl7

Open-source Linux tray app, settings window, and CLI for the **MCHOSE L7 Pro** mouse. It talks to the
mouse directly over hidraw, with no web driver needed.

- Tray icon showing battery level, with low-battery notifications from 25% down
- Settings window: DPI stages with indicator colours, polling rate, sensor mode, lift-off,
  ripple / angle snapping / motion sync, sensor angle, debounce, sleep, button remapping
- CLI: `openl7 --once`, `info`, `profile [1-3]`, `set <setting> <value>`

The protocol was worked out from the vendor's M HUB WebHID driver. Every write is read back
and verified.

## Build & install

```sh
cargo build --release
cp target/release/openl7 ~/.local/bin/
```

Run `openl7` for the tray icon, or `openl7 gui` for the settings window.

Your user needs read/write access to the mouse's hidraw device, for example with a udev rule:

```
KERNEL=="hidraw*", SUBSYSTEM=="hidraw", ATTRS{idVendor}=="5253", TAG+="uaccess"
```

## Hardware test

`cargo test --release -- --ignored` changes several settings on a connected mouse, then
restores them and checks every byte.

## Licence

MIT, see [LICENSE](LICENSE).
