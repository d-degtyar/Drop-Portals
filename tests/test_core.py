#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Тести для правил, переміщення файлів і збереження налаштувань
"""

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from dropportals.config import AppConfig, Portal, Rule, load_config, save_config  # noqa: E402
from dropportals.mover import transfer, transfer_many, undo, unique_path  # noqa: E402
from dropportals.presets import make_preset_portal, make_sorter_portal  # noqa: E402
from dropportals.rules import (normalize_extensions, render_subfolder, resolve_destination,  # noqa: E402
                               rule_matches, rules_summary)


class TempDirCase(unittest.TestCase):

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        self.src = self.root / "Downloads"
        self.dst = self.root / "Target"
        self.src.mkdir()

    def tearDown(self):
        self._tmp.cleanup()

    def make(self, name: str, data: bytes = b"x") -> str:
        path = self.src / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        return str(path)


class TestRules(TempDirCase):

    def test_normalize_extensions(self):
        self.assertEqual(normalize_extensions("pdf, .DOCX; *.xls  pdf"), ["pdf", "docx", "xls"])
        self.assertEqual(normalize_extensions(""), [])

    def test_extension_and_pattern(self):
        path = self.make("Invoice_March.PDF")
        self.assertTrue(rule_matches(Rule(extensions=["pdf"]), path))
        self.assertFalse(rule_matches(Rule(extensions=["mp4"]), path))
        self.assertTrue(rule_matches(Rule(pattern="*invoice*"), path))
        self.assertFalse(rule_matches(Rule(pattern="report*"), path))
        self.assertFalse(rule_matches(Rule(extensions=["pdf"], enabled=False), path))

    def test_size_limits(self):
        path = self.make("big.bin", b"0" * (2 * 1024 * 1024))
        self.assertTrue(rule_matches(Rule(min_mb=1), path))
        self.assertFalse(rule_matches(Rule(min_mb=3), path))
        self.assertFalse(rule_matches(Rule(max_mb=1), path))

    def test_folders_skip_extension_rules(self):
        folder = self.src / "pack"
        folder.mkdir()
        self.assertFalse(rule_matches(Rule(extensions=["zip"]), str(folder)))
        self.assertTrue(rule_matches(Rule(pattern="pa*"), str(folder)))

    def test_render_subfolder_is_safe(self):
        path = self.make("clip.mov")
        self.assertEqual(render_subfolder("Video/{EXT}", path), os.path.join("Video", "MOV"))
        self.assertEqual(render_subfolder("../../{ext}", path), "mov")
        self.assertEqual(render_subfolder('a<b>:c', path), "a_b__c")
        self.assertRegex(render_subfolder("{year}-{month}", path), r"^\d{4}-\d{2}$")
        self.assertEqual(render_subfolder("{unknown}", path), "{unknown}")

    def test_preset_portal_rejects_other_types(self):
        portal = make_preset_portal("Відео", str(self.dst))
        self.assertEqual(resolve_destination(portal, self.make("a.mp4")), str(self.dst))
        self.assertIsNone(resolve_destination(portal, self.make("a.pdf")))

    def test_sorter_uses_subfolders(self):
        portal = make_sorter_portal(str(self.dst))
        self.assertEqual(resolve_destination(portal, self.make("a.pdf")), str(self.dst / "Документи"))
        self.assertEqual(resolve_destination(portal, self.make("a.wav")), str(self.dst / "Аудіо"))
        self.assertEqual(resolve_destination(portal, self.make("a.xyz")), str(self.dst))

    def test_portal_without_folder_or_rules(self):
        self.assertIsNone(resolve_destination(Portal(), self.make("a.txt")))
        portal = Portal(folder=str(self.dst), unmatched="skip")
        self.assertEqual(resolve_destination(portal, self.make("b.txt")), str(self.dst))

    def test_summary(self):
        self.assertEqual(rules_summary(Portal()), "усі файли")
        self.assertEqual(rules_summary(Portal(rules=[Rule(extensions=["a", "b", "c", "d", "e"])])), "a b c d +1")


class TestMover(TempDirCase):

    def test_move_and_undo(self):
        path = self.make("doc.pdf", b"hello")
        result = transfer(path, str(self.dst))
        self.assertEqual(result.action, "moved")
        self.assertFalse(os.path.exists(path))
        self.assertEqual(Path(result.target).read_bytes(), b"hello")
        self.assertEqual(undo([result]), [])
        self.assertTrue(os.path.exists(path))
        self.assertFalse(os.path.exists(result.target))

    def test_copy_and_undo(self):
        path = self.make("doc.pdf")
        result = transfer(path, str(self.dst), mode="copy")
        self.assertEqual(result.action, "copied")
        self.assertTrue(os.path.exists(path))
        undo([result])
        self.assertTrue(os.path.exists(path))
        self.assertFalse(os.path.exists(result.target))

    def test_conflict_rename(self):
        self.dst.mkdir()
        (self.dst / "a.txt").write_text("old")
        result = transfer(self.make("a.txt", b"new"), str(self.dst))
        self.assertEqual(Path(result.target).name, "a (2).txt")
        self.assertEqual((self.dst / "a.txt").read_text(), "old")
        self.assertEqual(Path(unique_path(str(self.dst / "a.txt"))).name, "a (3).txt")

    def test_conflict_skip_and_replace(self):
        self.dst.mkdir()
        (self.dst / "a.txt").write_text("old")
        skipped = transfer(self.make("a.txt", b"new"), str(self.dst), conflict="skip")
        self.assertEqual(skipped.action, "skipped")
        replaced = transfer(self.make("a.txt", b"new"), str(self.dst), conflict="replace")
        self.assertEqual(replaced.action, "moved")
        self.assertEqual((self.dst / "a.txt").read_text(), "new")
        self.assertEqual(sorted(os.listdir(self.dst)), ["a.txt"])

    def test_folder_move_and_self_protection(self):
        folder = self.src / "pack"
        (folder / "inner").mkdir(parents=True)
        (folder / "inner" / "f.txt").write_text("1")
        bad = transfer(str(folder), str(folder / "inner"))
        self.assertEqual(bad.action, "error")
        good = transfer(str(folder), str(self.dst))
        self.assertEqual(good.action, "moved")
        self.assertTrue((self.dst / "pack" / "inner" / "f.txt").exists())

    def test_same_folder_and_missing(self):
        path = self.make("a.txt")
        self.assertEqual(transfer(path, str(self.src)).action, "skipped")
        self.assertEqual(transfer(str(self.src / "nope.txt"), str(self.dst)).action, "skipped")

    def test_transfer_many_with_rules(self):
        portal = make_sorter_portal(str(self.dst))
        paths = [self.make("a.pdf"), self.make("b.mp4"), self.make("c.unknown")]
        progress = []
        results = transfer_many(portal, paths, progress=lambda d, t: progress.append((d, t)))
        self.assertTrue(all(r.ok for r in results))
        self.assertTrue((self.dst / "Документи" / "a.pdf").exists())
        self.assertTrue((self.dst / "Відео" / "b.mp4").exists())
        self.assertTrue((self.dst / "c.unknown").exists())
        self.assertEqual(progress[-1], (3, 3))
        undo(results)
        self.assertEqual(sorted(os.listdir(self.src)), ["a.pdf", "b.mp4", "c.unknown"])

    def test_transfer_many_skips_rejected(self):
        portal = make_preset_portal("Відео", str(self.dst))
        results = transfer_many(portal, [self.make("a.pdf")])
        self.assertEqual(results[0].action, "skipped")
        self.assertTrue(os.path.exists(self.src / "a.pdf"))

    def test_undo_does_not_overwrite(self):
        path = self.make("a.txt", b"first")
        result = transfer(path, str(self.dst))
        self.make("a.txt", b"second")
        errors = undo([result])
        self.assertEqual(len(errors), 1)
        self.assertEqual(Path(path).read_bytes(), b"second")
        self.assertTrue(os.path.exists(result.target))


class TestConfig(TempDirCase):

    def test_roundtrip(self):
        path = self.root / "cfg" / "config.json"
        config = AppConfig(portals=[make_sorter_portal("D:/x"), Portal(name="Відео", mode="copy")])
        save_config(config, path)
        loaded = load_config(path)
        self.assertEqual(loaded, config)

    def test_broken_or_old_config(self):
        path = self.root / "config.json"
        self.assertEqual(load_config(path).portals, [])
        path.write_text("{not json", encoding="utf-8")
        self.assertEqual(load_config(path).portals, [])
        path.write_text(json.dumps({"portals": [
            {"name": "A", "mode": "teleport", "unknown_field": 1, "rules": [{"extensions": ["a"], "zzz": 2}]},
            "garbage",
        ]}), encoding="utf-8")
        portals = load_config(path).portals
        self.assertEqual(len(portals), 1)
        self.assertEqual(portals[0].mode, "move")
        self.assertEqual(portals[0].rules[0].extensions, ["a"])


if __name__ == "__main__":
    unittest.main()
