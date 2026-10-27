#!/usr/bin/env python3
"""Replay typing in a real zsh (adapter + real gohd, stand-in popup) and report how it behaves per keystroke.

There is no screen to look at, so this records what an observer would feel: for every character of a set of
realistic command lines, whether the menu is open, how many candidates it shows, how long the adapter spent,
and how many frames it sent the popup. It then flags friction: flicker (open, closed, open inside one word),
an open menu that only repeats what was typed, a menu that opens on a space with a long list, slow keystrokes.

  cargo build -p goh-daemon && python3 tools/typing-sim/typing_sim.py [--delay-ms 90] [--only SUBSTR] [--raw]
"""
import argparse, os, pty, select, shutil, signal, statistics, subprocess, sys, tempfile, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BIN = ROOT / "target" / "debug"
ADAPTER = ROOT / "adapters" / "zsh" / "goh.zsh"
FAKE_POPUP = ROOT / "crates" / "goh-daemon" / "tests" / "fixtures" / "fake-popup.sh"

CORPUS = [
    "git status",
    "git checkout -b feature/login",
    'git commit -m "fix bug"',
    "git push origin main",
    "git add src/main.rs",
    "git checkout main",
    "docker run --rm -it ubuntu bash",
    "npm run build",
    "npm install --save-dev vitest",
    "ls -la",
    "cd ..",
    "git chekout main",
    "gti status",
]


class Term:
    def __init__(self, env):
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.execvpe("zsh", ["zsh", "-f", "-i"], env)

    def drain(self, t):
        end, out = time.time() + t, b""
        while time.time() < end:
            if select.select([self.fd], [], [], max(0.0, min(0.02, end - time.time())))[0]:
                try:
                    d = os.read(self.fd, 4096)
                except OSError:
                    break
                if not d:
                    break
                out += d
        return out

    def send(self, s, wait=0.0):
        os.write(self.fd, s.encode())
        return self.drain(wait)


def parse_trace(path):
    ev = []
    for line in Path(path).read_text().splitlines() if Path(path).exists() else []:
        t, _, rest = line.partition(" ")
        kind, _, body = rest.partition(" ")
        d = {"t": float(t), "kind": kind}
        if kind == "key":
            b = body
            d["buf"] = b[b.index("[") + 1 : b.rindex("]")]
            for k in ("w", "open", "cands", "top", "ms"):
                import re
                m = re.search(rf"\b{k}=(\S*)", b[b.rindex("]") + 1 :])
                d[k] = m.group(1) if m else ""
        elif kind == "skip":
            d["buf"] = body[body.index("[") + 1 : body.rindex("]")]
        ev.append(d)
    return ev


def glyph(open_, cands):
    if not open_:
        return "·"
    return str(cands) if cands < 10 else "+"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--delay-ms", type=float, default=90, help="time between keystrokes (90 ≈ steady typing, 25 ≈ fast)")
    ap.add_argument("--only", default="")
    ap.add_argument("--raw", action="store_true", help="also print every trace event")
    ap.add_argument("--save-frames", metavar="FILE", help="keep the frames sent to the popup (one JSON per line)")
    args = ap.parse_args()
    for b in ("gohd", "goh"):
        if not (BIN / b).exists():
            sys.exit(f"missing {BIN / b}: run `cargo build -p goh-daemon` first")

    tmp = Path(tempfile.mkdtemp(prefix="ts", dir="/tmp"))
    (tmp / "specs").mkdir()   # empty: only the specs bundled in the binary are served
    sock, log, trace = tmp / "run" / "s", tmp / "popup.log", tmp / "trace.txt"
    env = {k: v for k, v in os.environ.items() if k not in ("TMUX", "STY", "SSH_CONNECTION", "TERM_PROGRAM")}
    env.update(LC_ALL=os.environ.get("LC_ALL", "en_US.UTF-8"), TERM="xterm-256color", COLUMNS="100", HOME=str(tmp),
               GOH_SOCKET=str(sock), GOH_SPEC_DIR=str(tmp / "specs"), GOH_CONFIG=str(tmp / "none.json"), GOH_CTL=str(BIN / "goh"),
               GOH_LIVE="on", GOH_DISPLAY="auto", GOH_POPUP=str(FAKE_POPUP), FAKE_LOG=str(log), FAKE_DISPLAY="overlay",
               GOH_TRACE=str(trace), PATH=f"{BIN}:{os.environ['PATH']}")
    daemon = subprocess.Popen([str(BIN / "gohd")], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        t0 = time.time()
        while not sock.exists() and time.time() - t0 < 5:
            time.sleep(0.05)
        term = Term(env)
        term.drain(1.0)
        term.send("PS1='READY> '\n", 0.3)
        term.send(f"source {ADAPTER}\n", 1.0)
        term.send("git c", 0.5); term.send("\x15", 0.3)            # warm up the daemon and the popup
        results = []
        for line in CORPUS:
            if args.only and args.only not in line:
                continue
            start = time.time()
            for ch in line:
                term.send(ch, args.delay_ms / 1000)
            term.drain(0.4)
            end = time.time()
            term.send("\x15", 0.25)
            results.append((line, start, end))
        term.send("exit\n", 0.4)
        events = parse_trace(trace)
        if args.save_frames and log.exists():
            shutil.copy(log, args.save_frames)
        frames = [e for e in events if e["kind"] == "frame"]
        hides = [e for e in events if e["kind"] == "hide"]
        all_ms, total_keys, total_frames, flags = [], 0, 0, []
        print(f"typing delay {args.delay_ms:.0f} ms/key.  Legend: · menu closed, digit = menu open with that many candidates (+ = 10 or more)\n")
        for line, start, end in results:
            keys = [e for e in events if e["kind"] == "key" and start - 0.01 <= e["t"] <= end + 0.05]
            by_buf = {}
            for e in keys:
                by_buf[e["buf"]] = e
            strip, rows = "", []
            for i in range(1, len(line) + 1):
                e = by_buf.get(line[:i])
                if e is None:
                    strip += "?"
                    continue
                o, c = e["open"] == "1", int(e["cands"] or 0)
                strip += glyph(o, c)
                rows.append((line[:i], o, c, e["top"], float(e["ms"] or 0)))
            nfr = len([f for f in frames if start - 0.01 <= f["t"] <= end + 0.05])
            nhide = len([h for h in hides if start - 0.01 <= h["t"] <= end + 0.5])
            opens = sum(1 for a, b in zip(rows, rows[1:]) if not a[1] and b[1]) + (1 if rows and rows[0][1] else 0)
            # friction heuristics
            word = lambda p: p.rsplit(" ", 1)[-1]
            exact = [p for (p, o, c, top, _), prev in zip(rows[1:], rows) if o and not prev[1] and c == 1 and top == word(p)]   # newly opened on a finished word
            flicker = 0
            for i in range(1, len(rows) - 1):
                if rows[i - 1][1] and not rows[i][1] and rows[i + 1][1] and " " not in rows[i][0][-1:]:
                    flicker += 1
            longopen = [p for (p, o, c, _, _), prev in zip(rows[1:], rows) if o and not prev[1] and prev[0][-1:] != " " and False]
            space_long = [(p, c) for (p, o, c, _, _), prev in zip(rows[1:], rows) if o and c >= 6 and p.endswith(" ")]
            slow = [(p, ms) for p, _, _, _, ms in rows if ms > 40]   # anything under ~40 ms is below what typing can feel
            ms = [r[4] for r in rows]
            all_ms += ms; total_keys += len(rows); total_frames += nfr
            print(line)
            print(strip)
            print(f"   keys {len(rows):2}  opens {opens}  frames sent {nfr:2}  hides {nhide}  max {max(ms or [0]):.0f} ms")
            if flicker: flags.append(f"FLICKER x{flicker} in `{line}`")
            for p in exact[:1]: flags.append(f"OPENED ON A FINISHED WORD: `{p}` (1 candidate = what is already typed) in `{line}`")
            for p, c in space_long[:1]: flags.append(f"LONG LIST ON A SPACE: `{p}` -> {c} candidates, no keystroke asked for it")
            for p, m in slow[:1]: flags.append(f"SLOW KEY: `{p}` took {m:.0f} ms")
            if args.raw:
                for e in keys: print("   ", e)
            print()
        if all_ms:
            s = sorted(all_ms)
            print(f"adapter time per key: median {statistics.median(s):.1f} ms, p95 {s[int(.95 * (len(s) - 1))]:.1f} ms, max {s[-1]:.1f} ms over {len(s)} keys")
            print(f"frames sent to the popup: {total_frames} for {total_keys} keys\n")
        print("Friction found:" if flags else "Friction found: none")
        for f in flags:
            print("  -", f)
    finally:
        try:
            os.kill(term.pid, signal.SIGKILL)
        except Exception:
            pass
        daemon.terminate()
        try:
            daemon.wait(3)
        except Exception:
            daemon.kill()
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    main()
