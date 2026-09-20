"""Public install/check contract tests; not evidence of agent routing behavior."""

from __future__ import annotations

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import install_skill as installer


class SkillBundleTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="studio skills ")
        self.addCleanup(self.temporary.cleanup)
        # macOS's temporary root may itself use a /var -> /private/var symlink.
        self.root = Path(self.temporary.name).resolve()
        self.repo = self.root / "trusted source"
        self.repo.mkdir()
        self.project = self.root / "consumer project"
        self.project.mkdir()
        self.skill = self.repo / "skills" / "scientific-figures"
        shutil.copytree(ROOT / "skills" / "scientific-figures", self.skill)
        self.git("init", "-q")
        self.git("config", "user.name", "Fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        self.git("config", "core.autocrlf", "false")
        self.git("config", "core.filemode", "true")
        self.first = self.commit()
        self.target = self.project / ".agents" / "skills" / "scientific-figures"

    def git(self, *args: str) -> str:
        return subprocess.check_output(["git", "-C", str(self.repo), *args], text=True).strip()

    def commit(self) -> str:
        self.git("add", "--all")
        self.git("-c", "commit.gpgsign=false", "commit", "-qm", "fixture")
        return self.git("rev-parse", "HEAD")

    def install(self, revision: str | None = None) -> installer.InstallResult:
        return installer.install(self.repo, revision or self.first, self.project)

    def second(self) -> str:
        (self.skill / "references" / "iteration.md").write_text("Updated reference.\n", encoding="utf-8")
        return self.commit()

    def test_clean_install_pinned_bytes_and_modes(self) -> None:
        self.assertEqual(self.install()["status"], "installed")
        self.assertEqual(installer.check(self.project, self.first)["status"], "clean")
        for path in self.skill.rglob("*"):
            if path.is_file():
                copy = self.target / path.relative_to(self.skill)
                self.assertEqual(path.read_bytes(), copy.read_bytes())
                self.assertEqual(bool(path.stat().st_mode & 0o111), bool(copy.stat().st_mode & 0o111))

    def test_dirty_worktree_does_not_change_pinned_bundle(self) -> None:
        original = (self.skill / "SKILL.md").read_bytes()
        (self.skill / "SKILL.md").write_text("uncommitted", encoding="utf-8")
        self.install()
        self.assertEqual((self.target / "SKILL.md").read_bytes(), original)

    def test_same_pin_is_byte_and_mtime_preserving_noop(self) -> None:
        self.install()
        receipt = self.target / installer.RECEIPT
        before = (receipt.read_bytes(), receipt.stat().st_mtime_ns)
        self.assertEqual(self.install()["status"], "unchanged")
        self.assertEqual(before, (receipt.read_bytes(), receipt.stat().st_mtime_ns))

    def test_upgrade_updates_only_managed_bundle(self) -> None:
        profile = self.project / ".agents" / "figure-project.md"
        profile.parent.mkdir()
        profile.write_text("User-owned profile", encoding="utf-8")
        guide = self.project / "AGENTS.md"
        guide.write_text("User routing", encoding="utf-8")
        self.install()
        second = self.second()
        self.install(second)
        self.assertEqual(installer.check(self.project)["revision"], second)
        self.assertEqual(profile.read_text(), "User-owned profile")
        self.assertEqual(guide.read_text(), "User routing")
        self.assertFalse(list(self.target.parent.glob(".scientific-figures-*")))

    def test_unmanaged_destination_is_never_replaced(self) -> None:
        self.target.mkdir(parents=True)
        own = self.target / "SKILL.md"
        own.write_text("Mine", encoding="utf-8")
        with self.assertRaisesRegex(installer.InstallError, "unmanaged"):
            self.install()
        self.assertEqual(own.read_text(), "Mine")

    def test_modified_file_refuses_update(self) -> None:
        self.install()
        own = self.target / "SKILL.md"
        own.write_text("Mine", encoding="utf-8")
        with self.assertRaisesRegex(installer.InstallError, "drift"):
            self.install(self.second())
        self.assertEqual(own.read_text(), "Mine")

    def test_missing_file_is_drift(self) -> None:
        self.install()
        (self.target / "NOTICE").unlink()
        with self.assertRaisesRegex(installer.InstallError, "drift"):
            installer.check(self.project)

    def test_untracked_file_refuses_update(self) -> None:
        self.install()
        (self.target / "notes.md").write_text("Human notes", encoding="utf-8")
        with self.assertRaisesRegex(installer.InstallError, "drift"):
            self.install(self.second())
        self.assertEqual((self.target / "notes.md").read_text(), "Human notes")

    def test_untracked_empty_directory_is_preserved(self) -> None:
        self.install()
        own = self.target / "my-experiments"
        own.mkdir()
        with self.assertRaisesRegex(installer.InstallError, "untracked directory"):
            self.install(self.second())
        self.assertTrue(own.is_dir())

    def test_executable_mode_change_is_drift(self) -> None:
        self.install()
        (self.target / "NOTICE").chmod(0o755)
        with self.assertRaisesRegex(installer.InstallError, "drift"):
            installer.check(self.project)

    def test_pin_requires_commit_not_branch_tag_or_abbreviation(self) -> None:
        for invalid in ("HEAD", "main", self.first[:10], "G" * 40):
            with self.subTest(invalid=invalid), self.assertRaises(installer.InstallError):
                self.install(invalid)
        self.assertFalse(self.target.exists())

    def test_check_detects_expected_revision_drift(self) -> None:
        self.install()
        with self.assertRaisesRegex(installer.InstallError, "revision drift"):
            installer.check(self.project, self.second())

    def test_incomplete_source_bundle_is_rejected(self) -> None:
        (self.skill / "NOTICE").unlink()
        with self.assertRaisesRegex(installer.InstallError, "complete"):
            self.install(self.commit())
        self.assertFalse(self.target.exists())

    def test_source_symlink_is_rejected(self) -> None:
        (self.skill / "link.md").symlink_to("SKILL.md")
        with self.assertRaisesRegex(installer.InstallError, "unsupported"):
            self.install(self.commit())
        self.assertFalse(self.target.exists())

    def test_destination_symlink_cannot_redirect_install(self) -> None:
        outside = self.root / "outside"
        outside.mkdir()
        (self.project / ".agents").symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(installer.InstallError, "symlink"):
            self.install()
        self.assertEqual(list(outside.iterdir()), [])

    def test_installed_symlink_is_rejected_without_following(self) -> None:
        self.install()
        notice = self.target / "NOTICE"
        notice.unlink()
        notice.symlink_to(self.skill / "NOTICE")
        with self.assertRaisesRegex(installer.InstallError, "symlink"):
            installer.check(self.project)

    def test_invalid_receipt_path_does_not_touch_outside_files(self) -> None:
        self.install()
        receipt = self.target / installer.RECEIPT
        original = json.loads(receipt.read_text())
        for name in ("../escape", "/absolute", ".", "a/../b", "a\\b", "bad\nname", installer.RECEIPT):
            with self.subTest(name=name):
                value = dict(original)
                value["files"] = dict(original["files"])
                value["files"][name] = original["files"]["NOTICE"]
                receipt.write_text(json.dumps(value), encoding="utf-8")
                with self.assertRaisesRegex(installer.InstallError, "invalid bundle path"):
                    installer.check(self.project)
        self.assertFalse((self.target.parent / "escape").exists())

    def test_invalid_receipt_revision_has_clear_failure(self) -> None:
        self.install()
        receipt = self.target / installer.RECEIPT
        value = json.loads(receipt.read_text())
        value["revision"] = None
        receipt.write_text(json.dumps(value), encoding="utf-8")
        with self.assertRaisesRegex(installer.InstallError, "revision must"):
            installer.check(self.project)

    def test_size_limits_preserve_prior_install(self) -> None:
        self.install()
        (self.skill / "oversized.txt").write_bytes(b"x" * (installer.MAX_FILE + 1))
        with self.assertRaisesRegex(installer.InstallError, "limit"):
            self.install(self.commit())
        self.assertEqual(installer.check(self.project)["revision"], self.first)

    def test_exclusive_installer_lock_refuses_concurrent_update(self) -> None:
        self.install()
        lock = self.target.parent / ".scientific-figures-install.lock"
        lock.write_text("owned", encoding="utf-8")
        with self.assertRaisesRegex(installer.InstallError, "lock exists"):
            self.install(self.second())
        self.assertEqual(lock.read_text(), "owned")
        self.assertEqual(installer.check(self.project)["revision"], self.first)

    def test_failed_replacement_restores_old_install_and_allows_retry(self) -> None:
        self.install()
        second = self.second()
        real_replace = os.replace

        def fail_candidate(source: Path, target: Path) -> None:
            if Path(source).name == "candidate":
                raise OSError("injected replacement failure")
            real_replace(source, target)

        with patch.object(installer.os, "replace", side_effect=fail_candidate):
            with self.assertRaisesRegex(OSError, "injected"):
                self.install(second)
        self.assertEqual(installer.check(self.project)["revision"], self.first)
        self.assertFalse(list(self.target.parent.glob(".scientific-figures-*")))
        self.assertEqual(self.install(second)["status"], "installed")

    def test_failed_rollback_retains_recovery_copy(self) -> None:
        self.install()
        second = self.second()
        real_replace = os.replace

        def fail_after_backup(source: Path, target: Path) -> None:
            if Path(source).name in ("candidate", "previous"):
                raise OSError("injected rollback failure")
            real_replace(source, target)

        with patch.object(installer.os, "replace", side_effect=fail_after_backup):
            with self.assertRaisesRegex(installer.InstallError, "recovery needed"):
                self.install(second)
        backups = list(self.target.parent.glob(".scientific-figures-*/previous"))
        self.assertEqual(len(backups), 1)
        self.assertEqual((backups[0] / "SKILL.md").read_bytes(), (self.skill / "SKILL.md").read_bytes())

    def test_cli_install_and_check_from_another_directory(self) -> None:
        command = [sys.executable, str(ROOT / "scripts" / "install_skill.py")]
        installed = subprocess.check_output(command + ["install", "--source", str(self.repo), "--revision", self.first, "--project", str(self.project)], cwd=self.root, text=True)
        self.assertEqual(json.loads(installed)["status"], "installed")
        checked = subprocess.check_output(command + ["check", "--project", str(self.project), "--revision", self.first], cwd=self.root, text=True)
        self.assertEqual(json.loads(checked)["status"], "clean")
        failed = subprocess.run(command + ["check", "--project", str(self.project), "--revision", "main"], capture_output=True, text=True, check=False)
        self.assertEqual(failed.returncode, 2)
        self.assertIn("revision must", failed.stderr)

    def test_installed_references_are_self_contained(self) -> None:
        self.install()
        for document in self.target.rglob("*.md"):
            for target in re.findall(r"\]\(([^)]+)\)", document.read_text(encoding="utf-8")):
                if "://" in target or target.startswith("#"):
                    continue
                resolved = (document.parent / target.split("#", 1)[0]).resolve()
                with self.subTest(document=document.name, link=target):
                    self.assertTrue(resolved.is_relative_to(self.target))
                    self.assertTrue(resolved.is_file())
        skill = (self.target / "SKILL.md").read_text(encoding="utf-8")
        self.assertTrue(skill.startswith("---\nname: scientific-figures\n"))
        self.assertIn("\ndescription:", skill.split("\n---\n", 1)[0])
        self.assertLess(len(skill.splitlines()), 140)
        scenarios = json.loads((self.target / "evals" / "scenarios.json").read_text())
        self.assertEqual(len(scenarios), 6)
        self.assertEqual(len({case["id"] for case in scenarios}), 6)
        # These are evaluation inputs, never fabricated successful agent runs.
        self.assertTrue(all("passed" not in case for case in scenarios))


if __name__ == "__main__":
    unittest.main()
