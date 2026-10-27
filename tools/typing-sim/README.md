# typing-sim

Replays typing in a real zsh with the adapter and a real `gohd`, and reports what the as-you-type menu did on every
keystroke, so its behaviour can be judged without watching a screen.

```bash
cargo build -p goh-daemon
python3 tools/typing-sim/typing_sim.py [--delay-ms 90] [--only SUBSTR] [--raw] [--save-frames FILE]
```

For each command line of a small corpus (git, docker, npm, unknown commands, typos) it prints the command and a strip,
one symbol per character: `·` menu closed, a digit = menu open with that many candidates (`+` = ten or more). Below it:
keys, how many times the menu opened, frames sent to the popup, hides, and the slowest key. At the end: adapter time per
key (median, p95, max) and a list of friction it found: **FLICKER** (open, closed, open inside one word),
**OPENED ON A FINISHED WORD**, **LONG LIST ON A SPACE** (the menu opens on a bare space with six or more candidates),
**SLOW KEY** (over 40 ms). `--delay-ms 25` simulates fast typing (queued keys are refreshed once). `--save-frames` keeps
the frames sent to the popup; feed one to `goh-popup --screenshot out.png` (with a manual anchor) to see exactly what it
would draw.

It serves only the specs bundled in the binary (its spec directory is empty). It uses a stand-in popup (`crates/goh-daemon/tests/fixtures/fake-popup.sh`) and the adapter's `GOH_TRACE` log, so it sees
the adapter's decisions and costs, **not** the real window: nothing about on-screen timing, blinking of a real window or
the Accessibility lookups. The corpus and the friction heuristics are mine; they find the problems I thought to look for.
