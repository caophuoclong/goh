Terminal Companion is autocomplete that looks like part of the terminal it sits in: small, quiet, keyboard-first. It is not a dashboard. Build every surface so it could be mistaken for terminal output.

## Content fundamentals

- Write command text exactly as typed: lowercase, no quotes, no trailing punctuation (`checkout`, `--force-with-lease`).
- Descriptions are one line, sentence case, no full stop, imperative or noun phrase ("Switch branches or restore paths"). Truncate with an ellipsis; never wrap.
- Footer reads `<command> · <kind>` on the left, key hints on the right: `git · subcommands`, `↑↓  Tab  Esc`.
- No emoji, no exclamation marks, no marketing voice. Diagnostics and onboarding are plain statements: what capability is needed, and why.
- UI strings are English first, kept as resource keys so Vietnamese can follow. Handle wide (CJK), combining and emoji characters in descriptions without clipping.

## Visual foundations

- **Colour.** The tokens here are a fallback palette. At runtime `surface-base` and `text-primary` come from the terminal (see Theme resolution). Set body text in `text-primary`, secondary text in `text-muted`, the selected marker, matches and focus ring in `accent`. Use `accent-soft` only for decoration.
- **Selection.** The highlighted row has `surface-selected`, the `❯` marker in `accent`, and `row-label-selected` (weight 600). Never rely on the fill alone.
- **Matches.** Matched characters take `text-match` and are bold and underlined, so they survive a theme with no usable accent.
- **Type.** One monospace family, `mono`, the platform default. Use `row-label` / `row-description` for rows and `footer` for the footer. Do not load a webfont for the popup.
- **Spacing and shape.** Rows are at least `row-height` tall with `space-2` side padding and `radius-row`. The popup is `popup-width-min` to `popup-width-max` wide, with `radius-popup` and a `border-subtle` hairline.
- **Depth.** `popup-shadow` only on the native overlay, only if the compositor allows it. Inline fallback is flat.
- **Rows.** Six to eight visible by default, four to twelve configurable. Flip above the caret line when there is no room below; never cover the input.
- **Motion.** None, or an 80–120 ms fade that is off under reduced motion. Nothing animates while hidden. No spinner: if results are not ready, render nothing.
- **Focus.** A solid 2px `focus-ring` outline on any focusable control; 3:1 or better in both themes.

## Iconography

No icon font, image or emoji. Kind is shown by one single-cell text glyph before the label, so it survives any font and `NO_COLOR`:

| Kind | Glyph | Inline colour (ANSI, follows the terminal palette) |
|---|---|---|
| command | `⌘` | cyan |
| subcommand | `↳` | magenta |
| option that takes a value | `◇` | blue |
| flag (option without a value) | `⚑` | yellow |
| argument | `●` | green |
| directory | `▸` | blue |
| file | `◦` | none |
| history line | `↺` | `text-muted` |

With learned choices on, the first three suggestions picked before replace their glyph with `①` `②` `③`, in the kind's colour. None are shown while learning is off. The popup does not draw glyphs yet.

## Inline menu

The inline menu is the main surface outside iTerm2, Terminal.app, VS Code and Claude: tmux, ssh, Ghostty, WezTerm, Kitty and Linux terminals. It is also the one a screen reader can read. By default it is a floating box: a one-cell frame in `text-muted`, filled with `surface-base`, the selected row in
`surface-selected`, starting under the first letter of the word being completed and moved left only as far as the
terminal's width needs. `display.inlineStyle: "flat"` keeps the frameless rows for screen readers. The frame is shape,
not colour: it stays under `NO_COLOR`.

- **Rows.** `❯`, glyph, label. Labels share one column as wide as the longest label in the list (at most 32 cells), so the menu does not shift while scrolling.
- **Selection.** `❯` and the label in `accent`, bold. Never colour alone: the marker and weight carry it under `NO_COLOR`.
- **Matches.** Bold and underlined, in `text-match` when the theme gives one.
- **Risk.** A warning (`⚠ overwrites the remote branch · context: prod`) follows the label column on every row, bold yellow, so it is seen before it is selected.
- **Detail line.** Below the rows: the selected suggestion's description (without the warning, already on its row), or its kind when it has none. In `text-muted`.
- **Footer.** Position (`3/24`, `50/50+` when more were cut) and key hints, in `text-muted`. The hints shorten, then go, as the terminal narrows.
- **No motion.** Moving the selection redraws at once; nothing waits or animates.

## Components

- `SuggestionPopup` is the overlay: rows plus footer. Use it where a native overlay is available.
- `SuggestionRow` is one row; use it only inside the popup.
- `Keycap` shows a key in the footer and onboarding text.
- `InlineFallback` is the same suggestion as ghost text on the prompt line, for Wayland, tmux, remote and any terminal without a caret API. Prefer it to a broken overlay.
- `PermissionNotice` is the single quiet line that asks for an OS capability; say which one and why. Never a modal.

## Behaviour that the visuals must not contradict

- Tab accepts the highlighted suggestion and never runs the command. Enter keeps the shell's own behaviour. Esc dismisses and returns the native editor.
- Hidden means no background work and the terminal's keyboard untouched.
- An empty result hides the popup; do not show an empty state or a toast.
