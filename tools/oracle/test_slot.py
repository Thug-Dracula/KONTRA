#!/usr/bin/env python3
"""Checks slot.remap_midi on a channel-1 file with running status and a dropped event. No Kontakt needed.

    tools/oracle/test_slot.py
"""
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import slot  # noqa: E402


def smf(track):
    return b"MThd" + (6).to_bytes(4, "big") + bytes([0, 0, 0, 1, 0x01, 0xE0]) + b"MTrk" + len(track).to_bytes(4, "big") + track


def events(path):
    """(delta, status, data) per channel event, after expanding nothing: the remapped file uses explicit status."""
    data = Path(path).read_bytes()
    trk = data[22:]
    i, out = 0, []
    while i < len(trk):
        delta, i = slot._vlq_read(trk, i)
        if trk[i] == 0xFF:
            n, j = slot._vlq_read(trk, i + 2)
            i = j + n
            continue
        status = trk[i]
        out.append((delta, status, trk[i + 1:i + 3]))
        i += 3
    return out


def install_case():
    """slot.install: the directory above x64/, and nothing for a flat build.

    `dump.py` builds `UserData/Settings.cfg` from it; a flat portable
    (`…/kontakt-8.13.1-re/Kontakt 8.exe`) has no x64/ above it, and naming
    `parents[2]` there aimed the settings path at whatever directory happened
    to be two levels up.
    """
    with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parent) as d:
        root = Path(d)
        nested = root / "Kontakt 8"
        (nested / "x64").mkdir(parents=True)
        exe = nested / "x64" / "Kontakt 8.exe"
        exe.touch()
        assert slot.install(exe) == nested, slot.install(exe)
        assert slot.install(exe) / "UserData/Settings.cfg" == nested / "UserData/Settings.cfg"
        flat = root / "kontakt-8.13.1-re"
        flat.mkdir()
        (flat / "Kontakt 8.exe").touch()
        assert slot.install(flat / "Kontakt 8.exe") is None, "a flat build has no x64/ above it"


def main():
    install_case()
    # delta 0 note-on 60 (status 0x90), delta 0 note-on 62 via running status, delta 10 CC on channel 1,
    # then end of track
    track = bytes([0x00, 0x90, 60, 100, 0x00, 62, 100, 0x0A, 0xB0, 1, 127, 0x00, 0xFF, 0x2F, 0x00])
    with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parent) as d:
        src, dst = Path(d) / "in.mid", Path(d) / "out.mid"
        src.write_bytes(smf(track))
        slot.remap_midi(src, dst, first=5)
        got = events(dst)
        assert got == [(0, 0x95, bytes([60, 100])), (0, 0x95, bytes([62, 100])), (10, 0xB5, bytes([1, 127]))], got
        # block 15: channel 1 (0x91) would land on 16 and is dropped; the delta it carried moves to the next event
        track = bytes([0x00, 0x90, 60, 100, 0x04, 0x91, 1, 127, 0x06, 0x90, 62, 100, 0x00, 0xFF, 0x2F, 0x00])
        src.write_bytes(smf(track))
        slot.remap_midi(src, dst, first=15)
        got = events(dst)
        assert got == [(0, 0x9F, bytes([60, 100])), (10, 0x9F, bytes([62, 100]))], got
    print("test_slot: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
