# Architecture

## Why Rust + native OS overlays

Requirements: very small install, low idle memory, background
execution, transparent click-through overlays, cross-platform, easy
install. Evaluation:

| Option | Install | Idle RAM | Overlay quality | Notes |
|---|---|---|---|---|
| Electron | ~150 MB | 100 MB+ | great | rejected: 30× the intended size |
| Tauri | ~8 MB | 40–80 MB (webview) | great | rejected: webview runtime stays alive; WebKitGTK pain on Linux |
| Go | ~8 MB | ~10 MB | poor | no first-class native GUI; cgo to ObjC/Win32 required |
| **Rust (native)** | **~7 MB** | **~5 MB** | direct AppKit/Win32/X11 | chosen |

One static binary contains CLI + daemon + renderer (~7 MB, self-contained,
default character embedded). The renderer is a *separate short-lived
process* so idle memory is just the daemon's ~5 MB and a renderer only
exists during the ~8-second walk.

## Layout

```
dribble/
├── core/        # wremind-core: Reminder, Schedule, persistence, config
├── daemon/      # wremind-daemon: scheduler loop, IPC, autostart
├── renderer/    # wremind-render: characters, anim math, platform overlay
│   └── src/platform/{mod,macos,stub}.rs
├── cli/         # dribble binary (clap)
├── characters/  # bundled original footballer sprite pack (embedded)
├── platform/    # OS integration notes/templates (LaunchAgent, XDG, Win32)
├── installer/   # install.sh (curl | sh)
├── tools/       # character-gen: generates the default sprite pack
└── tests/       # cross-crate test notes (tests live in each crate)
```

## Separation of concerns (the important part)

The **engine** (`core/`) has no notion of rendering:

```rust
// core::recurrence — pure, deterministic, fully unit-tested
next_due(&reminder, &state, now) -> Option<DateTime<Utc>>
is_due(&reminder, &state, now) -> bool
```

The **daemon** says "Reminder triggered: Drink water" and spawns
`dribble __render --message "💧 Drink water!"`. The
**renderer** decides what that looks like. You could replace the
renderer with a notification, a sound, a e-ink e-ink friend, anything —
the engine would not change.

## Daemon loop

- 1 s tick (`std::thread::sleep`) — ~0% CPU idle.
- Reloads `reminders.json`/`config.json` on mtime change (CLI edits are
  picked up live; no restart needed).
- For each enabled reminder: if `is_due`, persist `last_fired` **before**
  spawning the renderer → crash-safe duplicate prevention.
- Renderer pool enforces `max_simultaneous` (default 1); overflow
  reminders queue FIFO (bounded at 32) and display sequentially.
- Sleep/wake: a wall-clock jump > 5 s marks a "wake tick"; since
  `is_due` anchors on `last_fired`, a 3-hour sleep with an hourly
  reminder yields exactly ONE walk, then the cadence resumes.
- Single instance: `flock` on `daemon.lock`.
- IPC: Unix domain socket (`daemon.sock`), JSON-lines protocol:
  `ping`, `show`, `stop`. This is the seam for a future
  `dribble serve` / local API — no network server in the MVP.

## Renderer (macOS)

`renderer/src/platform/macos.rs`:

- `NSApplication` with `ActivationPolicy::Prohibited` (no Dock icon,
  cannot steal focus)
- Borderless `NSWindow`: `opaque=false`, clear background,
  floating level, `ignoresMouseEvents`, no shadow
- Custom `NSView` (`define_class!`) draws: rounded speech bubble +
  tail + emoji-capable `NSAttributedString`, ground shadow ellipse,
  current sprite frame
- `NSTimer` @ 60 Hz moves the window across the walk band near the
  bottom of the screen; `WalkPlan` (pure, tested) computes position +
  frame index; auto-`orderOut` + process exit when the walk completes
- Left-facing frames used when present (`walk_left_XX.png`), otherwise
  frames are mirrored via an `NSImage` drawing handler

## Scheduling semantics

| Schedule | JSON | First fire | Cadence |
|---|---|---|---|
| Interval | `{"type":"interval","minutes":60}` | created + 60 m | last_fired + 60 m |
| Daily | `{"type":"daily","time":"09:00"}` | next 09:00 | daily |
| Weekly | `{"type":"daily","time":"18:00","days":["mon","wed","fri"]}` | next matching day | matching days |
| Once | `{"type":"once","at":"2026-09-22T19:00:00-04:00"}` | at `at` | never again |

Each reminder carries an IANA timezone (`chrono-tz`); DST gaps shift the
fire time +1 h, fall-back ambiguity picks the earliest instant.
Clock-rollback clamps anchors to `now`.

## Failure handling

- Atomic writes (tmp+rename, fsync) for all state files
- `.bak` of last-good `reminders.json`; corrupted files are quarantined
  (`*.corrupt-<ts>`) and recovered from backup
- Corrupted `state.json` resets rather than crashing
- Renderer crash cannot take down the daemon (separate process)
- Daemon crash: unfired reminders appear due on restart and fire once

## Future API

`dribble serve` would expose the existing IPC protocol over a
local socket to other applications (`scheduler.addReminder(...)` style)
without invoking the CLI. Deliberately not in the MVP; the protocol
(`daemon/src/ipc.rs`) was designed for it.
