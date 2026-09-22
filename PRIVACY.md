# Privacy

Walking Reminder is completely local. Specifically:

- **Works offline.** After installation, no network access is used or
  required. The daemon never opens a socket to the outside world; the
  only network use is the installer downloading the binary + checksums
  over HTTPS.
- **No account, no login, no cloud sync.**
- **No analytics, telemetry, or crash reporting.** Nothing is collected.
- **All data stays on your machine** in `~/.walking-reminder/`
  (reminders, state, config, logs, characters). Delete that folder and
  everything about you is gone.

## Permissions used

| Permission | Used? | Why |
|---|---|---|
| Screen Recording (macOS) | **No** | The overlay is our own window; we never capture other windows/screens. |
| Accessibility (macOS) | **No** | The overlay ignores mouse/keyboard entirely. |
| Notifications | **No** | We draw our own overlay instead of system notifications. |
| Full Disk Access | **No** | We only read/write `~/.walking-reminder/`. |
| Network | Installer only | Downloading the release artifact + SHA-256 checksums. |
| User LaunchAgent / autostart entry | Optional (`enable`) | Starts the daemon at login, in your own user session — no root/admin. |

## Logs

`~/.walking-reminder/logs/walking-reminder.log` contains reminder
titles and fire times (local only). Review or delete any time with
`walking-reminder logs`.
