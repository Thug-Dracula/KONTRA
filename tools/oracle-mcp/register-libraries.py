#!/usr/bin/env python3
"""Give the oracle's Kontakt the library registrations it is missing.

    tools/oracle-mcp/register-libraries.py [--dry-run] [--keep-missing] [--no-hints]

Kontakt refuses every instrument that belongs to a library it has not been told about:

    "This instrument belongs to a library that is not installed currently!"

Two files carry that knowledge, both under the Kontakt install's `UserData`, and a fresh
portable build has neither for the user's libraries:

  `Service Center/LibraryHints.xml`  the list of installed libraries: Name, Company,
                                    SNPID and the `RegKey` Kontakt looks the library up
                                    by. This is the gate — measured 2026-10-31: the fresh
                                    build's 14,757-byte file refuses every library
                                    instrument; the user's 41,598-byte file loads them,
                                    with everything else held constant.

  `Settings.cfg`                    one section per library with `Name`, `SNPID`,
                                    `ContentDir` and the authorisation pair `JDX`/`HU`.
                                    Required as well: with the hints file in place and
                                    these sections absent, the same instrument is still
                                    refused.

The content database `komplete.db3` is *not* required: it indexes files for the browser,
but a 92 MB copy of the user's database made no difference to loading, so the oracle keeps
its own (1.4 MB) file. Copying the user's `Settings.cfg` wholesale is also wrong — it
carries window placement and the audio device, which move the window the oracle measures
against — so only the library sections are merged in.

The source (`ORACLE_MCP_LIBRARY_SOURCE`, default: the known portable installs, richest
first) is only ever read. The target is backed up before it is written
(`Settings.cfg.pre-libraries-<UTC>`, `LibraryHints.xml.pre-libraries-<UTC>`).

Run it once after the oracle's Kontakt is installed or updated, then `oracle_up`.
`ensure_settings()` in server.py edits `Settings.cfg` for the null sink and the MIDI port
and Kontakt rewrites it on exit; both keep unknown sections, so this is idempotent.
"""
from __future__ import annotations

import argparse
import os
import re
import sys
import time
import xml.etree.ElementTree as ET
from pathlib import Path

HOME = Path.home()

# Where the oracle's Kontakt lives, and where a fully registered UserData can come from.
# Same variable server.py reads (ORACLE_MCP_PORTABLE), so one env var moves both.
TARGET_DEFAULT = Path(os.environ.get("ORACLE_MCP_PORTABLE",
                                     HOME / "Armory/VSTs/Kontakt 8.14.1 Portable"))
SOURCES = [
    Path(os.environ["ORACLE_MCP_LIBRARY_SOURCE"]) if os.environ.get("ORACLE_MCP_LIBRARY_SOURCE") else None,
    HOME / "Armory/VSTs/Kontakt 8 Portable",
    HOME / "Armory/VSTs/Kontakt 7 Portable",
    TARGET_DEFAULT,
]
SECTION = re.compile(r"^\[(.+)\]\s*$")


def sections(text: str) -> list[tuple[str, list[str]]]:
    """The file's sections in order: (heading, body lines). Text before the first heading is dropped."""
    out: list[tuple[str, list[str]]] = []
    body: list[str] | None = None
    for line in text.splitlines():
        m = SECTION.match(line)
        if m:
            body = []
            out.append((m.group(1), body))
        elif body is not None:
            body.append(line)
    return out


def value(body: list[str], key: str) -> str | None:
    for line in body:
        if line.startswith(key + "="):
            return line.split("=", 1)[1].strip()
    return None


def is_library(body: list[str]) -> bool:
    """A library section: a product id, a name and a content directory."""
    return bool(value(body, "SNPID") and value(body, "Name")
                and (value(body, "ContentDir") or "").startswith("sz:"))


def posix(content_dir: str) -> Path | None:
    """`sz:X:\\VSTs\\…` as a path on this machine, using the prefix's own drive letters."""
    win = content_dir[3:]
    if not re.match(r"^[A-Za-z]:\\", win):
        return None
    drive = HOME / "Armory" if win[0].upper() == "X" else Path("/")
    return drive / win[2:].lstrip("\\").replace("\\", "/")


def hints_count(path: Path) -> int:
    """How many libraries a LibraryHints.xml declares (0 when it cannot be read)."""
    try:
        return len(ET.parse(path).getroot().findall("Library"))
    except (OSError, ET.ParseError):
        return 0


def backup(path: Path) -> Path:
    dst = path.with_name(f"{path.name}.pre-libraries-{time.strftime('%Y%m%dT%H%M%SZ', time.gmtime())}")
    dst.write_bytes(path.read_bytes())
    return dst


def merge_settings(source: Path, target: Path, keep_missing: bool, dry_run: bool) -> int:
    target_text = target.read_text(errors="surrogateescape")
    have = {name for name, _ in sections(target_text)}
    source_sections = sections(source.read_text(errors="surrogateescape"))

    added, existing, missing = [], [], []
    for name, body in source_sections:
        if not is_library(body):
            continue
        if name in have:
            existing.append(name)
            continue
        where = posix(value(body, "ContentDir") or "")
        if where is None or not where.is_dir():
            missing.append((name, str(where)))
            if not keep_missing:
                continue
        added.append(name)

    print(f"  settings: {len(added)} to add, {len(existing)} already present, "
          f"{len(missing)} skipped (content directory missing)")
    for name, where in missing:
        print(f"    skip {name}: {where} is not a directory")
    if dry_run or not added:
        return 0

    nl = "\r\n" if "\r\n" in target_text else "\n"
    tail = target_text if target_text.endswith(("\n", "\r")) else target_text + nl
    block = "".join(f"[{name}]" + nl + nl.join(body) + nl for name, body in source_sections
                    if name in added)
    print(f"    backup {backup(target).name}")
    target.write_bytes((tail + block).encode("utf-8", "surrogateescape"))
    return len(added)


def copy_hints(source: Path, target: Path, dry_run: bool) -> int:
    """Replace the target's library list when the source declares more libraries."""
    want, have = hints_count(source), hints_count(target)
    print(f"  hints   : source {want} libraries, target {have}")
    if want <= have:
        print("    nothing to do")
        return 0
    if dry_run:
        return want - have
    if target.exists():
        print(f"    backup {backup(target).name}")
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(source.read_bytes())
    return want - have


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--target", type=Path, default=TARGET_DEFAULT,
                    help=f"portable Kontakt to register into (default: $ORACLE_MCP_PORTABLE or {TARGET_DEFAULT})")
    ap.add_argument("--source", type=Path, default=None,
                    help="install to copy registrations from (default: the richest known install)")
    ap.add_argument("--dry-run", action="store_true", help="report what would change, write nothing")
    ap.add_argument("--keep-missing", action="store_true",
                    help="also copy Settings.cfg sections whose ContentDir does not exist")
    ap.add_argument("--no-hints", action="store_true", help="leave Service Center/LibraryHints.xml alone")
    args = ap.parse_args()

    target = args.target.expanduser()
    target_cfg = target / "UserData/Settings.cfg"
    if not target_cfg.exists():
        print(f"no such file: {target_cfg}", file=sys.stderr)
        return 2

    if args.source:
        candidates = [args.source.expanduser()]
    else:
        candidates = [p for p in SOURCES if p]
    best, best_n = None, -1
    for install in candidates:
        cfg = install / "UserData/Settings.cfg"
        if not cfg.exists() or cfg.resolve() == target_cfg.resolve():
            continue
        n = sum(1 for _, body in sections(cfg.read_text(errors="surrogateescape")) if is_library(body))
        if n > best_n:
            best, best_n = install, n
    if best is None:
        print("no source install with library registrations found; pass --source", file=sys.stderr)
        return 2

    print(f"source : {best}  ({best_n} libraries)")
    print(f"target : {target}")
    changed = merge_settings(best / "UserData/Settings.cfg", target_cfg, args.keep_missing, args.dry_run)
    if not args.no_hints:
        changed += copy_hints(best / "UserData/Service Center/LibraryHints.xml",
                              target / "UserData/Service Center/LibraryHints.xml", args.dry_run)
    print("dry run: nothing written" if args.dry_run else
          (f"done: {changed} registrations added. next: tools/oracle-mcp/smoke.sh"
           if changed else "nothing to do"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
