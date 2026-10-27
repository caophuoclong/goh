# 13 — Risk Register, Assumptions & Open Questions

## Top risks

| ID | Probability | Impact | Risk | Mitigation / trigger |
|---|---|---|---|---|
| R-01 | High | Critical | Bash cannot provide non-invasive per-keystroke editor events | R0 spike; revert to Tab-triggered inline mode |
| R-02 | High | High | Wayland disallows reliable external popup positioning | inline fallback; no general overlay claim |
| R-03 | Med | High | PSReadLine custom handlers conflict with user config/history | integration tests + opt-in bindings + revert path |
| R-04 | Med | High | OSC theme queries pollute terminal input | safe in-adapter query, timeout, fallback, test tmux |
| R-05 | Med | High | Editor cursor range incorrect for Unicode/escaping | explicit byte-index contract, property/fuzz tests |
| R-06 | Med | High | Native UI steals focus in split panes/multi-monitor | no-activate window, confidence-based anchor fallback |
| R-07 | Med | High | Imported community specs contain executable logic/license obligations | static importer only + attribution + legal review |
| R-08 | Med | Med | UI runtime overshoots lightweight memory budget | measure early; one daemon per user and lazy renderer |
| R-09 | Med | High | Installer corrupts user profile | bounded markers, preview/backup/diff tests |
| R-10 | Med | High | Command text leaks through diagnostics | zero-raw-buffer logging contract, audit crash reporting |
| R-11 | Med | High | Native overlay cannot expose an accessibility tree (egui/winit), so screen-reader users get no popup announcements | SPIKE-06; inline fallback is the accessible path; kill criterion in ADR-002 |
| R-12 | Med | Med | Design system and prototype were not checked in a rendered browser or with assistive tech; contrast is computed, not observed | render check and screen-reader pass before treating the UI contract as final |
| R-13 | Low | Med | Prototype behaviour drifts from the Rust engine (ranking, quoting) | golden tests derived from the prototype; docs 06 names it the behavioural reference |

## Required decisions — product owner / tech lead

Recommended defaults are listed so a decision can be a quick yes/no. They are proposals, not decisions taken.

| # | Decision | Recommended default |
|---|---|---|
| 1 | Brand & license | Pick a new name before R3; keep Terminal Companion as the repo codename. Permissive licence (MIT or Apache-2.0) with attribution policy for imported specs |
| 2 | v0.1 compatibility | macOS 14+ zsh on Terminal.app and iTerm2; Windows 11 PowerShell 7 on Windows Terminal; Ubuntu LTS bash on GNOME Terminal (X11 overlay, Wayland inline) |
| 3 | Display mode | Accept inline fallback on Wayland, tmux, SSH and when permission is missing |
| 4 | Fig spec reuse | Static importer into our JSON format (P1); no TypeScript runtime |
| 5 | Dynamic suggestions | Off by default; read-only built-ins (git branches) behind an explicit setting; no downloaded generators |

Original wording of the decisions:

1. **Brand & license:** working name, repository license, attribution policy, trademark boundary.
2. **Minimum shipped compatibility:** exact OS versions, shell versions, and terminal models to support in v0.1.
3. **Display mode expectation:** confirm acceptance of inline fallback on Wayland/unsupported combos rather than universal native popup.
4. **Fig spec reuse:** choose static importer to own JSON format (recommended) vs full compatibility with TypeScript runtime (expensive and riskier).
5. **Dynamic suggestions:** confirm defaults and explicit consent for any subprocess-based provider.

## Questions for later, not blockers to R0

- How to handle tmux/screen theme transparency and SSH remote command context without installing software remotely?
- Does user want fish and Windows Git Bash in P1? What counts as 'most terminals' measured by actual target users?
- Should commands be suggested automatically on every keystroke or only after Tab on fragile shells?
- User-configured custom themes, transparency, fonts, icons — how much matching is practical across each terminal?
- Should the product add Fig's broader workflows (scripts, dotfiles, snippets, workflows/store) or remain autocomplete-first?
- Update checker: strictly manual vs opt-in non-intrusive notification?

## Hypothesis validation worksheet

| Hypothesis | Success evidence | Decision if false |
|---|---|---|
| H01 zsh supports safe keystroke callbacks | 10k event buffer parity, measured overhead | modify adapter architecture |
| H02 macOS overlay can anchor | ≥98% correct fixture windows, no focus theft | inline-first on affected terminals |
| H03 PowerShell event handling works | keyboard history/undo test green | bind only explicit completion gesture |
| H04 users value automatic theme matching | usability tests with ≥4 palettes and override | simplified palette selector |
| H05 Rust+egui meets perf | reference hardware profile under budget | switch UI/daemon design before expanding features |

## Sign-off checklist

- [ ] Exact MVP support table approved
- [ ] Architecture decisions after spikes marked Accepted/Rejected
- [ ] Security/privacy policy approved
- [ ] All P0 AC mapped to QA cases
- [ ] Team agrees on 'works offline/no account' and license requirements
