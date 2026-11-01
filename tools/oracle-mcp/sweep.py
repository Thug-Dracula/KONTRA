#!/usr/bin/env python3
"""Sweep a sample of the installed library: Kontakt against KONTRA, one note each.

    tools/oracle-mcp/sweep.py [--plan tools/oracle-mcp/sweep-plan.json] [--out DIR]
                              [--limit N] [--only SUBSTRING] [--skip-kontakt]

One `oracle_up` … `oracle_down` for the whole batch (the oracle is serialised: one
session, one GUI). Per instrument: `oracle_load`, two renders of the same note (to see
the instrument's own round-robin spread), one `kontra-render`, then `parity.py`'s
comparison and verdict. Writes `SWEEP.md` (ranked worst first) and `sweep.json`, and
logs every step to `<out>/sweep.log` as it happens.

An instrument that will not load on either side is recorded with the reason, never
skipped silently. `--skip-kontakt` renders only KONTRA (no oracle, no audio verdict):
it answers "does KONTRA load this at all", not "does it match".
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import shutil
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import numpy as np  # noqa: E402

_spec = importlib.util.spec_from_file_location("parity", HERE / "parity.py")
parity = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(parity)

LOG = None


def log(message: str) -> None:
    line = f"{time.strftime('%H:%M:%S')} {message}"
    print(line, file=sys.stderr, flush=True)
    if LOG is not None:
        LOG.write(line + "\n")
        LOG.flush()


def load_plan(path: Path) -> list[dict]:
    """The plan's instruments as this machine spells them.

    A plan records `$HOME/Armory/VSTs/Kontakt1/...`; `parity.resolve` expands `$HOME` and
    moves anything under the default root onto `KONTRA_TEST_LIBRARY` when it is set.
    """
    plan = json.loads(path.read_text())
    for entry in plan:
        entry["nki"] = str(parity.resolve(entry["nki"]))
    return plan


def diag_features(path: Path) -> list[str]:
    """The Audible diagnostics `kontra-render` reported, one per line, deduplicated."""
    if not path.exists():
        return []
    seen = []
    for line in path.read_text(errors="replace").splitlines():
        stripped = line.strip()
        if stripped.startswith("Audible ") and stripped not in seen:
            seen.append(stripped)
    return seen


def guess(row: dict, features: list[str], kontra_silent: bool) -> str:
    """A one-line cause guess, marked as a guess, from the shape of the numbers."""
    if kontra_silent:
        return "guess: no sample mapped for this key (KONTRA rendered silence)"
    if features:
        first = features[0]
        detail = first[len("Audible "):].strip()
        return f"guess: {detail} (KONTRA's own diagnostic)"
    if row.get("silent"):
        return f"guess: page/stream failure on the {row['silent']} side"
    level, bands, worst = row["level_db"], row["band_rms_db"], row["worst_band_db"]
    if abs(level) > 1.0 and bands < 1.0:
        return "guess: gain staging (flat difference, wrong level)"
    if worst > 0 and row["worst_band_hz"] > 6000:
        return "guess: high-frequency stage unmodelled (brightness/FX/air)"
    if worst < 0 and row["worst_band_hz"] > 6000:
        return "guess: KONTRA brighter than Kontakt (unmodelled damping/filter)"
    if row.get("wave_r", 1.0) < 0.5:
        return "guess: different sample or articulation (low waveform correlation)"
    if bands > 1.5:
        return "guess: timbre shape differs across the band (modulation/envelope?)"
    return "guess: none obvious; difference is near the noise floor"


def sweep_one(client, entry: dict, out: Path, window: float, kontakt: bool,
              renders: int) -> dict:
    """One instrument: Kontakt renders (if a session is up), KONTRA render, comparison."""
    name = entry["name"]
    nki = Path(entry["nki"])
    row = {"name": name, "library": entry.get("library", ""), "family": entry.get("family", ""),
           "nki": str(nki), "expected": entry.get("expect", ""), "notes": []}
    if not nki.exists():
        row["failed"] = "instrument file missing"
        return row
    keys = entry.get("keys") or [60]
    midis = {}
    for key in keys:
        midi = out / "midi" / f"{entry['slug']}-{key}.mid"
        midi.parent.mkdir(parents=True, exist_ok=True)
        parity.load_mod("oracle_mcp_server", HERE / "server.py").write_midi(
            midi, [{"start_s": 0.0, "length_s": entry.get("hold", 1.5), "key": key,
                    "velocity": entry.get("velocity", 100)}])
        midis[key] = midi

    kontakt_wavs: dict[int, Path] = {}
    key = keys[0]
    if kontakt:
        loaded = False
        try:
            parity.load_instrument(client, nki)
            loaded = True
        except Exception as e:  # noqa: BLE001 - recorded, not hidden
            row["load_error"] = str(e).splitlines()[0][:300]
            log(f"{name}: load failed: {row['load_error']}")
        if not loaded:
            # `oracle_load` judges the page by Kontakt's keyboard strip; an instrument
            # with a full-window custom UI fails that verdict while loading fine. A
            # render that has sound settles it, so try before calling it a failure.
            row["notes"].append("page verdict failed; checked by rendering")
            log(f"{name}: page verdict failed, trying a render anyway")
        for candidate in keys:
            try:
                first = parity.render_kontakt(client, midis[candidate], attempts=2)
            except Exception as e:  # noqa: BLE001 - silence on this key
                row["notes"].append(f"key {candidate}: Kontakt silent ({str(e).splitlines()[0][:120]})")
                log(f"{name}: key {candidate} silent in Kontakt")
                continue
            key = candidate
            dest = out / "kontakt" / f"{entry['slug']}-0.wav"
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(first["wav"], dest)
            kontakt_wavs[0] = dest
            for n in range(1, renders):
                more = parity.render_kontakt(client, midis[candidate], attempts=2)
                extra = out / "kontakt" / f"{entry['slug']}-{n}.wav"
                shutil.copyfile(more["wav"], extra)
                kontakt_wavs[n] = extra
            break
        if not kontakt_wavs:
            row["failed"] = "Kontakt rendered silence on every key tried"
            return row
        row["note"] = {"key": key, "velocity": entry.get("velocity", 100),
                       "hold_s": entry.get("hold", 1.5)}
    else:
        row["note"] = {"key": key, "velocity": entry.get("velocity", 100),
                       "hold_s": entry.get("hold", 1.5),
                       "source": "plan (no Kontakt render)"}

    kontra_wav = out / "kontra" / f"{entry['slug']}.wav"
    kontra_wav.parent.mkdir(parents=True, exist_ok=True)
    diag = out / "kontra" / f"{entry['slug']}.diag"
    try:
        binary = parity.render_bin()
        if not binary.exists():
            raise FileNotFoundError(f"missing {binary}")
        env = {"PATH": "/usr/bin:/bin", "HOME": str(Path.home())}
        import subprocess
        p = subprocess.run([str(binary), "--instrument", str(nki), "--midi",
                            str(midis[key]), "--out", str(kontra_wav), "--diag", str(diag)],
                           env=env, capture_output=True, text=True, timeout=900)
        if p.returncode != 0 or not kontra_wav.exists():
            raise RuntimeError(f"kontra-render rc={p.returncode}: {p.stderr[-500:]}")
        row["kontra"] = parity.parse_summary(p.stdout)
    except Exception as e:  # noqa: BLE001 - recorded, not hidden
        row["kontra_error"] = str(e)[:400]
        log(f"{name}: KONTRA failed: {row['kontra_error']}")
        if not kontakt:
            row["failed"] = "KONTRA could not render it"
            return row
        row["failed"] = "KONTRA could not render what Kontakt rendered"
        row["features"] = diag_features(diag)
        return row

    row["features"] = diag_features(diag)
    if not kontakt:
        return row

    cap = parity.load_as_capture(kontakt_wavs[0], out / "kontakt" / f"{entry['slug']}-cap.wav")
    compared = parity.compare_renders(cap, kontra_wav, window, pre_a=parity.compare.PRE)
    row.update({k: compared.get(k) for k in (
        "level_db", "peak_db", "band_rms_db", "worst_band_db", "worst_band_hz",
        "lag_ms", "wave_r", "wave_lag_ms", "silent", "band_diff_db")})
    row["a_rms_db"] = compared.get("a_rms_db")
    row["b_rms_db"] = compared.get("b_rms_db")
    if len(kontakt_wavs) > 1:
        cap2 = parity.load_as_capture(kontakt_wavs[1], out / "kontakt" / f"{entry['slug']}-cap2.wav")
        other = parity.compare_renders(cap2, kontra_wav, window, pre_a=parity.compare.PRE)
        row["second"] = {"band_rms_db": other.get("band_rms_db"),
                         "worst_band_db": other.get("worst_band_db"),
                         "level_db": other.get("level_db"), "wave_r": other.get("wave_r")}
        spread = parity.compare_renders(cap, cap2, window, pre_a=parity.compare.PRE,
                                        pre_b=parity.compare.PRE)
        row["kontakt_self"] = {"band_rms_db": spread.get("band_rms_db"),
                               "worst_band_db": spread.get("worst_band_db"),
                               "level_db": spread.get("level_db")}
    if "silent" in compared:
        row["failed"] = f"one side silent ({compared['silent']})"
        return row
    row["verdict"] = parity.verdict(compared, parity.LEVEL_LIMIT_DB, parity.BAND_LIMIT_DB,
                                    parity.BAND_RMS_LIMIT_DB, parity.LAG_LIMIT_MS)
    row["guess"] = guess(row, row["features"], False)
    return row


def rank(rows: list[dict]) -> list[dict]:
    """Worst first: silent/load failures above everything, then band RMS + level."""
    def key(r):
        if "failed" in r:
            return (-1, 0.0)
        return (0, -(r.get("band_rms_db") or 0.0) - abs(r.get("level_db") or 0.0))
    return sorted(rows, key=key)


def markdown(rows: list[dict], window: float) -> str:
    lines = [
        "# Kontakt vs KONTRA: installed-library sweep",
        "",
        f"One note per instrument, {window:g} s window, velocity 100 held 1.5 s unless the "
        "note column says otherwise. Ranked worst first by band RMS + |level|.",
        "",
        "| # | instrument | family | key | level Δ dB | band RMS dB | worst band | Kontakt RR spread | wave r | verdict | cause (guess) |",
        "|---|---|---|---:|---:|---:|---|---:|---:|---|---|",
    ]
    for i, r in enumerate(rows, 1):
        note = r.get("note") or {}
        key = note.get("key", "-")
        if "failed" in r:
            lines.append(f"| {i} | {r['name']} | {r.get('family','')} | {key} | | | | | | "
                         f"**FAILED**: {r['failed']} | {r.get('guess','')} |")
            continue
        worst = f"{r['worst_band_db']:+.1f} dB @ {r['worst_band_hz']:.0f} Hz"
        self_noise = r.get("kontakt_self") or {}
        spread = (f"{self_noise['band_rms_db']:.2f} dB" if self_noise.get("band_rms_db")
                  is not None else "-")
        verdict = "pass" if (r.get("verdict") or {}).get("pass") else "FAIL"
        lines.append(
            f"| {i} | {r['name']} | {r.get('family','')} | {key} | {r['level_db']:+.2f} | "
            f"{r['band_rms_db']:.2f} | {worst} | {spread} | {r['wave_r']:.2f} | {verdict} | "
            f"{r.get('guess','')} |")
    notes = [r for r in rows if r.get("notes") or r.get("features") or r.get("kontra_error")
             or r.get("load_error")]
    if notes:
        lines += ["", "## Recorded per instrument", ""]
        for r in notes:
            for line in r.get("notes", []):
                lines.append(f"- **{r['name']}**: {line}")
            if r.get("load_error"):
                lines.append(f"- **{r['name']}**: Kontakt load error: `{r['load_error'][:200]}`")
            if r.get("kontra_error"):
                lines.append(f"- **{r['name']}**: KONTRA error: `{r['kontra_error'][:200]}`")
            for feature in (r.get("features") or [])[:6]:
                lines.append(f"- **{r['name']}**: KONTRA diagnostic `{feature}`")
    return "\n".join(lines) + "\n"


def main() -> int:
    global LOG
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--plan", default=str(HERE / "sweep-plan.json"))
    ap.add_argument("--out", default="/tmp/opencode/parity/sweep")
    ap.add_argument("--window", type=float, default=2.0)
    ap.add_argument("--renders", type=int, default=2,
                    help="Kontakt renders per instrument (2 shows the round-robin spread)")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--only", default=None, help="only instruments whose name matches")
    ap.add_argument("--skip-kontakt", action="store_true")
    ap.add_argument("--sessions", choices=("batch", "each"), default="each",
                    help="one oracle session for the whole batch, or a fresh one per "
                         "instrument (measured: after several loads in one session, later "
                         "instruments render silence)")
    ap.add_argument("--json")
    args = ap.parse_args()

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    LOG = open(out / "sweep.log", "a", encoding="utf-8")
    plan = load_plan(Path(args.plan))
    if args.only:
        plan = [p for p in plan if args.only.lower() in p["name"].lower()]
    if args.limit:
        plan = plan[: args.limit]
    log(f"sweep: {len(plan)} instruments -> {out} (log {out / 'sweep.log'})")

    rows = []
    client = None
    if not args.skip_kontakt and args.sessions == "batch":
        client = parity.client_mod.Client()
        client.handshake()
        up = parity.fetch_tool(client, "oracle_up", {})
        log(f"oracle up on {up['display']} in {up['launch_s']}s")
    try:
        for i, entry in enumerate(plan, 1):
            log(f"[{i}/{len(plan)}] {entry['name']}")
            if client is None and not args.skip_kontakt:
                client = parity.client_mod.Client()
                client.handshake()
                up = parity.fetch_tool(client, "oracle_up", {})
                log(f"oracle up on {up['display']} in {up['launch_s']}s")
            try:
                row = sweep_one(client, entry, out, args.window, not args.skip_kontakt,
                                args.renders)
            except Exception as e:  # noqa: BLE001 - one instrument never kills the batch
                row = {"name": entry["name"], "family": entry.get("family", ""),
                       "failed": f"harness error: {e}"}
                log(f"[{i}/{len(plan)}] harness error: {e}")
            rows.append(row)
            status = row.get("failed") or (
                f"level {row.get('level_db', float('nan')):+.2f} dB, "
                f"band {row.get('band_rms_db', float('nan')):.2f} dB")
            log(f"[{i}/{len(plan)}] {entry['name']}: {status}")
            if client is not None and args.sessions == "each":
                try:
                    parity.fetch_tool(client, "oracle_down", {})
                except Exception as e:  # noqa: BLE001
                    log(f"oracle_down failed: {e}")
                client.close()
                client = None
    finally:
        if client is not None:
            try:
                parity.fetch_tool(client, "oracle_down", {})
                log("oracle down")
            except Exception as e:  # noqa: BLE001
                log(f"oracle_down failed: {e}")
            client.close()

    ranked = rank(rows)
    text = markdown(ranked, args.window)
    (out / "SWEEP.md").write_text(text)
    (out / "sweep.json").write_text(json.dumps({"rows": ranked}, indent=1, default=float) + "\n")
    if args.json:
        Path(args.json).write_text(json.dumps({"rows": ranked}, indent=1, default=float) + "\n")
    print(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
