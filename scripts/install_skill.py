"""Install/check the scientific-figures bundle from an explicit local Git commit.

Only .agents/skills/scientific-figures is managed. No network, global agent
configuration or project profile is changed. Pause other writers during updates;
the lock coordinates this installer, not arbitrary filesystem editors.
"""

from __future__ import annotations

import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import subprocess
import tempfile
from typing import Iterator, Literal, TypedDict, cast

NAME = "scientific-figures"
PREFIX = f"skills/{NAME}/"
RECEIPT = ".cetz-studio-install.json"
MAX_FILE = 512 * 1024
MAX_TOTAL = 8 * 1024 * 1024
MAX_FILES = 256


class FileSignature(TypedDict):
    sha256: str
    mode: str


class Receipt(TypedDict):
    format: int
    name: str
    revision: str
    files: dict[str, FileSignature]


class InstallResult(TypedDict):
    status: Literal["clean", "unchanged", "installed"]
    revision: str
    files: int


class InstallError(ValueError):
    """The requested install/check cannot preserve its contract."""


def _pin(value: object) -> str:
    if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{40}", value):
        raise InstallError("revision must be a full, lowercase 40-character Git commit SHA")
    return value


def _relative(value: object) -> str:
    if not isinstance(value, str) or value == ".":
        raise InstallError(f"invalid bundle path: {value!r}")
    path = PurePosixPath(value)
    if (not value or path.is_absolute() or path.as_posix() != value
            or any(part in (".", "..") for part in path.parts)
            or "\\" in value or any(ord(c) < 32 for c in value) or value == RECEIPT):
        raise InstallError(f"invalid bundle path: {value!r}")
    return value


def _git(repo: Path, *args: str) -> bytes:
    result = subprocess.run(
        ["git", "-C", str(repo), *args], check=True, capture_output=True, timeout=30
    )
    return result.stdout


def _bundle(repo: Path, revision: str) -> dict[str, tuple[bytes, str]]:
    _pin(revision)
    actual = _git(repo, "rev-parse", "--verify", revision + "^{commit}").decode().strip()
    if actual != revision:
        raise InstallError("revision does not identify the requested commit")
    entries = _git(repo, "ls-tree", "-rz", revision, "--", PREFIX).split(b"\0")
    files: dict[str, tuple[bytes, str]] = {}
    total = 0
    for entry in filter(None, entries):
        header, raw_path = entry.split(b"\t", 1)
        mode, kind, oid = header.decode().split()
        path = raw_path.decode("utf-8")
        if not path.startswith(PREFIX) or kind != "blob" or mode not in ("100644", "100755"):
            raise InstallError(f"bundle contains unsupported entry: {path}")
        relative = _relative(path[len(PREFIX):])
        size = int(_git(repo, "cat-file", "-s", oid))
        total += size
        if size > MAX_FILE or total > MAX_TOTAL or len(files) >= MAX_FILES:
            raise InstallError("bundle exceeds the file/count/total size limit")
        data = _git(repo, "cat-file", "blob", oid)
        if len(data) != size or relative in files:
            raise InstallError("inconsistent or duplicate Git bundle entry")
        files[relative] = (data, mode)
    if not {"SKILL.md", "LICENSE", "NOTICE"}.issubset(files):
        raise InstallError("commit has no complete scientific-figures bundle")
    return files


def _no_symlinks(path: Path) -> None:
    for part in (path, *path.parents):
        if part.is_symlink():
            raise InstallError(f"symlink path is not supported: {part}")


def _target(project: Path) -> Path:
    project = Path(os.path.abspath(project.expanduser()))
    _no_symlinks(project)
    if not project.is_dir():
        raise InstallError("project must be an existing directory")
    target = project / ".agents" / "skills" / NAME
    _no_symlinks(target)
    return target


def _signature(data: bytes, mode: str) -> FileSignature:
    return {"sha256": hashlib.sha256(data).hexdigest(), "mode": mode}


def _verified_receipt(target: Path) -> Receipt:
    _no_symlinks(target)
    receipt_path = target / RECEIPT
    if not receipt_path.is_file() or receipt_path.is_symlink():
        raise InstallError("existing skill is unmanaged; refusing to replace user-owned files")
    if receipt_path.stat().st_size > MAX_FILE:
        raise InstallError("install receipt is too large")
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    if not isinstance(receipt, dict) or receipt.get("format") != 1 or receipt.get("name") != NAME:
        raise InstallError("invalid install receipt")
    _pin(receipt.get("revision", ""))
    expected = receipt.get("files")
    if not isinstance(expected, dict) or not 0 < len(expected) <= MAX_FILES:
        raise InstallError("invalid receipt file map")
    if not {"SKILL.md", "LICENSE", "NOTICE"}.issubset(expected):
        raise InstallError("incomplete receipt file map")
    allowed_dirs: set[str] = set()
    for name, signature in expected.items():
        _relative(name)
        if (not isinstance(signature, dict) or set(signature) != {"sha256", "mode"}
                or not isinstance(signature["sha256"], str)
                or not re.fullmatch(r"[0-9a-f]{64}", signature["sha256"])
                or signature["mode"] not in ("100644", "100755")):
            raise InstallError("invalid receipt file signature")
        allowed_dirs.update(str(p) for p in PurePosixPath(name).parents if str(p) != ".")
    actual: dict[str, FileSignature] = {}
    total = 0
    for path in target.rglob("*"):
        relative = path.relative_to(target).as_posix()
        info = path.lstat()
        if stat.S_ISLNK(info.st_mode):
            raise InstallError(f"symlink in installed bundle: {relative}")
        if stat.S_ISDIR(info.st_mode):
            if relative not in allowed_dirs:
                raise InstallError(f"untracked directory in installed bundle: {relative}")
            continue
        if not stat.S_ISREG(info.st_mode):
            raise InstallError(f"non-regular installed file: {relative}")
        if relative == RECEIPT:
            continue
        total += info.st_size
        if info.st_size > MAX_FILE or total > MAX_TOTAL or len(actual) >= MAX_FILES:
            raise InstallError("installed bundle exceeds size limits")
        mode = "100755" if info.st_mode & 0o111 else "100644"
        actual[relative] = _signature(path.read_bytes(), mode)
    if actual != expected:
        changed = sorted(set(actual) ^ set(expected) | {
            name for name in set(actual) & set(expected) if actual[name] != expected[name]
        })
        raise InstallError("installed bundle drift: " + ", ".join(changed))
    return cast(Receipt, receipt)


@contextmanager
def _lock(parent: Path) -> Iterator[None]:
    path = parent / f".{NAME}-install.lock"
    try:
        descriptor = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    except FileExistsError as error:
        raise InstallError(f"installer lock exists; inspect the owning process before removing {path}") from error
    os.close(descriptor)
    try:
        yield
    finally:
        path.unlink()


def check(project: Path, revision: str | None = None) -> InstallResult:
    """Verify managed bytes/modes and optionally the consumer's expected pin."""
    receipt = _verified_receipt(_target(project))
    if revision is not None and receipt["revision"] != _pin(revision):
        raise InstallError(f"revision drift: installed {receipt['revision']}, expected {revision}")
    return {"status": "clean", "revision": receipt["revision"], "files": len(receipt["files"])}


def install(source: Path, revision: str, project: Path) -> InstallResult:
    """Stage one pinned bundle; refuse unmanaged or modified destinations."""
    files = _bundle(source, revision)
    target = _target(project)
    target.parent.mkdir(parents=True, exist_ok=True)
    _no_symlinks(target.parent)
    receipt: Receipt = {
        "format": 1, "name": NAME, "revision": revision,
        "files": {name: _signature(data, mode) for name, (data, mode) in files.items()},
    }
    with _lock(target.parent):
        old = _verified_receipt(target) if target.exists() else None
        if old == receipt:
            return {"status": "unchanged", "revision": revision, "files": len(files)}
        workspace = Path(tempfile.mkdtemp(prefix=f".{NAME}-", dir=target.parent))
        stage = workspace / "candidate"
        backup = workspace / "previous"
        stage.mkdir()
        try:
            for relative, (data, mode) in files.items():
                path = stage / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)
                path.chmod(0o755 if mode == "100755" else 0o644)
            (stage / RECEIPT).write_text(json.dumps(receipt, sort_keys=True, indent=2) + "\n", encoding="utf-8")
            _verified_receipt(stage)
            # Cooperating installers hold the lock. Other writers must be paused.
            _no_symlinks(target)
            if old is not None:
                if _verified_receipt(target) != old:
                    raise InstallError("installed bundle changed while staging")
                os.replace(target, backup)
            elif target.exists():
                raise InstallError("destination appeared while staging")
            try:
                os.replace(stage, target)
            except OSError:
                if backup.exists():
                    if target.exists() or target.is_symlink():
                        raise InstallError(f"recovery needed; previous bundle retained at {backup}")
                    try:
                        os.replace(backup, target)
                    except OSError as error:
                        raise InstallError(f"recovery needed; previous bundle retained at {backup}") from error
                raise
            if backup.exists():
                shutil.rmtree(backup)
        finally:
            # A failed rollback keeps the previous version reachable for recovery.
            if not backup.exists():
                shutil.rmtree(workspace)
    check(project, revision)
    return {"status": "installed", "revision": revision, "files": len(files)}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    add = commands.add_parser("install")
    add.add_argument("--source", required=True, type=Path, help="Trusted local CeTZ Studio Git checkout")
    add.add_argument("--revision", required=True, help="Full commit SHA, not a branch or tag")
    add.add_argument("--project", required=True, type=Path)
    verify = commands.add_parser("check")
    verify.add_argument("--project", required=True, type=Path)
    verify.add_argument("--revision")
    args = parser.parse_args()
    try:
        result = (install(args.source, args.revision, args.project) if args.command == "install"
                  else check(args.project, args.revision))
    except (OSError, ValueError, TypeError, subprocess.SubprocessError) as error:
        parser.exit(2, f"error: {error}\n")
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
