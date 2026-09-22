# Windows integration (roadmap)

Startup at login will use the **per-user Startup folder** (no admin):

```
%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\dribble.cmd
```

or the per-user Run registry key:

```
HKCU\Software\Microsoft\Windows\CurrentVersion\Run
```

## Overlay plan

The transparent overlay will be a Win32 layered window:

- `CreateWindowExW` with `WS_EX_LAYERED | WS_EX_TRANSPARENT |
  WS_EX_NOACTIVATE | WS_EX_TOPMOST` (transparent, click-through,
  non-focus-stealing, always on top)
- `UpdateLayeredWindow` with premultiplied BGRA frames composited in
  Rust (sprite + rounded bubble + DirectWrite text for emoji support)

The platform seam is `renderer/src/platform/mod.rs`; the engine, daemon
(over a named pipe IPC), characters and CLI are already portable.
