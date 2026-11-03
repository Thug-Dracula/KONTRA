#!/usr/bin/env python3
"""Checks for tools/parity-corpus/run.py. Run: python3 tools/test_parity_corpus.py

The gate is only as good as the files behind it, and the one failure it cannot see
on its own is a case whose reference stopped early: `compare` cuts both sides to
their common length, so a take that holds 1.5 s of a 2.5 s note goes on passing
while measuring less of the note than the case claims. These tests pin the two
guards that close that hole (a missing take and a take of the wrong length) and
the controller rule the `accordions` case depends on: a controller must reach the
engine before the note-on it decides, which in a single MIDI track means before
it on the same tick.

The committed corpus is checked too, so a half-recorded case cannot be merged by
someone who never ran the gate.
"""
import importlib.util
import struct
import tempfile
import unittest
from array import array
from pathlib import Path

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("parity_run", HERE / "parity-corpus" / "run.py")
run = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(run)


def write_wav(path: Path, frames: int, channels: int = 2) -> None:
    """A float32 WAV of `frames` silent frames, the shape a reference is committed as."""
    data = array("f", [0.0] * (frames * channels)).tobytes()
    fmt = struct.pack("<HHIIHH", 3, channels, run.RATE, run.RATE * channels * 4,
                      channels * 4, 32)
    body = b"fmt " + struct.pack("<I", len(fmt)) + fmt + b"data" + struct.pack("<I", len(data))
    path.write_bytes(b"RIFF" + struct.pack("<I", 4 + len(body) + len(data)) + b"WAVE"
                     + body + data)


class Case:
    """One case's shape: a note of `length_s` held at 0, and a tail after it."""

    def __init__(self, cid="a-case", length_s=2.0, tail_s=0.5, references=None, ccs=None):
        self.doc = {
            "id": cid,
            "midi": {"type": "note",
                     "notes": [{"start_s": 0.0, "length_s": length_s,
                                "key": 60, "velocity": 100}]},
            "tail_s": tail_s,
            "references": references if references is not None
            else [f"kontakt/{cid}-a.wav", f"kontakt/{cid}-b.wav"],
        }
        if ccs is not None:
            self.doc["midi"]["ccs"] = ccs


class References(unittest.TestCase):
    """A take has to be there, and it has to be as long as the case's own MIDI."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)
        self.saved = run.REFS
        run.REFS = self.dir

    def tearDown(self):
        run.REFS = self.saved
        self.tmp.cleanup()

    def test_a_complete_pair_passes_and_returns_the_first_take(self):
        case = Case()
        for take in "ab":
            write_wav(self.dir / f"a-case-{take}.wav", 120_000)
        row = {"id": "a-case"}
        chans = run.check_references(case.doc, row)
        self.assertIsNotNone(chans, row)
        self.assertEqual(len(chans[0]), 120_000)
        self.assertNotIn("status", row)

    def test_a_missing_take_fails_the_case_by_name(self):
        case = Case()
        write_wav(self.dir / "a-case-a.wav", 120_000)
        row = {"id": "a-case"}
        self.assertIsNone(run.check_references(case.doc, row))
        self.assertFalse(row["ok"])
        self.assertEqual(row["status"], "reference missing")
        self.assertIn("a-case-b.wav", row["detail"])

    def test_a_short_take_fails_instead_of_comparing_less_of_the_note(self):
        case = Case()
        write_wav(self.dir / "a-case-a.wav", 72_000)  # 1.5 s of a 2.5 s capture
        write_wav(self.dir / "a-case-b.wav", 120_000)
        row = {"id": "a-case"}
        self.assertIsNone(run.check_references(case.doc, row))
        self.assertFalse(row["ok"])
        self.assertEqual(row["status"], "reference length wrong")
        self.assertIn("72000 frames", row["detail"])
        self.assertIn("120000", row["detail"])

    def test_the_expected_length_is_the_last_key_up_plus_the_tail(self):
        self.assertEqual(run.capture_frames(Case().doc), 120_000)
        self.assertEqual(run.capture_frames(Case(length_s=1.0, tail_s=0.25).doc), 60_000)


class Midi(unittest.TestCase):
    """The file both engines play: a controller that decides the note precedes it."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.path = Path(self.tmp.name) / "case.mid"

    def tearDown(self):
        self.tmp.cleanup()

    def events(self) -> list[tuple[int, int]]:
        """(status, data1) of every channel event in the one track, in file order."""
        data = self.path.read_bytes()
        header = struct.unpack(">I", data[4:8])[0]
        track = data[8 + header + 8:]
        at, out, tick, running = 0, [], 0, 0
        while at < len(track):
            value = 0
            while True:
                value = (value << 7) | (track[at] & 0x7F)
                at += 1
                if not track[at - 1] & 0x80:
                    break
            first = track[at]
            if first & 0x80:
                at += 1
                status = first
            else:
                status = running
            if status == 0xFF:
                kind = track[at]
                at += 1
                size = 0
                while True:
                    size = (size << 7) | (track[at] & 0x7F)
                    at += 1
                    if not track[at - 1] & 0x80:
                        break
                at += size
                continue
            out.append((status, track[at]))
            running = status
            at += 1 if (status & 0xF0) in (0xC0, 0xD0) else 2
        return out

    def test_a_controller_shares_tick_zero_with_the_note_and_comes_first(self):
        case = Case(ccs=[{"at_s": 0.0, "cc": 2, "value": 64}])
        run.write_case_midi(case.doc, self.path)
        statuses = self.events()
        self.assertEqual(statuses[0], (0xB0, 2), "the controller must lead the note-on")
        self.assertEqual(statuses[1], (0x90, 60))

    def test_no_controller_means_a_note_on_and_off_only(self):
        run.write_case_midi(Case().doc, self.path)
        self.assertEqual(self.events(), [(0x90, 60), (0x80, 60)])


class Corpus(unittest.TestCase):
    """Every case in cases.json: takes present, and as long as its own MIDI says."""

    def test_every_case_carries_takes_of_the_length_its_midi_captures(self):
        doc = run.load_cases()
        for case in doc["cases"]:
            if not case["references"]:
                # `lass-aleatoric-cellos` is refused by both engines: no reference at all.
                self.assertEqual(case.get("expect"), "expected-refusal",
                                 f"{case['id']}: no takes and not a recorded refusal")
                continue
            row = {"id": case["id"]}
            want = run.capture_frames(case)
            for name in case["references"]:
                path = run.REFS / Path(name).name
                self.assertTrue(path.exists(), f"{case['id']}: {name} is not on disk")
                chans, _ = run.read_wav(path)
                self.assertEqual(len(chans[0]), want,
                                 f"{case['id']}: {path.name} is not {want} frames")
            self.assertIsNotNone(run.check_references(case, row),
                                 f"{case['id']}: {row.get('detail') or row.get('status')}")


if __name__ == "__main__":
    unittest.main(verbosity=2)
