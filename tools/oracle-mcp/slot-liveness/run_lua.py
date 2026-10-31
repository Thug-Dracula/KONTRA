#!/usr/bin/env python3
"""Run one Lua probe inside a disposable real Kontakt 8 (the oracle-mcp launcher, standalone).

    run_lua.py PROBE.lua OUT.jsonl [--nki PATH] [--timeout S]

Same launch as oracle-mcp's oracle_dump_script (wine-staging 11.16, the DXVK prefix, a free
probe display, dump.py's ORACLE_INPUTS / ORACLE_OUT contract) but it takes the probe file
from the command line and returns whatever the probe wrote, so a measurement can bring its
own script instead of the two shipped probes.

Never touches the user's desktop: the display is one of the oracle's own Xvfb numbers.
"""
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "tools/oracle-mcp"))
import server  # noqa: E402


def main() -> int:
    probe = Path(sys.argv[1]).resolve()
    out = Path(sys.argv[2]).resolve()
    nki = None
    timeout = 300.0
    args = sys.argv[3:]
    while args:
        key = args.pop(0)
        if key == "--nki":
            nki = Path(args.pop(0)).resolve()
        elif key == "--timeout":
            timeout = float(args.pop(0))
        else:
            sys.exit(f"unknown argument {key}")
    out.parent.mkdir(parents=True, exist_ok=True)

    server.ensure_settings()
    display = server.free_display(server.PROBE_DISPLAYS)
    run = HERE / "runs" / time.strftime("lua-%Y%m%d-%H%M%S")
    run.mkdir(parents=True, exist_ok=True)
    script = run / "oracle.lua"
    script.write_text(
        "ORACLE_INPUTS = {" + ",".join(server.lua_str(server.winpath(p)) for p in ([nki] if nki else [])) + "}\n"
        + "ORACLE_OUT = " + server.lua_str(server.winpath(out)) + "\n"
        + probe.read_text())

    server.guard_display(display)
    server.clear_stale_lock(display)
    if server.display_up(display):
        sys.exit(f"display {display} already in use")
    xvfb = subprocess.Popen(["Xvfb", display, *server.XVFB_ARGS], start_new_session=True,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    t0 = time.monotonic()
    while time.monotonic() - t0 < 15 and not server.display_up(display):
        time.sleep(0.3)
    if not server.display_up(display):
        os.killpg(os.getpgid(xvfb.pid), signal.SIGKILL)
        sys.exit(f"Xvfb {display} did not come up")

    env = server.base_env(display)
    env.update({"WINEPREFIX": str(server.PREFIX), "VK_ICD_FILENAMES": str(server.ICD),
                "WINEDLLOVERRIDES": server.DLL_OVERRIDES, "WINEDEBUG": "-all"})
    log_path = run / "kontakt.log"
    started = time.monotonic()
    with open(log_path, "wb") as logf:
        kontakt = subprocess.Popen([str(server.WINE), str(server.EXE), server.winpath(script)],
                                   cwd=str(server.EXEDIR), env=env, stdout=logf,
                                   stderr=subprocess.STDOUT, start_new_session=True)
    done = False
    try:
        while time.monotonic() - started < timeout:
            if kontakt.poll() is not None and not out.exists():
                sys.exit(f"Kontakt exited rc={kontakt.returncode} without writing {out.name}; "
                         f"log: {server.read_log_tail(log_path)}")
            if out.exists() and out.read_text(errors="replace").rstrip().endswith('{"done":true}'):
                done = True
                break
            time.sleep(0.5)
    finally:
        for sig in (signal.SIGTERM, signal.SIGKILL):
            try:
                os.killpg(os.getpgid(kontakt.pid), sig)
            except OSError:
                break
            t0 = time.monotonic()
            while time.monotonic() - t0 < 6 and kontakt.poll() is None:
                time.sleep(0.2)
        try:
            os.killpg(os.getpgid(xvfb.pid), signal.SIGKILL)
        except OSError:
            pass
    print(json.dumps({"done": done, "out": str(out), "run_dir": str(run), "display": display,
                      "seconds": round(time.monotonic() - started, 1)}))
    return 0 if done else 1


if __name__ == "__main__":
    sys.exit(main())
