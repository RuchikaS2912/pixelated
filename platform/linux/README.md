# Linux integration

Startup at login uses **XDG autostart** (works on GNOME, KDE, and most
desktop environments). `dribble enable` writes:

```
~/.config/autostart/dribble.desktop
```

Template:

```ini
[Desktop Entry]
Type=Application
Name=Dribble
Exec=/usr/local/bin/dribble __daemon
X-GNOME-Autostart-enabled=true
```

On systemd-based compositors you can also use a **user service**
(no root needed):

```
~/.config/systemd/user/dribble.service
```

```ini
[Unit]
Description=Dribble daemon

[Service]
ExecStart=%h/.local/bin/dribble __daemon
Restart=on-failure

[Install]
WantedBy=default.target
```

Then: `systemctl --user enable --now dribble`

## Overlay status

The transparent overlay is currently implemented for macOS
(`renderer/src/platform/macos.rs`). The Linux overlay will use a GTK
transparent input-shape window (X11) / layer-shell (Wayland); the
platform seam lives in `renderer/src/platform/mod.rs`. Everything else
— engine, daemon, IPC, characters, CLI — is fully portable today.
