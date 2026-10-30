# Linux popup backends

Goh separates caret verification from rendering. Completion, themes, candidate selection and the
inline menu remain shared. A desktop name is diagnostic information, never permission to draw.

## Current backend

X11 and XWayland terminals can use the same X11 surface backend. Requirements:

- The local terminal exports its X11 `WINDOWID` to the shell.
- Its window is viewable, owns X input focus, and its `_NET_WM_PID` is an ancestor of the popup.
- Its AT-SPI application has that same process ID and exposes a focused, showing `Terminal` role
  with a stable character rectangle at its caret. Screen coordinates must lie in the terminal
  client area and a RandR monitor. End-of-buffer positions without a rectangle are rejected.
- No SSH, tmux or screen session. These need an independent pane/remote identity provider.

An X11 connection alone does not establish any of these facts. In particular, `DISPLAY` inside a
Wayland desktop does not authorize anchoring to a native Wayland terminal. Missing evidence returns
an explicit inline decision. `GOH_DISPLAY=overlay` cannot override verification.

The popup uses an override-redirect tooltip window with activation disabled and mouse passthrough.
The adapter retains keyboard navigation. Linux wheel navigation is not implemented. X11 uses physical
pixels throughout, pins winit's X11 scale to one, and clamps the popup to the caret's RandR monitor.
The popup revalidates its original window and AT-SPI object while open, hiding on focus/visibility
loss and following verified movement. AT-SPI discovery/traversal is bounded to 200 ms, 512 objects
and 24 levels. No terminal query is sent while a shell editor is reading input.

AT-SPI access uses the desktop accessibility D-Bus service through Rust bindings. No AT-SPI package
is installed on macOS; dependencies are compiled only for Linux. The service and terminal accessibility
support must be available on Linux. The X11 renderer also needs libxkbcommon-x11 and EGL/OpenGL runtime libraries. Goh does not change accessibility settings or enable screen readers.
`goh-popup --resolve` and `--probe` explain the chosen path. `GOH_POPUP_FONT` and popup font config use
Fontconfig on Linux when a font is requested.

## Native Wayland work remaining

Native Wayland currently returns inline. Sway, Hyprland, KWin and GNOME are detected for diagnostics;
compositor detection is not implemented surface support. In particular, this change does not claim
that a winit Wayland top-level window is a positioned popup.

A native surface backend must negotiate layer-shell where available, request no keyboard interactivity,
verify terminal process/pane identity and coordinate transforms with a compositor or terminal provider,
and convert global caret coordinates to output-local coordinates. KWin needs a supported cooperation
path; GNOME needs a Shell extension or terminal integration. Unsupported protocols or fractional-scale
coordinate uncertainty must continue to return inline. These backends and their real compositor tests
are separate follow-up implementation work, not validated by the X11 tests.

## Validation

CI runs the whole Linux workspace and a real VTE terminal under D-Bus, Xvfb and Openbox,
then the same terminal as an XWayland client under headless Sway. The integration
fixture launches the popup from the terminal's child process, requires an actual verified AT-SPI caret,
checks X input focus before/after showing, inspects override-redirect policy, verifies visibility and
hide-on-unmap, and checks native Wayland-without-WINDOWID fallback. The XWayland run checks a real
Wayland compositor session, rather than merely setting `XDG_SESSION_TYPE`.

Run on a Linux desktop or VM with the dependencies from `.github/workflows/ci.yml`:

```sh
cargo build --locked -p goh-popup
LIBGL_ALWAYS_SOFTWARE=1 NO_AT_BRIDGE=0 dbus-run-session -- xvfb-run -a sh -c \
  'openbox >/tmp/openbox.log 2>&1 & python3 crates/goh-popup/tools/linux-x11-test.py target/debug/goh-popup'
```

A reproducible Linux container environment is in `crates/goh-popup/tools/Dockerfile`.

Before claiming broad desktop support, manually verify real terminals, split panes, end-of-line and
empty prompts, monitor movement/scaling, scrolling, dismissal and focus switching in UTM. Xvfb is
regression coverage for its tested configuration, not proof of every terminal/compositor combination.

Local verification uses a scale-one VTE terminal in Debian bookworm containers on Apple silicon.
The X11/Xvfb and XWayland/Sway fixtures both pass. macOS library tests also pass when run serially;
the complete macOS suite still has a failing Bash PTY case,
`bundled_commands_register_and_complete_without_user_specs`, which is not counted as passing validation.
