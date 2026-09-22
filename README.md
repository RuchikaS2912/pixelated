# Pixelated ⚽💙

A tiny companion who lives on your computer. Every so often, a little
character **walks across your screen** carrying a speech bubble with your
reminder — then walks off again. He can also sit on your desktop as a
**draggable pet**. No dashboard, no dock icon, no interruption.

```text
                💧 Drink water!
                      ↓
                   ⚽🏃 ───────────────►
```

- **One small native binary** (Rust, ~2 MB) — CLI + daemon + renderer + pet
- **Runs quietly in the background** (~0% CPU idle, ~5 MB RAM)
- **Completely local** — no account, no cloud, no analytics, works offline
- **Transparent, non-intrusive overlay** — no focus stealing, no sound by
  default, auto-dismisses
- **Desktop pet mode** — he idles on your screen; drag him anywhere, he
  remembers where you put him
- **Robust scheduling** — intervals, daily, weekly, one-time; handles
  sleep/wake, DST, timezone changes, missed-reminder coalescing
- **Bring your own character** — any sprite sheet works (see below)

---

## Install on your computer

### macOS / Linux — one line in the Terminal

```bash
curl -fsSL https://raw.githubusercontent.com/RuchikaS2912/pixelated/master/installer/install.sh | sh
```

Prefer to inspect first? (recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/RuchikaS2912/pixelated/master/installer/install.sh -o install.sh
less install.sh && sh install.sh
```

The installer detects your OS + architecture, downloads the right binary,
verifies its SHA-256 checksum, installs it, and runs a health check.

**On macOS it also installs a real app**: "Dribble" appears in
`/Applications` (icon built from your character's sprite) — double-click
it or find it in Spotlight to start everything after quitting.

### From source (works anywhere, no release needed)

1. Install Rust: https://rustup.rs
2. Then:

```bash
git clone https://github.com/RuchikaS2912/pixelated.git
cd pixelated
cargo build --release
```

The binary is `target/release/dribble`. To make it available
everywhere:

```bash
sudo cp target/release/dribble /usr/local/bin/
```

### After installing

```bash
dribble --version    # health check
dribble test         # watch him walk across your screen
```

On macOS, just open **Dribble** from `/Applications` or
Spotlight — the daemon, menu bar item, and desktop pet all come back.

---

## Quick start

```bash
dribble add "Drink water" --every 1h
dribble add "Call Mom" --at 19:00
dribble add "Go running" --days mon,wed,fri --at 18:00
dribble add "Dentist" --at "2026-10-01 09:00"

dribble start         # start the background daemon
dribble enable        # start automatically at login
dribble status
```

An hour later, your character walks across your desktop:
**💧 Drink water!**

### Menu bar control (macOS)

While the daemon runs, a small ⚽ sits in your menu bar:

- **Reminders list** — every reminder with its schedule and next-fire
  countdown; click any to pause/resume just that one (● active · ○ paused)
- **Meeting mode** — ✓ toggle that suppresses walks and hides the pet
  while screen sharing
- **Walk now** — he crosses the screen immediately
- **Pause / Resume all reminders**
- **Show / Hide desktop pet**
- **Quit Dribble** — no terminal needed

The list refreshes every time you open the menu.

### Meeting mode (screen sharing)

A walk **does** appear on a shared screen — so before (or during) a
meeting:

```bash
dribble meeting on    # no walks, pet hidden; reminders queue up
dribble meeting off   # he's back (one coalesced walk if any came due)
```

Also one click in the ⚽ menu bar: **Meeting mode — no walks, hide pet**
(shows a ✓ when active). Reminders aren't lost — anything that came due
while sharing fires once, right after you turn it off.

### The desktop pet

```bash
dribble pet           # he appears on your screen
```

- **Drag** him anywhere — position is remembered
- **Right-click** him (or `dribble pet --stop`) to dismiss
- He idles with a little animation while reminders still walk by on schedule

## Commands

| Command | What it does |
|---|---|
| `add <title> --every 1h / --at 19:00 / --daily 09:00 / --days mon,wed,fri --at 18:00` | create a reminder |
| `list` | table of reminders + next-fire times |
| `remove <id>` / `edit <id> ...` | manage reminders |
| `pause [id]` / `resume [id]` | pause one or all |
| `test` | walk the character across the screen now |
| `pet [--character NAME] [--stop]` | draggable desktop companion |
| `start` / `stop` / `status` | control the background daemon |
| `enable` / `disable` | autostart at login (LaunchAgent / XDG autostart) |
| `config get` / `config set <key> <value>` | settings |
| `character list / set / show / import` | manage sprite packs |
| `doctor` | health check |
| `logs [n]` | recent daemon log lines |

## Configuration

Everything lives in `~/.dribble/`:

```
~/.dribble/
├── config.json      # settings
├── reminders.json   # your reminders
├── state.json       # runtime bookkeeping
├── characters/      # installed sprite packs
└── logs/
```

```bash
dribble config set speed 60         # px/sec (lower = slower stroll)
dribble config set size 240         # bigger character
dribble config set sound true       # soft sound with each walk
dribble config set direction random # left | right | random
```

## Custom characters

A character is a folder of transparent PNG sprite frames + a manifest.
Generate art with any AI image tool using this prompt:

```
Create a pixel-art sprite sheet of a walking character.
- A single horizontal strip with EXACTLY 6 equal-sized square panels
- Same character, one pose per panel of a walk cycle, facing RIGHT
- Identical size/position per panel, feet on the same baseline
- Fully TRANSPARENT background, no ground/shadow/border/text
- Crisp pixel edges, clean silhouette
```

Then:

```bash
dribble character import ~/Desktop/my-character/
dribble character set my-character
dribble test --character my-character
```

See [docs/CHARACTERS.md](docs/CHARACTERS.md) for the manifest format.

## How it works

```
CLI  ─►  reminders.json  ─►  daemon (1s tick, ~0% CPU)
                                   │
                                   ├─► scheduler ─► due? ─► spawn renderer
                                   │                         (short-lived process)
                                   └─◄ IPC socket (status / test / stop)
```

The reminder engine is a pure library with zero GUI knowledge; the
renderer receives "show this message with this character" events. Multiple
simultaneous reminders queue and display sequentially. If the laptop
sleeps through three hourly reminders, exactly one walk happens after
wake. Full details: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) ·
[PRIVACY.md](PRIVACY.md)

## Development

```bash
cargo build --release
DRIBBLE_NO_RENDER=1 cargo test --workspace
./target/release/dribble test

cargo run -p character-gen         # regenerate the bundled sprite pack
```

## Platform support

| OS | Daemon | CLI | Walk overlay | Desktop pet |
|---|---|---|---|---|
| macOS (ARM64/x64) | ✅ | ✅ | ✅ | ✅ |
| Linux (x64/ARM64) | ✅ | ✅ | 🔜 | 🔜 |
| Windows (x64) | 🔜 | ✅ | 🔜 | 🔜 |

## License

MIT — made with 💙
