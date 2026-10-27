The native overlay: a window of up to `maxRows` rows (default 8) above a footer with the context, a position count and key hints.

Provide `items` (each with optional per-item `matches` ranges), `context` (for example `git · subcommands`), `selectedIndex`, `id` and, to keep scrolling steady, the previous `windowStart`. It renders nothing when `items` is empty: never show an empty state.

**Windowing.** The visible window always contains the selected row and moves one row at a time. When there are more items than rows the footer shows `n/total`. At either end the selection wraps (Down on the last item selects the first, Up on the first selects the last) and the window jumps with it. The shell adapter owns the index; the popup only draws it.

**Width and truncation.** Width follows content between `popup-width-min` and `popup-width-max`, never wider than the space available. The description shrinks first and disappears completely before a label is shortened. A label that still does not fit ends with an ellipsis; its full text is in the row's `title` and in the screen-reader announcement. Where the host has less than `popup-width-min` free, do not draw the popup: use `InlineFallback`.

**Input and accessibility contract.** The terminal keeps input focus at all times; the popup is never focused and has no tab stop. The shell adapter handles ↑↓, Tab and Esc and passes `selectedIndex` in. The listbox has the given `id` and each option `"<id>-opt-<index>"`, so a host that can expose them (an overlay window, an IDE) sets `aria-activedescendant` on the input to the selected option's id. The popup contains a polite status region that reads the selected suggestion and its position. Update `selectedIndex` on selection change only, never per keystroke, so announcements stay throttled.

Use it only where an overlay is verified. Do not add a title bar, a spinner or a close button.
