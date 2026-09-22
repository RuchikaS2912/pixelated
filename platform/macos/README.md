# macOS integration

The daemon is started at login via a **LaunchAgent** (no admin/root
required). `walking-reminder enable` writes:

```
~/Library/LaunchAgents/com.walking-reminder.daemon.plist
```

Template (rendered by the CLI with the real binary path):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.walking-reminder.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/walking-reminder</string>
        <string>__daemon</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Background</string>
</dict>
</plist>
```

`KeepAlive` is intentionally off so `walking-reminder stop` means stop.
The overlay renderer is a plain NSWindow process — it needs no Screen
Recording, Accessibility, or Full Disk Access permissions.
