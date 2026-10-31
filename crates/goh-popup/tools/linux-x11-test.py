#!/usr/bin/env python3
"""Real VTE/AT-SPI + X11 regression. Run under dbus-run-session + Xvfb + Openbox."""
import json
import os
import pathlib
import selectors
import subprocess
import sys
import tempfile
import time


def command(*args):
    return subprocess.check_output(args, text=True).strip()


def resolve(binary, stage="initial", within=0):
    """The popup's decision, asked again for up to `within` seconds until it is overlay (the window manager and the
    accessibility tree settle at their own pace on CI runners)."""
    deadline = time.monotonic() + within
    while True:
        focus = command("xdotool", "getwindowfocus")
        result = subprocess.run([binary, "--resolve"], capture_output=True, text=True, timeout=5)
        data = json.loads(result.stdout)
        if data["display"] == "overlay" or time.monotonic() > deadline:
            break
        time.sleep(0.2)
    assert data["display"] == "overlay", (stage, focus, os.environ.get("WINDOWID"), data)
    return data


def popup_windows():
    found = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^goh-popup$"], capture_output=True, text=True)
    return found.stdout.split()


def wait_until(condition, timeout):
    """Poll instead of sleeping a fixed time: a debug build with software GL is slow on CI runners."""
    deadline = time.monotonic() + timeout
    while not condition():
        if time.monotonic() > deadline:
            return False
        time.sleep(0.1)
    return True


def child(binary, result_file):
    try:
        time.sleep(2)  # allow the terminal accessibility tree to register
        if os.environ.get("GOH_TEST_SPLIT"):
            result = subprocess.run([binary, "--resolve"], capture_output=True, text=True, timeout=5)
            data = json.loads(result.stdout)
            assert data["display"] == "inline" and "multiple visible terminal panes" in data["reason"], data
            pathlib.Path(result_file).write_text("ok")
            return
        initial = resolve(binary, within=15)
        def geometry():
            return dict(line.split("=", 1) for line in command("xdotool", "getwindowgeometry", "--shell", os.environ["WINDOWID"]).splitlines())
        before = geometry()
        subprocess.check_call(["xdotool", "windowmove", "--sync", os.environ["WINDOWID"], str(int(before["X"]) + 40), str(int(before["Y"]) + 30)])
        time.sleep(0.2)
        after = geometry()
        moved = resolve(binary, "after movement")
        for axis in ["x", "y"]:
            delta = int(after[axis.upper()]) - int(before[axis.upper()])
            assert abs(moved["anchor"]["caret"][axis] - initial["anchor"]["caret"][axis] - delta) <= 1, "caret did not follow terminal movement"
        terminal_focus = command("xdotool", "getwindowfocus")
        frame = json.loads(pathlib.Path(__file__).with_name("sample-frame-auto.json").read_text())
        stderr_log = tempfile.TemporaryFile(mode="w+")
        popup = subprocess.Popen([binary], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr_log, text=True)
        try:
            popup.stdin.write(json.dumps(frame) + "\n")
            popup.stdin.flush()
            selector = selectors.DefaultSelector()
            selector.register(popup.stdout, selectors.EVENT_READ)
            assert selector.select(5), "no popup decision"
            decision = json.loads(popup.stdout.readline())
            assert decision["display"] == "overlay", decision
            if not wait_until(popup_windows, 15):
                stderr_log.seek(0)
                raise AssertionError(f"popup did not become visible: {stderr_log.read()}")
            # Focus is checked once the popup is mapped, which is when a window manager would move it.
            assert command("xdotool", "getwindowfocus") == terminal_focus, "popup stole keyboard focus"
            # This checks actual window policy, independently of the JSON decision.
            info = command("xwininfo", "-id", popup_windows()[0])
            assert "Override Redirect State: yes" in info, info
            subprocess.check_call(["xdotool", "windowunmap", os.environ["WINDOWID"]])
            assert wait_until(lambda: not popup_windows(), 5), "popup remained visible after terminal unmapped"
        finally:
            if popup.poll() is None:
                popup.stdin.write('{"type":"quit"}\n')
                popup.stdin.flush()
                popup.wait(timeout=5)
            else:
                stderr_log.seek(0)
                raise RuntimeError(f"popup exited {popup.returncode}: {stderr_log.read()}")
        # A display hint cannot grant access to a native Wayland terminal without WINDOWID.
        env = dict(os.environ, XDG_SESSION_TYPE="wayland", WAYLAND_DISPLAY="wayland-test")
        env.pop("WINDOWID", None)
        result = subprocess.run([binary, "--resolve"], env=env, capture_output=True, text=True, timeout=5)
        assert json.loads(result.stdout)["display"] == "inline"
        env.pop("DISPLAY", None)
        fallback = subprocess.run([binary], input=json.dumps(frame) + '\n{"type":"quit"}\n', env=env, capture_output=True, text=True, timeout=5, check=True)
        assert json.loads(fallback.stdout)["display"] == "inline", fallback
        pathlib.Path(result_file).write_text("ok")
    except Exception as error:
        pathlib.Path(result_file).write_text(repr(error))
        raise


def desktop(binary):
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Vte", "2.91")
    from gi.repository import Gtk, Vte, GLib, GdkX11
    _ = GdkX11
    result_dir = tempfile.mkdtemp(prefix="goh-x11-result-")
    result = os.path.join(result_dir, "result")
    window = Gtk.Window(title="Goh X11 test terminal")
    terminal = Vte.Terminal()
    if os.environ.get("GOH_TEST_SPLIT"):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        box.pack_start(terminal, True, True, 0)
        box.pack_start(Vte.Terminal(), True, True, 0)
        window.add(box)
    else:
        window.add(terminal)
    window.set_default_size(900, 600)
    window.show_all()
    window.present()
    terminal.grab_focus()
    xid = window.get_window().get_xid()
    tries = [0]
    def focus_terminal():
        # BadMatch until the window manager has mapped the window: try again for a while.
        tries[0] += 1
        ok = subprocess.run(["xdotool", "windowfocus", "--sync", str(xid)], capture_output=True).returncode == 0
        if not ok and tries[0] >= 40:
            raise RuntimeError("could not focus the test terminal")
        return not ok
    GLib.timeout_add(250, focus_terminal)
    env = [f"{key}={value}" for key, value in dict(os.environ, WINDOWID=str(xid)).items()]
    terminal.spawn_async(Vte.PtyFlags.DEFAULT, os.getcwd(),
        [sys.executable, __file__, "--child", binary, result], env,
        GLib.SpawnFlags.DEFAULT, None, None, -1, None, None)
    started = time.monotonic()

    def check():
        if pathlib.Path(result).exists() or time.monotonic() - started > 60:
            Gtk.main_quit()
            return False
        return True
    GLib.timeout_add(100, check)
    Gtk.main()
    assert pathlib.Path(result).exists(), "terminal test timed out"
    message = pathlib.Path(result).read_text()
    pathlib.Path(result).unlink()
    os.rmdir(result_dir)
    assert message == "ok", message
    if os.environ.get("GOH_TEST_SPLIT"):
        print("X11: ambiguous split panes use inline: PASS")
        return
    print("X11: verified caret, movement verified, focus preserved, popup visible, unmap dismissal, Wayland fallback: PASS")


if __name__ == "__main__":
    if sys.argv[1] == "--child":
        child(sys.argv[2], sys.argv[3])
    else:
        desktop(os.path.abspath(sys.argv[1]))
