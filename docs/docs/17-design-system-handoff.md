# 17 — Design System & Prototype Handoff

**Status:** Delivered as a working reference. Contrast was computed; the rendered previews, real keyboard use and screen-reader behaviour have **not** been verified ([R-12](13-risks-open-questions.md)).

## What was delivered

| Item | Where | Purpose |
|---|---|---|
| Design system | [`../design-system/`](../design-system/) (zip: `../terminal-companion-design-system.zip`) | Tokens, brand-book README, theme-resolution rules, five reference components with previews |
| Interactive prototype, English | [`../prototype/terminal-companion-prototype.en.html`](../prototype/terminal-companion-prototype.en.html) | Behavioural reference: typing, popup, windowing, inline fallback, theme resolution, settings |
| Interactive prototype, Vietnamese | [`../prototype/terminal-companion-prototype.vi.html`](../prototype/terminal-companion-prototype.vi.html) | Same, translated UI and descriptions |

Both prototype files are self-contained pages that open in a browser; the design system files are plain text. Hosted copies were published as private artifacts during design; this repository is the durable source.

## Design system contents

```text
design-system/
├─ README.md               # usage rules for a consuming agent or developer
├─ theme-resolution.md     # how terminal colours become tokens
├─ tokens.json             # single source of values (ADR-010)
├─ tokens.css              # generated from tokens.json
├─ tools/gen_tokens_css.py # regenerates tokens.css; checked to reproduce it exactly
└─ components/
   ├─ bundle.js, bundle.css, index.d.ts
   └─ Keycap · SuggestionRow · SuggestionPopup · InlineFallback · PermissionNotice
      (each: README.md + preview.html), Cover/preview.html
```

- **Tokens:** colour (dark first, then light) with a usage note on every token, one monospace family, four type styles for rows and footer, spacing, radii, popup sizes, one shadow. Colours are a *fallback palette*; at runtime the terminal's own colours replace `surface-base`, `text-primary` and, when verified, `accent`.
- **Components** are written for the web as the reference implementation of the contract in [07](07-ux-design-system.md). The shipping renderers (egui overlay, inline) implement the same contract; they do not reuse this JavaScript.
- **Regenerate CSS:** `python3 design-system/tools/gen_tokens_css.py design-system > design-system/tokens.css` (proposed CI step DS-01).

## What the prototype demonstrates

| Area | Behaviour fixed by the prototype |
|---|---|
| Keys | ↑↓ move with wrap, Shift+Tab back, Tab inserts and never runs, Esc dismisses (a second Esc releases focus), Enter runs the line as typed, Tab with no popup leaves the page's normal focus order |
| Completion | Commands, `git`/`npm`/`docker` subcommands and options, `npm run` scripts, paths (escaped insert), optional git branches, accent-folded fuzzy match with per-item ranges |
| Windowing | Window follows selection, `n/total` footer, flip above when no room below |
| Display | Overlay vs inline, resolved from mode × environment (native, tmux, Wayland, SSH) × permission × available width |
| Theme | Auto from terminal colours (verified / background only → approximate / none → fallback), dark, light, custom; contrast adjustment with the adjusted tokens listed |
| Privacy | Switches for dynamic generators, network specs, telemetry; history retention locked off; live `config.json` matching [`schemas/config.schema.json`](../schemas/config.schema.json) |

The prototype uses a small hand-written completion set and executes nothing.

## From prototype to implementation

| Prototype piece | Becomes |
|---|---|
| `context()`, `tokenize()`, `matchRanges()` | `companion-core` tokenizer, context resolution and ranker; golden cases in [16](16-implementation-guide.md) |
| `windowStart()` and selection wrap | Daemon/adapter state (`selected_index`, `window_start`) per ADR-009 |
| `derive()`, `resolveTheme()` | `companion-theme` resolver; same contrast rules, adjusted-token report for `doctor` |
| `resolveDisplay()` | Capability probe and display-mode selection ([05](05-terminal-integration.md), ADR-003) |
| Component CSS and tokens | Generated constants from `tokens.json` for the egui overlay and inline renderer |
| Settings panel | `companion config` and the `config.json` schema |

## Known gaps

- Not rendered or tested with VoiceOver, Narrator or Orca. The accessibility contract (stable option ids, status region, no focus) is specified but unproven in a native overlay (SPIKE-06).
- The prototype relies on a hidden text input for key capture; real terminals get keys from the shell adapter.
- Column positioning of the popup assumes one terminal cell per character; wide (CJK) characters before the cursor shift it. The real renderer must use cell widths from the terminal.
- Only the dark and light fallback themes and four test palettes exist. A high-contrast theme and colour-blind-safe status colours are not defined because the UI has no status colours yet; add them if status messages appear.
- The prototype page chrome loads IBM Plex Sans from Google Fonts and falls back to the system font offline; the terminal and popup never use a webfont.
- No logo or product mark exists. The name is a working name.

## Acceptance for this handoff

The design contract is considered stable when DS-02 (render check) is done, U01–U12 pass in at least the inline renderer, and SPIKE-06 has a recorded outcome.
