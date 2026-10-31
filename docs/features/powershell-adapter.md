---
id: powershell-adapter
title: PowerShell: suggestions as you type
status: experimental
since: unreleased
updated: 2026-10-31
area: shells
shells: [pwsh]
settings: [display.live]
commands: [goh init pwsh]
env: [GOH_LIVE]
files: [~/.cache/goh/powershell/]
---

# PowerShell: suggestions as you type

On PowerShell 7.2+ with PSReadLine 2.2.2+, goh's suggestions appear in PSReadLine's list under the line while you
type, marked `[goh]`. Load it with `. /path/to/goh.ps1` in `$PROFILE`.

## Behaviour

- When history is one of PSReadLine's prediction sources, the list shows goh's suggestions next to your history.
- Each suggestion is the whole line with the current word completed; ↑↓ pick one, Enter runs it. Its description
  shows as the tooltip.
- A suggestion whose text holds a control character (an escape sequence, a newline) is dropped, and control characters in
  a tooltip show as spaces: what the daemon sends is checked, as in the other shells.
- goh adds `Plugin` to PSReadLine's prediction source (`None` becomes `Plugin`, `History` becomes
  `HistoryAndPlugin`) and switches the view to the list view. F2 switches views.
- `display.live: false` (`GOH_LIVE=off`) turns this off: nothing is compiled or loaded, and PSReadLine's prediction
  source and view are left as they are.
- The first load compiles a small predictor into `~/.cache/goh/powershell/` (or `$XDG_CACHE_HOME/goh/powershell/`;
  a few seconds, again only when the predictor itself changes). A broken cached file is rebuilt.
- goh starts its daemon at shell start when the daemon's socket is missing.
- Tab still completes through goh as before, in every PowerShell including Windows PowerShell 5.1.

## Limits

- No popup and no floating box: the list is PSReadLine's own.
- A suggestion that takes longer than PSReadLine's 20 ms is not shown.
- Older PowerShell or PSReadLine: Tab completion only.
- Tested with PowerShell 7.6.9 and PSReadLine 2.4.5; the list on screen has not yet been checked by a person.

## Changes

- unreleased: first release.
- unreleased: suggestions containing control characters are dropped and tooltips are cleaned, as in the other shells.
