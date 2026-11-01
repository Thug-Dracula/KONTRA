"""Checks for tools/plan/plan.py. Run: python3 -m unittest tools/plan/test_plan.py"""
import contextlib
import io
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import plan  # noqa: E402


class Globs(unittest.TestCase):
    def test_match(self):
        self.assertTrue(plan.match("crates/dsp/eq/src/lib.rs", ["crates/dsp/eq/**"]))
        self.assertTrue(plan.match("src/sound/v2.rs", ["src/sound/v2.rs"]))
        self.assertFalse(plan.match("src/sound/v2s.rs", ["src/sound/v2.rs"]))
        self.assertTrue(plan.match("a/color/bite.rs", ["a/color/{bite,dirt}.rs"]))
        self.assertFalse(plan.match("a/color/amp/x.rs", ["a/color/*.rs"]))
        self.assertTrue(plan.match("crates/x/Cargo.toml", ["crates/*/Cargo.toml"]))

    def test_overlap_new_paths(self):
        self.assertTrue(plan.overlap(["crates/dsp/filter/**"], ["crates/dsp/filter/src/ladder*"], []))
        self.assertFalse(plan.overlap(["crates/dsp/filter/src/ladder*"], ["crates/dsp/filter/src/formant*"], []))
        self.assertFalse(plan.overlap(["crates/dsp/eq/**"], ["crates/dsp/dynamics/**"], []))

    def test_overlap_existing(self):
        self.assertEqual(plan.overlap(["a/*.rs"], ["a/b.rs"], ["a/b.rs", "a/c.rs"]), ["a/b.rs"])


class Manifest(unittest.TestCase):
    def setUp(self):
        self.plan = plan.load()

    def test_valid(self):
        errors, _ = plan.validate(self.plan)
        self.assertEqual(errors, [])

    def test_cycle_detected(self):
        p = plan.load()
        p["by_id"]["P0.1"]["deps"] = ["P0.6"]
        with self.assertRaises(ValueError):
            plan.topo(p)

    def test_offline_ready_is_conflict_free(self):
        st = plan.status(self.plan, {})
        batch, _ = plan.ready_batch(self.plan, st)
        self.assertIn("P0.0", batch)
        skip = ("append", "regenerate")
        for i, a in enumerate(batch):
            for b in batch[i + 1:]:
                self.assertFalse(plan.overlap(plan.owns(self.plan, self.plan["by_id"][a], skip_modes=skip),
                                              plan.owns(self.plan, self.plan["by_id"][b], skip_modes=skip),
                                              list(plan.tracked())), (a, b))

    def test_brief_and_branch(self):
        text = plan.brief(self.plan, "P1.2")
        self.assertIn("<!-- v3-task: P1.2 -->", text)
        self.assertIn("crates/sampler-kontakt/src/stream.rs", text)
        # The mantra comes from VISION.md, so every brief carries the current one.
        self.assertIn(f"> **{plan.mantra()}**", text)
        self.assertTrue(plan.mantra().startswith("One gold-standard engine."))
        self.assertIn("Vision check", text)
        self.assertEqual(plan.task_of("v3-P5.3a-filter-part-a", self.plan), "P5.3a")
        self.assertIsNone(plan.task_of("v3-P9.9-nope", self.plan))
        self.assertIsNone(plan.task_of("fix/other", self.plan))

    def test_sync_keeps_claims(self):
        self.assertFalse(plan.managed("status:claimed"))
        self.assertFalse(plan.managed("scope-override"))
        self.assertFalse(plan.managed("size-growth"))
        self.assertTrue(plan.managed("status:ready"))
        self.assertTrue(plan.managed("phase:3"))

    def test_generated_files_follow_their_source(self):
        brief = plan.brief(self.plan, "P1.1b")
        self.assertIn("ksp_symbols.py sync", brief)
        self.assertNotIn("ksp_symbols.py sync", plan.brief(self.plan, "P0.7"))

    def test_any_task_may_touch_the_lockfile(self):
        self.assertTrue(plan.match("Cargo.lock", plan.allowed(self.plan, "P4.1")))
        self.assertTrue(plan.match("tools/xtask/size-baseline.json", plan.allowed(self.plan, "P4.1")))
        self.assertFalse(plan.match("Cargo.toml", plan.allowed(self.plan, "P4.1")))

    def test_slug(self):
        self.assertEqual(plan.slug("5. Current → new mapping"), "5-current--new-mapping")
        self.assertEqual(plan.slug("Phase 1–3: playback (users hear it)"), "phase-13-playback-users-hear-it")


class Locks(unittest.TestCase):
    def empty_globs(self, p: dict) -> list[str]:
        """Lock globs that match no tracked file, as readable strings."""
        files = list(plan.tracked())
        return [f"@{name}: {glob}"
                for name, lock in p["locks"].items()
                for glob in lock["paths"]
                if not any(plan.match(f, [glob]) for f in files)]

    def test_lock_paths_exist(self):
        """A lock naming deleted paths locks the empty set, so `ready` hands out conflicting work.

        This is the failure mode that made `@v2rs`, `@plugin` and `@corelib` useless: the crates/
        reorganisation moved every file under `src/` and `crates/sampler-core/` away, the locks
        kept their old paths, and the conflict detector compared empty sets without complaining.
        """
        self.plan = plan.load()
        for name, lock in self.plan["locks"].items():
            for glob in lock["paths"]:
                prefix = plan.literal(glob).rstrip("/")
                self.assertTrue(prefix, f"@{name}: {glob} must start with a real path")
                self.assertTrue((plan.ROOT / prefix).exists(),
                                f"@{name} locks a path that no longer exists: {glob}")

    def test_every_lock_glob_matches_a_tracked_file(self):
        """The prefix existing is not the question: a lock must match something.

        `crates/` exists and `crates/legacy-v2/**` matches nothing, so a test that stops at the
        literal prefix passes for a lock that protects no file at all -- which is the same dead
        lock the four deleted paths above were, one reorganisation later.
        """
        self.assertEqual(self.empty_globs(plan.load()), [])

    def test_a_lock_whose_prefix_exists_but_matches_nothing_is_caught(self):
        """Prove the check above can fail: a real prefix, a glob that matches no file."""
        p = plan.load()
        ghost = dict(p["locks"]["manifests"])
        ghost["paths"] = ["crates/**/*.no-such-extension"]
        p["locks"]["ghost"] = ghost
        self.assertTrue((plan.ROOT / "crates").exists(), "the prefix this test plants does exist")
        self.assertEqual(self.empty_globs(p), ["@ghost: crates/**/*.no-such-extension"])


class WarningRatchet(unittest.TestCase):
    """`validate` is lenient by default; `--max-warnings` turns the count into a ratchet."""

    # One task that owns `tools/plan/plan.py`, which P0.8 (the repo-wide `**/*.rs`
    # guard-rail task) and others already own: it can only add "serialized" warnings.
    EXTRA = """
[[task]]
id = "P0.99"
title = "Ratchet probe"
phase = 0
lane = "coordinator"
size = "S"
agent = "coordinator"
deps = ["P0.0"]
owns = ["tools/plan/plan.py"]
docs = ["12-WORKFLOW.md"]
goal = "Exists only inside test_plan.py."
steps = ["Nothing."]
gate = ["true"]
done = ["Nothing."]
"""

    def validate(self, *args: str) -> tuple[int, str]:
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = plan.main(["validate", *args])
        return code, out.getvalue()

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.manifest = Path(self.tmp.name) / "tasks.toml"
        self.manifest.write_text(plan.MANIFEST.read_text() + self.EXTRA)

    def test_ratchet_fails_on_growth(self):
        today = len(plan.validate(plan.load())[1])
        grown = len(plan.validate(plan.load(self.manifest))[1])
        self.assertGreater(grown, today, "the extra task must add warnings")
        self.assertEqual(self.validate("--manifest", str(self.manifest), "--max-warnings", str(grown))[0], 0)
        code, out = self.validate("--manifest", str(self.manifest), "--max-warnings", str(today))
        self.assertEqual(code, 1, out)
        self.assertIn("new warning: serialized: P0.8 and P0.99 share tools/plan/plan.py", out)
        self.assertIn(f"warning budget exceeded: {grown} warnings, budget {today}", out)

    def test_baseline_is_current(self):
        """The committed baseline must not be below today's count, or CI fails on a clean tree."""
        warnings = plan.validate(plan.load())[1]
        baseline = plan.read_baseline(plan.WARNINGS_BASELINE)
        self.assertIsNotNone(baseline, "the ratchet has no baseline committed")
        self.assertLessEqual(len(warnings), len(baseline),
                             "warnings grew: fix the conflict, or accept it in a v3-plan- PR with "
                             "`plan.py validate --write-baseline`")
        self.assertEqual(self.validate("--max-warnings", "baseline")[0], 0)
        self.assertEqual(self.validate("--max-warnings", "baseline")[1].count("\nerror:"), 0)

    def test_a_swap_is_not_a_pass(self):
        """One warning gone and another in its place: the count is flat, the change is not.

        This is what a count-based ratchet could not see. The warning that appeared was never
        reviewed because the total did not move, which is the whole failure the ratchet exists
        to prevent.
        """
        warnings = plan.validate(plan.load())[1]
        planted = "serialized: P99.9 and P99.8 share nowhere.rs"
        # The baseline says `planted` where today's tree says `warnings[0]`: one
        # warning gone, another in its place, and not a single extra warning.
        swapped = [planted, *warnings[1:]]
        self.assertEqual(len(swapped), len(warnings), "the swap must keep the count identical")
        self.assertNotIn(planted, warnings)
        path = Path(self.tmp.name) / "swapped-baseline.txt"
        path.write_text("# a baseline of the same length with one warning replaced\n"
                        + "\n".join(swapped) + "\n")
        code, out = self.validate("--max-warnings", "baseline", "--warnings-baseline", str(path))
        self.assertEqual(code, 1, "a swap at an unchanged count must not pass")
        self.assertIn(f"error: new warning: {warnings[0]}", out)
        # And the one it replaced is reported separately, so the pair reads as a change.
        self.assertIn(f"  gone: {planted}", out)

    def test_an_empty_baseline_is_an_error(self):
        """An empty file ratchets nothing, and that must not read as "not configured"."""
        path = Path(self.tmp.name) / "empty-baseline.txt"
        path.write_text("")
        code, out = self.validate("--max-warnings", "baseline", "--warnings-baseline", str(path))
        self.assertEqual(code, 1, out)
        self.assertIn("is empty", out)
        self.assertNotIn("no baseline at", out)

    def test_a_header_only_baseline_is_an_error(self):
        path = Path(self.tmp.name) / "header-only.txt"
        path.write_text("# v3 plan warnings: the ratchet baseline.\n#\n\n")
        code, out = self.validate("--max-warnings", "baseline", "--warnings-baseline", str(path))
        self.assertEqual(code, 1, out)
        self.assertIn("is empty", out)

    def test_an_absent_baseline_is_not_configured(self):
        """Not there is a different answer from there and empty, and it says so."""
        path = Path(self.tmp.name) / "absent.txt"
        code, out = self.validate("--max-warnings", "baseline", "--warnings-baseline", str(path))
        self.assertEqual(code, 1, out)
        self.assertIn("no baseline at", out)
        self.assertNotIn("is empty", out)


class Baseline(unittest.TestCase):
    def test_absent_and_empty_are_different_answers(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "warnings-baseline.txt"
            self.assertIsNone(plan.read_baseline(path))
            path.write_text("# only a comment\n\n")
            self.assertEqual(plan.read_baseline(path), [])
            path.write_text("# c\nserialized: a and b share c.rs\n")
            self.assertEqual(plan.read_baseline(path), ["serialized: a and b share c.rs"])


if __name__ == "__main__":
    unittest.main()
