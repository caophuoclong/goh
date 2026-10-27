# Theme resolution

The popup's colours are resolved per session, from the terminal, not from the OS light or dark setting. A dark editor in a light OS gets a dark popup.

## Token sources

| Token | Derived from | Fallback |
|---|---|---|
| `surface-base` | Terminal default background | The theme's value for this token |
| `text-primary` | Terminal default foreground | Near-white or near-black chosen for contrast |
| `text-muted` | Blend of foreground into background, kept at 4.5:1 or more | Gray at the same floor |
| `surface-selected` | ANSI accent if verified, else a tint of the foreground over the background | A contrast-safe neutral |
| `border-subtle` | Blend of background and foreground | A subtle separator |
| `text-match` | Accent if verified | Primary text with underline and bold |
| `popup-shadow` | Allowed only if the compositor supports it | None |

## Order of resolution

Session override, then user terminal override, then terminal API or profile, then a verified OSC 10/11 answer, then a reliable environment hint, then the fallback theme. Do not invent a 16-colour palette from foreground and background. If only the background is known, derive a legible foreground and mark the source `approximate` in diagnostics.

## Rules

- Query colours only from the shell adapter, with a bounded timeout and a size cap. A reply must never leak escape sequences into the prompt.
- Parse `rgb:RR/GG/BB` through `rgb:RRRR/GGGG/BBBB`, `#RGB` and `#RRGGBB`.
- Contrast is adjusted even when the terminal's own accent fails it: 4.5:1 for text, 3:1 for the selected marker and highlights.
- Refresh on session or prompt change and on `companion theme refresh`. Do not poll.
- Use a translucent backdrop only when contrast is confirmed.
- Config `theme.mode` is `auto`, `light`, `dark` or `custom` (custom needs `foreground` and `background`, optionally `accent`, as `#RRGGBB`).
