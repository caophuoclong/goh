# 15 — Research Sources, Licenses & Evidence Limits

**Research date:** 27 October 2026. Sources below are for feature understanding and implementation decisions, not proof that every older app feature/OS guarantee remains current.

## Primary public sources

1. **withfig/autocomplete — source repository & README**: https://github.com/withfig/autocomplete
   - Describes completion specs as trees of `subcommands`, `options`, `args`; lists the repository as MIT licensed; older docs mention macOS Accessibility-based positioning and shell integration.
2. **Archived Fig GitBook autocomplete docs**: https://fig.gitbook.io/fig/autocomplete
   - Documents autocomplete specs, suggestions, insert values, args and original behavior. Historical documentation; do not treat it as a complete modern contract.
3. **withfig/autocomplete-tools**: https://github.com/withfig/autocomplete-tools
   - Useful for understanding historical tooling and framework integrations.
4. **aws/amazon-q-developer-cli**: https://github.com/aws/amazon-q-developer-cli
   - Open-source descendant, dual MIT/Apache-2.0 as indicated in repo; not a build-time/runtime dependency of our product. At research time README states it is not actively maintained outside critical fixes and refers to Kiro CLI.
5. **xterm.js VT features**: https://xtermjs.org/docs/api/vtfeatures/
   - Describes support for OSC 10/11 default foreground/background query and terminal control-sequence behavior; actual support must be tested per emulator.
6. **xterm dynamic color queries (background)**: https://terminfo.dev/extensions/osc-11-bg-color-query
   - Context for OSC 11; use only through safe terminal input handling.

## User-provided historical links

- https://web.archive.org/web/20260129234623/https://fig.io/docs/getting-started
- https://web.archive.org/web/20230828172653/https://fig.io/user-manual

**Research limitation:** The specific Wayback snapshots above could not be opened by the browsing service during drafting. Accordingly, we did not assert that their full content had been verified; historical GitBook and public repositories were used as accessible alternative references.

## Legal & design boundaries

- A similar interaction concept does not authorize copying trademark, logo, proprietary UI resources or undisclosed functionality. Use a new product name and original artwork.
- Retain existing copyright/license notices when importing code or spec data; inspect per-file licenses and transitive packages.
- Native shell APIs, Wayland/compositor protocols, PSReadLine/UI Automation and terminal color responses are version/environment-dependent. The compatibility matrix describes what to *test*, not current achieved support.

## Open fact checks before implementation

Check official API docs/source for current versions of zsh ZLE, Bash Readline `bind -x`, PSReadLine custom handlers, Windows Terminal UI Automation, macOS Accessibility, winit/egui platform support, and target Wayland compositor protocols. Do not build on repo README marketing statements as sole compatibility evidence.

## Design and prototype notes (added in v0.2)

- The design system and prototypes are original work for this project. They contain no Fig or Amazon Q assets, logo or name.
- The "Solarized-style dark" test palette uses the widely published Solarized colour values as an example of an ANSI-accent terminal theme; check the Solarized licence before shipping those values in a product.
- The prototype page loads IBM Plex Sans from Google Fonts for its surrounding page chrome and falls back to the system font offline. The terminal and popup use the platform monospace font and no webfont.
- Contrast figures were computed with the WCAG 2 relative-luminance formula, not measured on rendered output.
