# 07 — UX/UI Specification & Terminal Theme Design System

## Design intent

A small, elegant popup that feels like an extension of the current terminal, **not a floating SaaS dashboard**. Respect native keyboard workflow, terminal's color identity, pixel density and contrast. No login wall or distracting animations.

## Information hierarchy

```text
┌────────────────────────────────────────────────────┐
│  ❯ checkout       Switch branches or restore paths │
│    cherry-pick    Apply changes from a commit       │
│    check-ignore   Debug ignore behavior             │
├────────────────────────────────────────────────────┤
│  git  ·  subcommands                 ↑↓  Tab  Esc  │
└────────────────────────────────────────────────────┘
```

Schematic only. Avoid icon assets/emojis as mandatory UI; prefer tiny vector symbols or text and fallback fonts. Each row shows `label`, optional `type` icon, single-line `description`; truncate gracefully; a help line is optional and can be hidden on short viewports.

## States

- **Hidden**: zero background work in renderer, terminal keyboard untouched.
- **Loading**: normally don't render (latency target low); avoid spinner flicker.
- **Suggestions**: ≤8 rows initially (configurable 4–12). With more results the window follows the selection one row at a time and the footer shows `n/total`; mouse optional.
- **Selected**: explicit highlight distinct from terminal text selection.
- **Empty**: hide, no modal toast.
- **Unsupported**: prefer inline/native completion, diagnostics accessible by command rather than intrusive banners.
- **Permission required**: unobtrusive onboarding that says which OS capability is needed and why.

## Keyboard behavior

| Input | Default behavior |
|---|---|
| Up/Down | Move popup highlight only when popup owns navigation context and shell adapter can safely delegate. Selection **wraps** (Down on last → first, Up on first → last) and the window jumps with it |
| Tab | Accept highlighted completion; never auto-run command. In zsh live mode, use native completion when the menu is closed; only `GOH_LIVE=off` uses Tab to open it |
| Shift+Tab | Previous selection where shell supports safely |
| Esc | Dismiss popup, restore native editor |
| Enter | After explicit menu navigation, insert the selected completion without executing; before navigation use the shell's existing behavior. Typing resets this intent |
| Right / Left | In zsh, Right accepts the highlighted completion; Left dismisses and performs the original cursor movement. Original bindings apply when the menu is closed |
| Ctrl+C / Ctrl+D | Pass through shell semantics; close suggestion UI |
| Mouse | Optional select, but keyboard-only complete |

Do not install global hotkeys/keyloggers. Key behaviors may need platform-specific compromises; CLI docs must state defaults.

## Theme mapping: auto first

| Token | Source / derivation | Fallback |
|---|---|---|
| `surface.base` | Active terminal default bg (OSC 11 / known adapter) | palette neutral dark/light |
| `text.primary` | Active terminal default fg (OSC 10) | contrast-selected near-white/near-black |
| `text.muted` | blend primary with background **while preserving readable contrast** | gray with >= target contrast |
| `surface.selected` | derive from ANSI accent if verified, else tinted fg/bg blend | contrast-safe neutral selection |
| `border.subtle` | bg/fg blend | subtle separator |
| `text.match` | accent (if verified), otherwise primary + underline/bold | underline without color dependency |
| `shadow` | transparent light shadow if allowed by compositor | none |

**Actual colors are resolved per session**, not global OS dark/light alone. A dark editor in a light OS must still receive a dark popup if terminal reports it.

### Color resolution order

`session override > user terminal override > terminal API/profile > verified OSC 10/11 > reliable environment hint > fallback`. For ANSI accent colors, use OSC 4 only when safe; don't invent a 16-color palette based on fg/bg. If only background is known, derive legible foreground heuristically but label theme source as `approximate` in diagnostics.

### Technical details

- OSC queries may be ignored, delayed, blocked by tmux or interpreted by a different TTY. Query only from shell adapter with response handling that cannot leak escape sequences into prompt.
- Parse `rgb:RR/GG/BB` through `rgb:RRRR/GGGG/BBBB`, `#RGB`, `#RRGGBB`, with max response size and bounded timeout.
- Respect terminal color underlays and opacity if known; transparent/translucent backdrop only when contrast confirmed.
- If dynamic theme changes are not observable, refresh on session/prompt change and explicit `companion theme refresh`. No aggressive polling.

## Typography & layout

- **Font:** prefer terminal monospace font if available via trusted API; otherwise platform monospace default. Do not attempt to read arbitrary app config files without permission.
- **Size:** terminal's font size where known; fallback 12–14 px equivalent with OS DPI scaling.
- **Width:** 320–520 px equivalent adaptable to available screen; min row height ~28 px for pointer; use dense mode option.
- **Rows:** 6–8 visible default, configurable 4–12. Never cover editor input if avoidable; flip above caret if lower viewport insufficient.
- **Motion:** none by default or 80–120 ms fade respecting reduced-motion; no persistent GPU updates while hidden.

## Accessibility & localization

- Keyboard navigation + focus state; high contrast settings and icon-independent labels.
- WCAG AA-like targets (4.5:1 normal text, 3:1 non-text highlights) *as design targets*, with contrast adjustment even if raw terminal accent is low contrast.
- Ensure screen readers don't receive duplicated live announcements every keystroke; throttle.
- Handle Vietnamese, CJK, emoji width, Unicode combining marks, and RTL descriptions as best effort; index text safely.
- UI language MVP English or Vietnamese/English resource keys; docs bilingual later.

## UX acceptance walkthrough

1. Terminal dark green-on-black -> popup shares dark surface/green-ish primary with readable highlight.
2. Terminal light paper-like palette -> popup becomes light without flipping based on OS theme.
3. OSC unsupported -> usable fallback, terminal input not polluted.
4. Window moved across monitors with different scale -> popup correct placement/size or inline fallback.
5. Terminal/IDE gains focus or user starts Vim/less -> popup hidden.

## Component contract (v1)

The implemented source of truth is the design system under `design-system/` (see [17 — Design System & Prototype Handoff](17-design-system-handoff.md)). Token names there use hyphens (`surface-base`) where this document writes `surface.base`; they are the same tokens.

**Ownership.** The popup is presentational. The shell adapter and daemon own the suggestion list, the selected index, key handling and focus; the UI only draws the state it is given. The UI never inserts text itself.

### Data the UI receives

| Field | Meaning |
|---|---|
| `items[]` | Ordered suggestions: `label`, optional one-line `description`, `kind` (`subcommand`, `option`, `path`, `argument`) |
| `match_ranges` (per item) | Matched parts of `label`. Each result carries its own ranges because fuzzy matches are not always a prefix. The protocol sends UTF-8 byte ranges ([06](06-completion-engine.md)); the renderer converts them to grapheme-cluster boundaries so combining marks are never split. The reference web component takes code-point ranges |
| `selected_index`, `window_start` | Selection and the first visible row, owned by the adapter/daemon so the window moves one row at a time |
| `context` | Footer text such as `git · subcommands` |
| `display` | Resolved mode: `overlay` or `inline`, with the reason (diagnostics only) |

### Windowing

The visible window always contains the selected row. It moves one row at a time; the footer shows `n/total` when there are more results than rows. Arrow navigation wraps at either end and the window jumps with it. Wheel/trackpad navigation stops at either end. The macOS overlay forwards relative wheel movement to the shell with a per-query id; only the shell changes selection, so accepting always inserts the highlighted item. Wheel handling is confined to the visible popup bounds and does not activate the window. The visible row limit is independent of the query limit (up to 50 results).

### Width and truncation

- Width follows content between 320 and 520 px equivalent, never wider than the space available.
- The description shrinks first and disappears completely before a label is shortened.
- A label that still does not fit ends with an ellipsis. The full label is exposed as a tooltip/accessible name and in the selection announcement.
- Where less than 320 px is available, do not draw the popup; use the inline fallback.

### Focus and accessibility

- The terminal keeps input focus at all times. The popup is never focused and has no tab stop.
- Each option has a stable id (`<popup-id>-opt-<index>`). Hosts that can expose accessibility attributes (overlay window, IDE) point the input's active-descendant at the selected option.
- One polite status region announces the selected suggestion and its position. Update it on selection change only, never per keystroke.
- Selection is shown by the `❯` marker and weight 600 as well as the fill, so it survives low-contrast or forced-colour terminals.
- Matches are bold and underlined as well as accent-coloured.
- Open risk: whether the native overlay (egui/winit) can expose this tree to screen readers is untested; see SPIKE-06 in [14](14-backlog.md).

### Inline fallback

The shell adapter, not the UI, decides what Tab inserts. It supplies `before` (line text before the cursor), `insertion` (exactly the text it will insert, already quoted or escaped for the shell) and `after` (line text after the cursor, shown unchanged). The UI does not derive the insertion from the last word. If the adapter has no valid insertion, nothing is drawn. Use inline on Wayland, tmux, remote sessions, below 320 px, and when the overlay permission is missing.

### Permission notice

One quiet line naming the OS capability and why it is needed. After a refusal it adds a one-sentence recovery step (for example "Enable it in System Settings, then run `companion doctor`"). Never a modal, never repeated on every prompt.

### Theme implementation

`tokens.json` is the single source of values; `tokens.css` and any Rust constants are generated from it. At runtime the resolver ([theme resolution](../design-system/theme-resolution.md)) replaces `surface-base`, `text-primary` and, when verified, `accent` with the terminal's own colours, then derives the rest and adjusts contrast: 4.5:1 for text, muted text and matches on both the base and selected surfaces. If a terminal colour had to be adjusted, diagnostics list which token. Themes are redeclared wherever the theme changes, including aliases (`text-match`, `focus-ring`), so a nested light container never inherits a dark accent.

## Design artifacts

| Artifact | Status |
|---|---|
| Design system (tokens, README, 5 components with previews, cover) | Delivered, `design-system/` |
| Keyboard/interaction prototype (EN and VI) | Delivered, `prototype/` |
| 4 theme screenshots | Covered by the prototype palettes: dark green, light paper, low-contrast (contrast-adjusted), ANSI-accent. Static screenshots not yet captured |
| Fallback palette contrast | Computed: text, muted and accent exceed 4.5:1 on both row backgrounds in both themes |
| Rendered check of previews, real keyboard and screen-reader behaviour | **Not yet verified** |
| High-fidelity Figma mockups | Optional; not required now that the design system is code |
| Overlay-vs-inline usability test | Still needed after the spikes |

Command logos are bundled local Devicon assets, with a reserved icon column in the row width calculation.
Aliases share their tool's logo, and nested command rows retain the parent brand. Commands without a bundled logo use a muted, font-independent `>_` terminal outline in the same icon column. The footer's Enter hint follows the adapter's `enter_accepts`
flag; the renderer never decides whether Enter accepts or executes.
