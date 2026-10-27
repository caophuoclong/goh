# goh.xsh — spec-driven completion for xonsh through its completer API.
# Add to ~/.xonshrc:  source /path/to/goh.xsh
# Fail-open: errors yield no candidates and the other completers run.
# Not verified against a real xonsh session.
import subprocess
import os
from xonsh.completers.tools import contextual_command_completer, RichCompletion

_GOH_SESSION = f"xonsh-{os.getpid()}"
_GOH_NAMES = set()


def _goh_ctl(*args):
    try:
        return subprocess.run(["goh", *args], capture_output=True, text=True, timeout=1).stdout
    except Exception:
        return ""


for _line in _goh_ctl("specs").splitlines():
    _GOH_NAMES.add(_line.split("\t")[0])


@contextual_command_completer
def _goh_completer(ctx):
    args = ctx.args
    if not args or str(args[0]) not in _GOH_NAMES:
        return None
    words = [str(a) for a in args[: ctx.arg_index]] + [ctx.prefix]
    buffer = " ".join(words)
    point = len(buffer.encode())
    cur_bytes = len(ctx.prefix.encode())
    out = _goh_ctl("menu", "--session", _GOH_SESSION, "--cwd", os.getcwd(), "--limit", "50",
                   "--cursor-byte", str(point), "--buffer", buffer)
    found = set()
    for line in out.splitlines():
        f = line.split("\x1f")
        if len(f) < 7 or int(f[0]) != point - cur_bytes or int(f[1]) != point:
            continue
        found.add(RichCompletion(f[5].rstrip(" ").replace("\\ ", " "), display=f[4], description=f[6]))
    return found or None


__xonsh__.completers["goh"] = _goh_completer
__xonsh__.completers.move_to_end("goh", last=False)


@events.on_chdir
def _goh_visit(olddir, newdir, **kw):
    # Directory history for `cd` suggestions when zoxide is not installed.
    try:
        subprocess.Popen(["goh", "visit", newdir], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    except Exception:
        pass
