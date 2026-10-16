# Method Budget brand

How Method Budget looks and sounds, in the app, on methodbudget.com, in the README and
anywhere else it shows up. The tokens live in [site/assets/tokens.css](../site/assets/tokens.css);
the artwork is drawn by [scripts/make-brand.py](../scripts/make-brand.py) and, for the app
icon, [crates/method-app/icons/source/make_icon.py](../crates/method-app/icons/source/make_icon.py).

## The name

- Always **Method Budget**, two words, both capitalized, in running text.
- Never shorten it to initials: they are ambiguous, and a name exists so the product is
  not mistaken for anything else. On second mention "the app" or "it" is fine; plain
  "method" is only the command.
- The command is `method`, in code style. Budget files are `.mthd` files.
- The wordmark is set in lowercase (`method budget`). That is artwork, not spelling: in
  text, write Method Budget.
- Credit "the Method Budget contributors". The copyright line is "Method Production Suite Team and the
  Method Budget contributors", the same pattern the ArtCraft apps use. No personal or
  company names in public material.

## Voice

Plain, confident, specific. Write for someone who budgets films, not for someone who
reads the code.

- Say what a person can do, in their words: "Trace any total down to the lines that make
  it up", not "explainability engine".
- Claim only what is built. Planned work is labelled as planned ("coming soon", "on the
  roadmap"). The feature list in `docs/features.toml` is checked against the roadmap for
  exactly this reason.
- No hype: no "revolutionary", "powerful", "seamless", "blazing". A number beats an
  adjective ("about 5 ms on a 5,000-line budget").
- No em dashes anywhere. Use a colon, a comma, parentheses or a new sentence.
- No exclamation marks.
- Other products: one factual line saying Method Budget is not affiliated, in the FAQ and
  in the README's clean-room note. Never a comparison, a dig, a screenshot or a logo.
- Examples use invented budgets and invented people only.

## The mark

A budget page with its corner turned back. On the page, a column of figures aligned to
the right, as numbers are in a budget, and the accountant's double rule under the total.
The turned corner is the page at half strength: the working underneath. The figures and
the gap around the corner are cut out of the page, so the mark sits on any background.

It is drawn on a 64-unit grid. A separate 16-unit drawing (one figure instead of two,
whole-unit rows) is used for favicons so it stays sharp at 16 and 32 pixels.

| File | Use |
|---|---|
| `site/assets/brand/mark.svg` | The mark, teal, for light backgrounds |
| `site/assets/brand/mark-dark.svg` | The mark, light teal, for dark backgrounds |
| `site/assets/brand/wordmark.svg` | Mark and name, for light backgrounds |
| `site/assets/brand/wordmark-dark.svg` | Mark and name, for dark backgrounds |
| `site/favicon.svg` | The 16-unit mark; switches color with the browser's scheme |
| `site/favicon.ico` | The 16-unit mark at 16 and 32 pixels |
| `site/apple-touch-icon.png` | 180 pixels, white mark on a full-bleed teal square |
| `crates/method-app/icons/` | The app icon set (below) |

Rules:

- Leave clear space around the mark of at least a quarter of its height.
- Don't recolor it outside the palette, outline it, add effects, rotate it, or put
  anything inside the page.
- Smallest sizes: the mark at 16 pixels (use the favicon drawing below 24), the wordmark
  at 18 pixels tall.
- Use the `-dark` files on dark backgrounds. In HTML, a `<picture>` with a
  `prefers-color-scheme: dark` source does this; the README and the site both do.

### The wordmark

`method budget` in lowercase, drawn from strokes (not typed in a font): geometric round
letters, one weight, a 100-unit x-height with tall ascenders. Being drawn, it looks the
same everywhere and ships no font. The mark stands as tall as the ascenders and sits on
the baseline. The name is ink, the mark teal.

### The app icon

The mark in white on a teal rounded square, the figures showing the teal through them.
`make_icon.py` draws the 1024-pixel master from the same geometry as the mark; the
bundle's sizes and formats come from it with the Tauri CLI:

```bash
python3 crates/method-app/icons/source/make_icon.py crates/method-app/icons/source/icon-1024.png
cd crates/method-app && cargo tauri icon icons/source/icon-1024.png -o <scratch dir>
```

Copy back only the files already in `crates/method-app/icons/` (the PNG sizes,
`icon.icns`, `icon.ico`); the CLI also writes Android and iOS sets the app doesn't use.

Regenerate the SVGs, favicon and touch icon with `python3 scripts/make-brand.py`
(`--check` changes nothing and exits 1 when a file is out of date).

## Color

The accent is the desktop app's own teal, so the site and the app read as one product.
Neutrals are warm paper and ink on the light side, the app's own dark surfaces on the
other. All values are in `site/assets/tokens.css`.

| Token | Light | Dark | Use |
|---|---|---|---|
| `--bg` | `#F7F6F2` | `#111417` | Page |
| `--surface` | `#FFFFFF` | `#1B1F24` | Cards, panels |
| `--surface-2` | `#EFEDE7` | `#22272E` | Inset areas, table heads |
| `--ink` | `#1C2228` | `#E4E8EC` | Text |
| `--ink-soft` | `#3A424C` | `#C3CAD2` | Secondary text |
| `--muted` | `#59626D` | `#9AA3AD` | Captions, fine print |
| `--rule` | `#DEDBD3` | `#2B3138` | Dividers (decorative) |
| `--rule-strong` | `#8F897C` | `#67717D` | Borders that carry meaning |
| `--accent` | `#0E7468` | `#2CC7B2` | Links, buttons, the mark |
| `--accent-strong` | `#0A5C53` | `#5ED6C5` | Hover |
| `--accent-ink` | `#FFFFFF` | `#08201D` | Text on an accent fill |
| `--accent-soft` | `#E1F1EE` | `#173A35` | Selection, highlights |
| `--note` | `#8A5A00` | `#E3B341` | Pencil amber: notes, the odd highlight |

### Contrast (WCAG 2.x)

Body text needs 4.5:1 (AA); borders that carry meaning need 3:1.

| Pair | Light | Dark |
|---|---|---|
| ink on bg | 14.84 | 15.01 |
| ink on surface-2 | 13.71 | 12.20 |
| ink-soft on surface-2 | 8.69 | 9.09 |
| muted on bg | 5.72 | 7.23 |
| muted on surface-2 | 5.29 | 5.88 |
| accent on bg | 5.22 | 8.73 |
| accent on surface-2 | 4.83 | 7.10 |
| accent on accent-soft | 4.85 | 5.86 |
| accent-ink on accent | 5.65 | 8.03 |
| ink on accent-soft | 13.77 | 10.07 |
| note on surface | 5.93 | 8.51 |
| rule-strong on bg | 3.22 | 3.21 |

The terminal sample on the site is dark in both schemes (`--term-*` tokens): prompt
`#2CC7B2` on `#14181C` is 8.4:1, comments `#9AA3AD` 7.0:1.

## Type

The system's own faces: San Francisco on a Mac, Segoe UI on Windows, the desktop's sans on
Linux; `ui-monospace` and its fallbacks for figures in code. That is what the app uses, so
nothing is downloaded and the site matches the app on every machine. Figures in tables
use tabular numbers (`font-variant-numeric: tabular-nums`) so columns line up.

Headings are bold and tightly tracked (`letter-spacing: -.02em`); body text is 17 pixels
on 1.6 line height. The wordmark is artwork, so it needs no font.

## Imagery

- No stock photos and no other product's screens, logos or text.
- The home page's app picture is HTML drawn from the app's own layout with an invented
  budget. Real screenshots replace it later: captures of an invented budget
  (`method sample`), never a real one.
- The share card (`site/og.png`, 1200 by 630) is drawn from `site/og-card.html` by
  `scripts/make-og.sh`.
