# Linux integration

Startup at login uses **XDG autostart** (works on GNOME, KDE, and most
desktop environments). `walking-reminder enable` writes:

```
~/.config/autostart/walking-reminder.desktop
```

Template:

```ini
[Desktop Entry]
Type=Application
Name=Walking Reminder
Exec=/usr/local/bin/walking-reminder __daemon
X-GNOME-Autostart-enabled=true
```

On systemd-based compositors you can also use a **user service**
(no root needed):

```
~/.config/systemd/user/walking-reminder.service
```

```ini
[Unit]
Description=Walking Reminder daemon

[Service]
ExecStart=%h/.local/bin/walking-reminder __daemon
Restart=on-failure

[Install]
WantedBy=default.target
```

Then: `systemctl --user enable --now walking-reminder`

## Overlay status

The transparent overlay is currently implemented for macOS
(`renderer/src/platform/macos.rs`). The Linux overlay will use a GTK
transparent input-shape window (X11) / layer-shell (Wayland); the
platform seam lives in `renderer/src/platform/mod.rs`. Everything else
— engine, daemon, IPC, characters, CLI — is fully portable today.
