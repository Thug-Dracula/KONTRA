#!/usr/bin/env python3
"""Kontakt-side slot liveness: one fresh session per capture, first render only (round-robin 0).

    capture_variants.py --out DIR [--configs control,sup2,...] [--clean 6] [--max-attempts 18]

Why one session per capture: Kontakt's round robin is a per-session counter, so render 1 of a
session is round-robin index 0 and render n is index n-1 (measured by the Electric Mint stage
attribution record, 2026-10-30). Every capture here is the first render of its own session, so
the four configurations are compared at the same round-robin sample.

Every capture is scored for the capture path's intermittent HF burst (+46 dB in the 12.9 kHz band
over a 0.2 s window, ~0.8-1.2 s into the note) and rendered again when hit; every attempt is
recorded, kept or rejected.

The instrument files differ only in stored insert-effect bypass flags, written by Kontakt itself
(probe_variants.lua) and verified from the saved files with kontra-inspect.
"""
import argparse
import json
import shutil
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "tools/oracle-mcp"))
sys.path.insert(0, str(REPO / "tools/oracle"))

import numpy as np  # noqa: E402

import parity  # noqa: E402

compare = parity.compare
client_mod = parity.client_mod
server = parity.load_mod("oracle_mcp_server", REPO / "tools/oracle-mcp/server.py")
BURST_DB = 20.0
ONSET_FLOOR_S = 0.05

LIB = parity.library_path("Native Instruments Session Guitarist Electric Mint 1.1.0/"
                          "Session Guitarist - Electric Mint Library/Instruments")
DEFAULT_NKI = LIB / "Electric Mint.nki"
VARIANT_DIR = LIB / "__slotliveness"


def burst_db(path: Path) -> float:
    bands = list(compare.BANDS)
    hi = int(np.argmin([abs(b - 12902) for b in bands]))
    ref = int(np.argmin([abs(b - 1016) for b in bands]))
    x = compare.load(path)
    cells = []
    for i in range(0, x.shape[1] - 9600, 4800):
        level = compare.band_levels(x[:, i:i + 9600])
        cells.append(level[hi] - level[ref])
    cells = np.asarray(cells)
    return float(cells.max() - np.median(cells))


def fetch(client, name, arguments):
    resp = client.tool(name, arguments)
    res = resp.get("result") or {}
    body = client_mod.text_of(resp)
    if res.get("isError"):
        raise RuntimeError(f"{name}: {body}")
    return json.loads(body)


def one_capture(nki: Path, midi: Path, dest: Path, log) -> dict:
    client = client_mod.Client()
    try:
        client.handshake()
        fetch(client, "oracle_up", {})
        for attempt in (1, 2):
            try:
                loaded = fetch(client, "oracle_load", {"nki": str(nki)})
                break
            except RuntimeError as e:
                log(f"    load attempt {attempt} failed ({e}); new session")
                fetch(client, "oracle_down", {})
                client.close()
                client = client_mod.Client()
                client.handshake()
                fetch(client, "oracle_up", {})
        else:
            raise RuntimeError("oracle_load never worked")
        rendered = fetch(client, "oracle_render", {"midi": str(midi)})
        fetch(client, "oracle_down", {})
    finally:
        try:
            client.close()
        except Exception:  # noqa: BLE001
            pass
    src = Path(rendered["wav"])
    shutil.copyfile(src, dest)
    rendered["wav"] = str(dest)
    rendered["page"] = loaded.get("page") or loaded.get("verdict")
    return rendered


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--configs", default="orig,control,sup2,chor6,diff4,both46")
    ap.add_argument("--clean", type=int, default=6)
    ap.add_argument("--max-attempts", type=int, default=20)
    ap.add_argument("--variant-dir", default=str(VARIANT_DIR),
                    help="where the probe wrote the <config>.nki copies")
    args = ap.parse_args()

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    logf = open(out / "capture.log", "a")

    def log(msg):
        line = time.strftime("%H:%M:%S ") + msg
        print(line, file=sys.stderr, flush=True)
        logf.write(line + "\n")
        logf.flush()

    midi = out / "one-note.mid"
    server.write_midi(midi, [{"start_s": 0.0, "length_s": 1.5, "key": 60, "velocity": 100}])

    files = {"orig": DEFAULT_NKI}
    variant_dir = Path(args.variant_dir)
    for name in ("control", "sup2", "chor6", "diff4", "both46", "addr2", "addr4", "addr6",
                 "s0", "s1", "s2", "s12", "s3", "s5", "s7", "s137", "all",
                 "volwrite", "vol0", "volm6", "volp6"):
        files[name] = variant_dir / f"{name}.nki"

    ledger = {"burst_limit_db": BURST_DB, "configs": {}, "midi": str(midi)}
    for name in args.configs.split(","):
        nki = files[name]
        if not nki.exists():
            raise SystemExit(
                f"{nki} is missing: `orig` needs the Electric Mint library (set "
                f"KONTRA_TEST_LIBRARY to the sample-library root) and the other variants are "
                f"written by probe_variants.lua under --variant-dir {variant_dir}")
        cfg_dir = out / name
        cfg_dir.mkdir(exist_ok=True)
        log(f"== {name}: {nki.name}")
        rows, clean = [], 0
        for attempt in range(1, args.max_attempts + 1):
            if clean >= args.clean:
                break
            dest = cfg_dir / f"attempt-{attempt:02d}.wav"
            t0 = time.monotonic()
            try:
                rendered = one_capture(nki, midi, dest, log)
            except Exception as e:  # noqa: BLE001
                log(f"  {name} attempt {attempt}: SESSION FAILED: {e}")
                rows.append({"attempt": attempt, "error": str(e)})
                continue
            d = burst_db(dest)
            row = {"attempt": attempt, "burst_db": round(d, 1),
                   "peak_dbfs": rendered.get("peak_dbfs"), "rms_dbfs": rendered.get("rms_dbfs"),
                   "onset_s": rendered.get("onset_s"), "duration_s": rendered.get("duration_s"),
                   "sha256": rendered.get("sha256"), "seconds": round(time.monotonic() - t0, 1)}
            if d > BURST_DB:
                row["rejected"] = "HF burst"
                dest.unlink(missing_ok=True)
                log(f"  {name} attempt {attempt}: REJECTED burst {d:+.1f} dB")
            elif (rendered.get("onset_s") or 0) < ONSET_FLOOR_S:
                row["rejected"] = "attack lost"
                dest.unlink(missing_ok=True)
                log(f"  {name} attempt {attempt}: REJECTED onset {rendered.get('onset_s')} s")
            else:
                clean += 1
                kept = cfg_dir / f"clean-{clean}.wav"
                dest.rename(kept)
                row.update({"wav": str(kept), "render": clean,
                            "peak_dbfs": rendered["peak_dbfs"]})
                log(f"  {name} attempt {attempt}: clean {clean} "
                    f"(burst {d:+.1f} dB, peak {rendered['peak_dbfs']:+.2f} dBFS)")
            rows.append(row)
            ledger["configs"][name] = {"nki": str(nki), "clean": clean, "rows": rows}
            (out / "captures.json").write_text(json.dumps(ledger, indent=1))
        ledger["configs"][name] = {"nki": str(nki), "clean": clean, "rows": rows}
        (out / "captures.json").write_text(json.dumps(ledger, indent=1))
        log(f"== {name}: {clean} clean")
    (out / "captures.json").write_text(json.dumps(ledger, indent=1))
    total = {k: v["clean"] for k, v in ledger["configs"].items()}
    log(f"done: {total}")
    return 0 if all(v >= args.clean for v in total.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
