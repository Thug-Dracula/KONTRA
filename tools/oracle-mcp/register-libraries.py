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

The content database `komplete.db3` gates nothing: a 92 MB copy of the user's database made
no difference to loading. It is still installed (a) because it indexes the libraries' files
for the browser - 45,195 presets instead of an empty page - and (b) so the oracle runs the
same content database the user's install has. Kontakt 8.14 migrates it in place on first
launch without changing `db3_master.version` or the file size (see `copy_database`), which
makes the copy one-way: never copy it back into the source install. `--no-database` skips
this step.

Copying the user's `Settings.cfg` wholesale is wrong even so - it carries window placement
and the audio device, which move the window the oracle measures against - so only the
library sections are merged in.

The source (`ORACLE_MCP_LIBRARY_SOURCE`, default: the known portable installs, richest
first) is only ever read. The target is backed up before it is written
(`Settings.cfg.pre-libraries-<UTC>`, `LibraryHints.xml.pre-libraries-<UTC>`,
`komplete.db3.pre-populated-<UTC>`).

Run it once after the oracle's Kontakt is installed or updated, then `oracle_up`. The oracle
stays on 8.14.1 (the version upstream measures against); nothing here launches the 8.8 build.
`ensure_settings()` in server.py edits `Settings.cfg` for the null sink and the MIDI port
and Kontakt rewrites it on exit; both keep unknown sections, so this is idempotent.
"""
from __future__ import annotations

import argparse
import copy
import os
import re
import shutil
import sqlite3
import sys
import tempfile
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


def fields(body: list[str]) -> dict[str, str]:
    """A section's stored fields, by key. Order and blank lines are not part of it."""
    out: dict[str, str] = {}
    for line in body:
        key, sep, val = line.partition("=")
        if sep:
            out[key.strip()] = val.strip()
    return out


# The fields that say *which library* this section registers and whether it is authorized.
# ContentDir is not one of them: see `merge_settings`.
IDENTITY = ("Name", "SNPID", "Company", "JDX", "HU", "ContentVersion")


def identity(fields_: dict[str, str]) -> dict[str, str]:
    """The subset of a section's fields that identifies and authorizes the library."""
    return {k: v for k, v in fields_.items() if k in IDENTITY}


def rewrite_sections(text: str, updates: dict[str, list[str]], nl: str) -> str:
    """Replace the body of the named sections, leaving every other byte of the file alone."""
    lines = text.splitlines(keepends=True)
    out: list[str] = []
    i = 0
    while i < len(lines):
        line = lines[i]
        m = SECTION.match(line.rstrip("\r\n"))
        out.append(line)
        i += 1
        if m is None:
            continue
        body = updates.get(m.group(1))
        if body is None:
            continue
        # Skip the section's old body; the caller's lines replace it.
        while i < len(lines) and SECTION.match(lines[i].rstrip("\r\n")) is None:
            i += 1
        out.extend(b + nl for b in body)
    return "".join(out)


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


def hints_keys(path: Path) -> dict[str, ET.Element]:
    """The libraries a LibraryHints.xml declares, keyed by `RegKey` (0 when unreadable).

    Keyed rather than counted: equal counts do not prove equal identities, and this file
    is the loading gate, so "the source has as many libraries as the target" is not the
    question -- "is this library in there" is.
    """
    try:
        root = ET.parse(path).getroot()
    except (OSError, ET.ParseError):
        return {}
    out: dict[str, ET.Element] = {}
    for lib in root.findall("Library"):
        key = (lib.findtext("RegKey") or lib.findtext("Name") or "").strip()
        if key:
            out[key] = lib
    return out


def backup(path: Path) -> Path:
    dst = path.with_name(f"{path.name}.pre-libraries-{time.strftime('%Y%m%dT%H%M%SZ', time.gmtime())}")
    dst.write_bytes(path.read_bytes())
    return dst


def copy_database(source: Path, target: Path, dry_run: bool, refresh: bool = False) -> int:
    """Install the source's content database, folding its WAL in first.

    The database indexes the libraries' files for the browser (45,195 presets here). It is
    *not* required for loading - see the module docstring - but with it the browser shows the
    libraries instead of an empty list, which is what an agent comparing browser behaviour
    needs.

    Kontakt 8.14 migrates it on first launch, in place, without changing `db3_master.version`
    or the file size: it drops the two `p_sound_info_Leap_*` tables (8.8 wrote them; one held
    10 rows) with their indexes and triggers and widens the Instrument triggers to accept
    `.nkl`. That makes the copied file one-way: never copy it back into the 8 Portable
    install, or the user's build inherits an 8.14-shaped schema minus those rows.

    The source is only ever read: the copy is made first, then its WAL is checkpointed.
    """
    src = source / "UserData/Kontakt 8/komplete.db3"
    dst = target / "UserData/Kontakt 8/komplete.db3"
    if not src.exists():
        print("  database: source has none, left alone")
        return 0
    # The size test is deliberately not an identity test. 8.14 migrates this file in
    # place without changing its version stamp *or* its size, so a version/size
    # comparison cannot tell "already installed" from "installed and migrated";
    # re-copying on that guess is how a migrated file would silently be replaced.
    # `--refresh-database` is the deliberate way to ask for that replacement.
    if dst.exists() and dst.stat().st_size >= src.stat().st_size // 2 and not refresh:
        print(f"  database: target already populated ({dst.stat().st_size // 2**20} MB), left alone"
              " (--refresh-database replaces it)")
        return 0
    print(f"  database: {src.stat().st_size // 2**20} MB from {src.parent.parent.parent.name}")
    if dry_run:
        return 1
    scratch = Path(tempfile.mkdtemp(prefix="oracle-db-"))
    try:
        work = scratch / "komplete.db3"
        shutil.copyfile(src, work)
        if (src.parent / "komplete.db3-wal").exists():
            shutil.copyfile(src.parent / "komplete.db3-wal", scratch / "komplete.db3-wal")
        con = sqlite3.connect(work)
        con.execute("PRAGMA journal_mode=WAL")
        con.execute("PRAGMA wal_checkpoint(TRUNCATE)")
        con.close()
        for stale in (dst, dst.with_name("komplete.db3-wal"), dst.with_name("komplete.db3-shm")):
            if stale.exists():
                moved = stale.with_name(f"{stale.name}.pre-populated-"
                                        f"{time.strftime('%Y%m%dT%H%M%SZ', time.gmtime())}")
                print(f"    backup {moved.name}")
                stale.replace(moved)
        shutil.copyfile(work, dst)
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    return 1


def merge_settings(source: Path, target: Path, keep_missing: bool, dry_run: bool) -> int:
    target_text = target.read_text(errors="surrogateescape")
    have = {name for name, _ in sections(target_text)}
    source_sections = sections(source.read_text(errors="surrogateescape"))

    target_sections = dict(sections(target_text))
    added, existing, updated, missing = [], [], [], []
    for name, body in source_sections:
        if not is_library(body):
            continue
        if name in have:
            # A section that exists is not necessarily a section that is right: if the
            # library's identity or authorization fields have moved since it was
            # registered, skipping it leaves Kontakt refusing a library the command just
            # claimed to register. Compare identity, not text -- Kontakt reorders and
            # re-spaces this file, and a reorder is not a change.
            #
            # ContentDir is deliberately not part of the comparison. A portable install
            # writes it relative to its own root (`sz:\Libraries\<name>`) while a normal
            # install writes an absolute drive path for the same library, so the two
            # spellings differ for a registration that works. Overwriting the target's
            # spelling with the source's would be a change made for the sake of looking
            # uniform, on a library that currently loads.
            mine = fields(target_sections.get(name, []))
            theirs = fields(body)
            if identity(mine) != identity(theirs):
                updated.append((name, body))
            else:
                if mine.get("ContentDir") != theirs.get("ContentDir"):
                    print(f"    keep {name}: its ContentDir stays {mine.get('ContentDir')!r}"
                          f" (source spells it {theirs.get('ContentDir')!r})")
                existing.append(name)
            continue
        where = posix(value(body, "ContentDir") or "")
        if where is None or not where.is_dir():
            missing.append((name, str(where)))
            if not keep_missing:
                continue
        added.append(name)

    print(f"  settings: {len(added)} to add, {len(updated)} to update, "
          f"{len(existing)} already present, {len(missing)} skipped (content directory missing)")
    for name, where in missing:
        print(f"    skip {name}: {where} is not a directory")
    for name, _ in updated:
        print(f"    update {name}: its stored fields differ from the source's")
    if dry_run or not (added or updated):
        return 0

    nl = "\r\n" if "\r\n" in target_text else "\n"
    text = target_text
    if updated:
        text = rewrite_sections(text, dict(updated), nl)
    if added:
        text = text if text.endswith(("\n", "\r")) else text + nl
        text += "".join(f"[{name}]" + nl + nl.join(body) + nl for name, body in source_sections
                        if name in added)
    print(f"    backup {backup(target).name}")
    target.write_bytes(text.encode("utf-8", "surrogateescape"))
    return len(added) + len(updated)


def copy_hints(source: Path, target: Path, dry_run: bool) -> int:
    """Add the source's library entries to the target's, keyed by `RegKey`.

    A count comparison was the wrong test twice over: it can leave a source library
    absent (same counts, different identities) after `merge_settings` has already
    written that library's section -- which is a target that looks registered and is
    not -- and it replaced the whole file, discarding entries only the target had.
    Target-only entries are kept; an entry both sides have is taken from the source,
    because that is the install whose registration the caller asked to propagate.
    """
    want, have = hints_keys(source), hints_keys(target)
    new = [k for k in want if k not in have]
    differing = [k for k in want if k in have and ET.tostring(want[k]) != ET.tostring(have[k])]
    print(f"  hints   : source {len(want)}, target {len(have)}; "
          f"{len(new)} new, {len(differing)} differ, {len(have) - len(differing)} target-only/equal")
    if not (new or differing):
        print("    nothing to do")
        return 0
    if dry_run:
        return len(new) + len(differing)
    if target.exists():
        print(f"    backup {backup(target).name}")
        root = ET.parse(target).getroot()
        for key in differing:
            for i, lib in enumerate(root.findall("Library")):
                name = (lib.findtext("RegKey") or lib.findtext("Name") or "").strip()
                if name == key:
                    root.remove(lib)
                    root.insert(i, copy.deepcopy(want[key]))
                    break
    else:
        root = ET.Element("LibraryHints")
    for key in new:
        root.append(copy.deepcopy(want[key]))
    target.parent.mkdir(parents=True, exist_ok=True)
    try:
        ET.indent(root)
    except AttributeError:  # pragma: no cover - Python < 3.9
        pass
    ET.ElementTree(root).write(target, encoding="UTF-8", xml_declaration=True)
    return len(new) + len(differing)


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
    ap.add_argument("--no-database", action="store_true",
                    help="do not install the source's komplete.db3 (browser index; not needed for loading)")
    ap.add_argument("--refresh-database", action="store_true",
                    help="replace an already-installed komplete.db3 (backed up first); the "
                         "browser index can otherwise go stale when the source gains libraries")
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
    if not args.no_database:
        changed += copy_database(best, target, args.dry_run, args.refresh_database)
    print("dry run: nothing written" if args.dry_run else
          (f"done: {changed} registrations added. next: tools/oracle-mcp/smoke.sh"
           if changed else "nothing to do"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
