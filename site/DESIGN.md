# Pipeline OS site — design system

The launch site should feel like the app: a dark control room for creative work.
Calm ink surfaces, one teal signal color, monospace for anything the machine says.
Every token below lives as a CSS variable in `assets/site.css`; pages use the names,
never raw values.

Direction taken from the desktop app's own theme (`apps/desktop/src/index.css`) and logo,
not from other websites.

## Color

Dark only. The product is dark-first and every hero screenshot is dark.

| Token | Value | Role |
| --- | --- | --- |
| `--ink-0` | `oklch(0.13 0.014 250)` | Page background |
| `--ink-1` | `oklch(0.16 0.015 250)` | Panels, the app's own background |
| `--ink-2` | `oklch(0.2 0.018 250)` | Cards, code blocks |
| `--ink-3` | `oklch(0.26 0.02 250)` | Hover and raised surfaces |
| `--line` | `oklch(1 0 0 / 9%)` | Hairline borders, dividers |
| `--line-strong` | `oklch(1 0 0 / 16%)` | Input and focused borders |
| `--text` | `oklch(0.95 0.005 240)` | Headings, body |
| `--text-2` | `oklch(0.74 0.02 245)` | Secondary copy |
| `--text-3` | `oklch(0.6 0.02 245)` | Captions, labels (≥4.5:1 on ink-0) |
| `--teal` | `oklch(0.78 0.13 184)` | Primary: buttons, links, live state, focus ring |
| `--teal-ink` | `oklch(0.16 0.03 200)` | Text on teal |
| `--teal-soft` | `oklch(0.3 0.05 190 / 55%)` | Selected rows, active tab |
| `--blue` | `oklch(0.66 0.17 258)` | Accent, logo and code syntax only |
| `--ok` / `--warn` / `--err` | `oklch(0.74 0.15 150)` / `oklch(0.8 0.15 75)` / `oklch(0.68 0.19 25)` | Log status only |

No gradients on text or buttons. The single gradient is the logo mark itself.

## Typography

- **Schibsted Grotesk** (400, 500, 700) for headings and body. Editorial and firm,
  not the default UI sans.
- **JetBrains Mono** (400, 500) for eyebrows, labels, filenames, commands and logs —
  the app speaks in mono whenever it shows machine output.

| Step | Size / line height | Use |
| --- | --- | --- |
| display | `clamp(2.75rem, 6.4vw, 5.5rem)` / 0.98, 700, -0.035em | Hero only |
| h2 | `clamp(2rem, 3.6vw, 3.25rem)` / 1.05, 700, -0.025em | Section titles |
| h3 | `1.375rem` / 1.25, 500 | Sub-heads |
| lead | `1.25rem` / 1.5 | Hero and section intros |
| body | `1.0625rem` / 1.6 | Copy, max 64ch |
| small | `0.9375rem` / 1.5 | Captions, table cells |
| mono-label | `0.75rem` / 1.4, 500, 0.14em tracking, uppercase | Eyebrows (like "CONTINUE WHERE YOU LEFT OFF" in the app) |

## Spacing and layout

- 4px base: 4, 8, 12, 16, 24, 32, 48, 64, 96, 144.
- Content width 1200px; text columns 64ch. Side gutter 16px on phones, 32px from 768px.
- Section spacing 144px desktop, 96px phone. Sections are left-aligned; only the final
  download call is centred.

## Shape and depth

- Radius: 6px (chips, inputs), 11px (buttons, cards — the app's `--radius`), 18px (screenshot frames).
- One shadow, for screenshot frames: `0 40px 120px -40px oklch(0 0 0 / 70%)`, plus a 1px `--line` ring.
- No glass, no blur except the sticky nav.

## Components

- **Button / primary**: teal fill, `--teal-ink` text, 44px tall. Hover lifts brightness; focus is a 2px teal ring with 3px offset; disabled at 45% opacity.
- **Button / secondary**: transparent, `--line-strong` border, `--text`. Hover fills `--ink-3`.
- **Button / quiet**: text-only teal with an arrow.
- **Link**: teal, underline on hover and in body copy.
- **Chip**: mono 12px, `--ink-2` fill, `--line` border — mirrors the app's `game` tag.
- **Card**: `--ink-1` fill, `--line` border, 11px radius. Used sparingly; never three identical cards in a row.
- **Code block**: `--ink-1`, mono 14px, window bar with filename, copy button.
- **Nav**: sticky, blurred ink, logo + wordmark ("Pipeline" + teal "OS" as in the app sidebar), anchors, GitHub, Download.
- **Footer**: hairline top, three short link columns, licence line.

## Imagery and icons

- Real app screenshots only (`docs/screenshots`, converted to WebP), always inside a frame
  with the app's dark chrome. Captions say they show the real app with sample projects.
- Creative apps are named in text (never their logos — trademarks).
- Icons: inline Lucide-style 1.5px strokes, the same set the app uses. No emoji.

## Motion

- The hero's pipeline strip animates a running build (progress, log lines). Everything
  else fades up once on scroll. All motion stops under `prefers-reduced-motion`.
