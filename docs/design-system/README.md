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

No icon set. Kind is shown by one text glyph before the label: `›` subcommand, `-` option, `/` path, `·` argument. These render in `text-muted`. The selected row replaces its glyph with `❯` in `accent`. Never make an image or emoji mandatory.

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
