# Brand

## Idea
A U-shaped tray with a square lifting out of it: something taken out of its container. The tray is also the letter *u*, and the same shape (inverted) is the *n* in the wordmark. The wordmark is lowercase, built from the same rounded-corner strokes, no font involved.

## Files (`assets/brand/`)
| File | Use |
|---|---|
| `unpack-symbol.svg` / `-dark` | Symbol on light / dark backgrounds |
| `unpack-symbol-small.svg` | Heavier cut for 16-24 px (favicon, titlebar). Also `app/public/favicon.svg` |
| `unpack-symbol-black.svg` / `-white.svg` | One-colour use (print, embroidery, stamps) |
| `unpack-lockup.svg` / `-dark` | Symbol + wordmark, horizontal |
| `unpack-lockup-animated.svg` | Looping intro, used in the README. CSS inside the SVG, follows light/dark |
| `app-icon.svg` / `.png` | 1024 px tile (dark squircle). Source for `pnpm tauri icon` |
| `build.py` | Generates all SVGs and `app/src/components/logoPaths.ts`. Edit geometry there, not in the outputs |

## Colour
| Role | Light | Dark |
|---|---|---|
| Tray and wordmark | `#1f2328` | `#e6edf3` |
| Square | `#238636` | `#3fb950` |
| App icon tile | `#161b22` | |

These match the app theme tokens (`--strong`, `--accent`, `--accent-ink`). In the UI the logo reads the tokens, so it follows the theme.

## Rules
- Clear space: one square width (64 units of the 256 grid) on every side.
- Minimum size: symbol 16 px (use the small cut below 32 px), lockup 96 px wide.
- One colour: use the black or white symbol; the square then becomes part of the same colour.
- Do not stretch, recolour the tray green, rotate, add shadows or outlines.

## Motion
Same sequence in the splash (`Splash.tsx`, `logo.css`) and the README SVG (about 2.3 s, then hold):
1. Tray drops in and settles (0.1 s).
2. Square rises out of the tray, overshoots, settles; the tray squashes slightly (0.75 s).
3. Letters rise in one by one, 70 ms apart (1.3 s).
4. Tagline fades in; the splash fades out at 2.8 s. Click skips it.
`prefers-reduced-motion` shows the final frame and closes the splash after 0.4 s.

## Regenerate
```bash
python3 assets/brand/build.py
# then, if the icon changed:
python3 <logo-design skill>/scripts/render_png.py assets/brand/app-icon.svg --out-dir assets/brand --size 1024
cd app && pnpm tauri icon ../assets/brand/app-icon.png && rm -rf src-tauri/icons/android src-tauri/icons/ios
```

## Open
- No trademark or reverse-image search done. Do one before a public release.
- Icon not yet seen in a real GNOME/KDE launcher or the `.deb` **(unverified)**.
