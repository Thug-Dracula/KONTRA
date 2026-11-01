"""Self-check for tools/oracle-mcp/register-libraries.py (no Kontakt, no display).

    python3 -m unittest discover -s tools/oracle-mcp -p 'test_*.py'

Two behaviours here were review findings, both of the shape this project keeps
paying for -- a check that reports success over the wrong question:

* `merge_settings` skipped a library whose section already existed, so a
  registration whose *identity* fields had moved stayed stale while the command
  said "already present". It now compares identity and updates the section --
  but deliberately **not** ContentDir, because a portable install writes that
  relative to its own root and a normal install writes it absolute, and the two
  spellings are both correct for a library that loads.

* `copy_hints` decided whether to copy `LibraryHints.xml` by counting
  `Library>` elements. Equal counts do not prove equal identities, and replacing
  the whole file discarded entries only the target had. It now merges by
  `RegKey`.
"""

import importlib.util
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "register_libraries", Path(__file__).with_name("register-libraries.py"))
rl = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(rl)

def sz_path(p: Path) -> str:
    """A posix path as Kontakt's `Z:` spelling (`/` is `Z:` here). LIB adds the `sz:`."""
    return "Z:" + str(p).replace("/", "\\")


LIB = """[{name}]

Name=sz:{name}
SNPID=sz:K54
Company=sz:Native Instruments GmbH
ContentDir=sz:{where}
ContentVersion=sz:1.1.0
HU=sz:{hu}
JDX=sz:C4472FD3
"""


def install(root: Path, libraries: list[tuple[str, str, str]]) -> Path:
    """A fake install: UserData/Settings.cfg plus Service Center/LibraryHints.xml."""
    (root / "UserData/Service Center").mkdir(parents=True, exist_ok=True)
    cfg = "".join(LIB.format(name=n, where=w, hu=hu) for n, w, hu in libraries)
    (root / "UserData/Settings.cfg").write_text(cfg)
    xml = ["<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<LibraryHints>\n"]
    for n, _, _ in libraries:
        xml.append(f"  <Library><Name>{n}</Name><RegKey>{n}</RegKey></Library>\n")
    xml.append("</LibraryHints>\n")
    (root / "UserData/Service Center/LibraryHints.xml").write_text("".join(xml))
    return root


class Settings(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def test_a_section_with_a_moved_identity_is_updated(self):
        # The target registered Brass at a content dir that has since moved, with the
        # old authorization hash. A command that reports "already present" leaves
        # Kontakt refusing it.
        src = install(self.root / "src", [("Brass", r"X:\libs\Brass", "AAAA")])
        dst = install(self.root / "dst", [("Brass", r"X:\old\Brass", "OLD")])
        target = dst / "UserData/Settings.cfg"

        import contextlib
        import io
        with contextlib.redirect_stdout(io.StringIO()) as out:
            changed = rl.merge_settings(src / "UserData/Settings.cfg", target, False, False)

        self.assertEqual(changed, 1, out.getvalue())
        after = target.read_text()
        self.assertIn(r"ContentDir=sz:X:\libs\Brass", after)
        self.assertIn("HU=sz:AAAA", after)
        self.assertNotIn("OLD", after)
        # and the old file is kept
        self.assertTrue(list(target.parent.glob("Settings.cfg.pre-libraries-*")))

    def test_a_section_that_only_spells_its_content_dir_differently_is_left_alone(self):
        # A portable install writes `sz:\Libraries\<name>`; the source writes an
        # absolute path for the same library. Rewriting it would be a change made to
        # look uniform, on a library that loads.
        src = install(self.root / "src", [("Mint", r"X:\VSTs\Mint", "AAAA")])
        dst = install(self.root / "dst", [("Mint", r"\Libraries\Mint", "AAAA")])

        import contextlib
        import io
        with contextlib.redirect_stdout(io.StringIO()) as out:
            changed = rl.merge_settings(src / "UserData/Settings.cfg",
                                       dst / "UserData/Settings.cfg", False, False)

        self.assertEqual(changed, 0, out.getvalue())
        self.assertIn(r"ContentDir=sz:\Libraries\Mint",
                      (dst / "UserData/Settings.cfg").read_text())
        self.assertIn("keep Mint", out.getvalue())

    def test_a_missing_section_is_appended(self):
        # A section is only added when its ContentDir exists: a registration pointing
        # nowhere is a library Kontakt lists and cannot open.
        where = self.root / "libs/Brass"
        where.mkdir(parents=True)
        src = install(self.root / "src", [("Brass", sz_path(where), "AAAA")])
        dst = install(self.root / "dst", [("Mint", r"\Libraries\Mint", "BBBB")])

        import contextlib
        import io
        with contextlib.redirect_stdout(io.StringIO()):
            changed = rl.merge_settings(src / "UserData/Settings.cfg",
                                       dst / "UserData/Settings.cfg", False, False)

        self.assertEqual(changed, 1)
        after = (dst / "UserData/Settings.cfg").read_text()
        self.assertIn("[Brass]", after)
        self.assertIn("[Mint]", after)


class Hints(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def test_equal_counts_with_different_libraries_still_copy_the_missing_one(self):
        # The old count test: two libraries each, so "nothing to do" -- and the source's
        # Brass is never registered although settings.cfg now names it.
        src = install(self.root / "src", [("Brass", r"X:\libs\Brass", "A")])
        dst = install(self.root / "dst", [("Mint", r"X:\libs\Mint", "B")])

        import contextlib
        import io
        with contextlib.redirect_stdout(io.StringIO()) as out:
            changed = rl.copy_hints(src / "UserData/Service Center/LibraryHints.xml",
                                   dst / "UserData/Service Center/LibraryHints.xml", False)

        self.assertEqual(changed, 1, out.getvalue())
        text = (dst / "UserData/Service Center/LibraryHints.xml").read_text()
        self.assertIn("Brass", text)
        self.assertIn("Mint", text, "a target-only library was discarded")
        self.assertTrue(list((dst / "UserData/Service Center")
                             .glob("LibraryHints.xml.pre-libraries-*")))

    def test_identical_lists_are_left_alone(self):
        src = install(self.root / "src", [("Brass", r"X:\a", "A")])
        dst = install(self.root / "dst", [("Brass", r"X:\a", "A")])

        import contextlib
        import io
        with contextlib.redirect_stdout(io.StringIO()):
            changed = rl.copy_hints(src / "UserData/Service Center/LibraryHints.xml",
                                   dst / "UserData/Service Center/LibraryHints.xml", False)

        self.assertEqual(changed, 0)


class Database(unittest.TestCase):
    def test_refresh_replaces_a_populated_target_and_keeps_a_backup(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            src = root / "src/UserData/Kontakt 8"
            dst = root / "dst/UserData/Kontakt 8"
            src.mkdir(parents=True)
            dst.mkdir(parents=True)
            # A real (tiny) SQLite database: copy_database folds the WAL with
            # `PRAGMA wal_checkpoint`, which a file of bytes is not.
            import sqlite3
            for path, tag in ((src / "komplete.db3", "source"), (dst / "komplete.db3", "target")):
                con = sqlite3.connect(path)
                con.execute("CREATE TABLE marker (who TEXT)")
                con.execute("INSERT INTO marker VALUES (?)", (tag,))
                con.commit()
                con.close()

            import contextlib
            import io
            import sqlite3

            def who(path: Path) -> str:
                con = sqlite3.connect(path)
                try:
                    return con.execute("SELECT who FROM marker").fetchone()[0]
                finally:
                    con.close()

            with contextlib.redirect_stdout(io.StringIO()):
                # Without the flag: the size guard is a deliberate no-op.
                self.assertEqual(
                    rl.copy_database(root / "src", root / "dst", False, False), 0)
                self.assertEqual(who(dst / "komplete.db3"), "target")
                # With it: replaced, and the old file kept under a dated name.
                self.assertEqual(
                    rl.copy_database(root / "src", root / "dst", False, True), 1)

            self.assertEqual(who(dst / "komplete.db3"), "source")
            self.assertTrue(list(dst.glob("komplete.db3.pre-populated-*")))


if __name__ == "__main__":
    unittest.main()
