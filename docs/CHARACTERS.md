# Custom characters

A character is a folder with a manifest and one or more transparent PNG
sprite frames. Install it without touching source code:

```bash
walking-reminder character import ./my-character/
walking-reminder character set my-character
walking-reminder test --character my-character
```

## Layout

```
my-character/
├── character.json
├── walk_01.png            # facing RIGHT, required
├── walk_02.png
├── ...
├── walk_left_01.png       # facing LEFT, optional
├── walk_left_02.png       # (frames are auto-mirrored if absent)
└── ...
```

Frame numbering must be zero-padded (`walk_01.png` … `walk_12.png`).
At least one `walk_XX.png` is required; 6–12 frames at ~10 fps makes a
nice walk cycle.

## character.json

```json
{
  "name": "footballer",
  "description": "Default football character (original art)",
  "frameRate": 10,
  "width": 120,
  "height": 120,
  "walkingSpeed": 110
}
```

| Field | Meaning |
|---|---|
| `name` | display name |
| `description` | shown in `character list` |
| `frameRate` | sprite frames per second while walking |
| `width` / `height` | on-screen size in points (config `size` overrides) |
| `walkingSpeed` | pixels per second (config `speed` overrides) |

PNGs must have an alpha channel. The bubble renders emoji + multiline
text with the system font automatically.

## Making a Messi pack 🐐

The engine has zero Messi-specific code — it only ever reads
`character = "messi"` from config. To personalize:

1. Create frames you are authorized to use (your own pixel art, an
   officially licensed sprite sheet, etc.). **Do not redistribute
   copyrighted assets.**
2. Keep each frame the same square size, transparent background.
3. Name them `walk_01.png` … and optionally mirrored `walk_left_01.png` …
4. Write `character.json` (name `messi`, 6–12 frames).
5. Import:

```bash
walking-reminder character import ./messi/
walking-reminder character set messi
walking-reminder test --character messi
```

The bundled `footballer` is original programmatically-generated pixel
art (see `tools/character-gen`) so the default install uses no
third-party likeness.
