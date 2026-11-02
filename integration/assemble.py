"""Assemble pristine upstream and owned source overlays without editing either."""

from __future__ import annotations

import argparse
from contextlib import contextmanager, nullcontext
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tempfile
import time
import tomllib

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_DEST = ROOT / "target/integration/workspace"
STATE = ".gpui-assembly.json"


class AssemblyError(RuntimeError):
    pass


def git(root: Path, *args: str, data: bytes | None = None) -> bytes:
    result = subprocess.run(
        ["git", *args], cwd=root, input=data, capture_output=True, check=False
    )
    if result.returncode:
        raise AssemblyError(result.stderr.decode(errors="replace").strip())
    return result.stdout


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_source(path: Path) -> bytes:
    return os.readlink(path).encode() if path.is_symlink() else path.read_bytes()


def source_files(directory: Path, *, exclude: frozenset[str] = frozenset()):
    if directory.is_symlink():
        raise AssemblyError(f"Source directory must not be a symlink: {directory}")
    for parent, directories, files in os.walk(directory, followlinks=False):
        if Path(parent) == directory:
            directories[:] = [name for name in directories if name not in exclude]
        for name in directories:
            if (Path(parent) / name).is_symlink():
                raise AssemblyError(f"Source directory must not be a symlink: {parent}/{name}")
        for name in sorted(files):
            yield Path(parent) / name


def relative_path(value: str) -> Path:
    path = PurePosixPath(value)
    if not value or path.is_absolute() or ".." in path.parts or "\\" in value or ":" in value:
        raise AssemblyError(f"Expected a repository-relative path: {value!r}")
    return Path(*path.parts)


def upstream_files(root: Path, config: dict) -> dict[str, tuple[str, str]]:
    entries = git(root, "ls-tree", "-r", "-z", config["import_commit"], "--", *config["tracked"])
    result = {}
    for entry in entries.split(b"\0"):
        if not entry:
            continue
        meta, path = entry.split(b"\t", 1)
        mode, kind, oid = meta.split()
        if kind != b"blob":
            raise AssemblyError(f"Unsupported upstream entry: {path.decode()}")
        result[path.decode()] = (mode.decode(), oid.decode())
    if not result:
        raise AssemblyError("The recorded upstream import contains no tracked files")
    return result


def verify_pristine(root: Path = ROOT) -> dict[str, tuple[str, str]]:
    config = tomllib.loads((root / "UPSTREAM").read_text())
    expected = upstream_files(root, config)
    actual = {
        file.relative_to(root).as_posix(): file
        for directory in config["tracked"]
        for file in source_files(root / relative_path(directory))
    }
    problems = [f"added: {path}" for path in sorted(actual.keys() - expected.keys())]
    problems += [f"missing: {path}" for path in sorted(expected.keys() - actual.keys())]
    for path in sorted(expected.keys() & actual.keys()):
        data = read_source(actual[path])
        blob = hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()
        if blob != expected[path][1]:
            problems.append(f"modified: {path}")
        elif os.name != "nt" and expected[path][0] != "120000":
            executable = bool(actual[path].stat().st_mode & 0o111)
            if executable != (expected[path][0] == "100755"):
                problems.append(f"executable mode changed: {path}")
    if problems:
        raise AssemblyError(
            "Upstream sources must match UPSTREAM.import_commit exactly:\n  "
            + "\n  ".join(problems)
            + "\nPut owned changes in fast/, extensions/ or integration/patches/."
        )
    return expected


def write_if_changed(source: Path, target: Path):
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.is_symlink():
        target.unlink()
    if not target.exists() or source.read_bytes() != target.read_bytes():
        shutil.copy2(source, target)
    elif os.name != "nt" and (source.stat().st_mode & 0o777) != (target.stat().st_mode & 0o777):
        shutil.copymode(source, target)


def copy_tree(source: Path, target: Path):
    for file in source_files(source):
        destination = target / file.relative_to(source)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(file, destination)


@contextmanager
def assembly_lock(root: Path = ROOT):
    directory = root / "target/integration"
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / "assembly.lock"
    deadline = time.monotonic() + 30
    while True:
        try:
            descriptor = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o644)
            break
        except FileExistsError:
            if time.monotonic() >= deadline:
                raise AssemblyError(f"Assembly is busy; if its process exited, remove {path}")
            time.sleep(0.1)
    try:
        with os.fdopen(descriptor, "w") as lock:
            lock.write(str(os.getpid()))
        yield
    finally:
        path.unlink()


def check_generated(dest: Path):
    state = dest / STATE
    if not state.exists():
        if dest.exists() and any(dest.iterdir()):
            raise AssemblyError(f"Refusing to replace a directory not created by the assembler: {dest}")
        return
    metadata = json.loads(state.read_text())
    changed = []
    expected = set(metadata["files"])
    for name, fingerprint in metadata["files"].items():
        if name == "Cargo.lock":
            continue
        file = dest / relative_path(name)
        if not file.is_file() or file.is_symlink() or digest(file.read_bytes()) != fingerprint:
            changed.append(name)
    for file in source_files(dest, exclude=frozenset({"target"})):
        name = file.relative_to(dest).as_posix()
        if name not in expected and name != STATE:
            changed.append(name)
    if changed:
        raise AssemblyError(
            "Generated sources were edited; reassembly would discard these changes:\n  "
            + "\n  ".join(sorted(set(changed)))
            + "\nMove edits to their owned sources or integration patches. "
            "Use --discard-generated-changes only after saving them."
        )


def apply_patch(staging: Path, patch: Path):
    environment = dict(os.environ, GIT_CEILING_DIRECTORIES=str(staging.parent))
    for flag in ["--check", None]:
        command = ["git", "apply", "--whitespace=nowarn"]
        if flag:
            command.append(flag)
        result = subprocess.run(
            [*command, str(patch)], cwd=staging, env=environment, capture_output=True
        )
        if result.returncode:
            raise AssemblyError(
                f"Integration patch failed: {patch}\n"
                + result.stderr.decode(errors="replace")
                + "The previous assembled workspace was left intact."
            )


def assemble(
    root: Path = ROOT,
    dest: Path | None = None,
    *,
    native_gpu: bool = True,
    discard_generated_changes: bool = False,
    lock: bool = True,
) -> Path:
    dest = dest or root / "target/integration/workspace"
    if not dest.resolve().is_relative_to((root / "target/integration").resolve()):
        raise AssemblyError("Assembly destinations must be inside target/integration/")
    with assembly_lock(root) if lock else nullcontext():
        dest.parent.mkdir(parents=True, exist_ok=True)
        expected = verify_pristine(root)
        if not discard_generated_changes:
            check_generated(dest)
        elif dest.exists() and not (dest / STATE).is_file():
            raise AssemblyError(f"Refusing to replace an unrecognized directory: {dest}")
        plan = json.loads((root / "integration/overlays.json").read_text())
        if plan["version"] != 1:
            raise AssemblyError("Unsupported overlay manifest version")
        layers = {"base", "native-gpu"} if native_gpu else {"base"}
        overlays = []
        for entry in plan["overlays"]:
            if entry["layer"] not in layers:
                continue
            source, target = relative_path(entry["source"]), relative_path(entry["target"])
            directory = root / source
            if directory.is_dir():
                overlays.extend(
                    {"source": (source / file.relative_to(directory)).as_posix(),
                     "target": (target / file.relative_to(directory)).as_posix()}
                    for file in source_files(directory)
                )
            else:
                overlays.append(entry)
        targets = set()
        for entry in overlays:
            source = relative_path(entry["source"])
            target = relative_path(entry["target"])
            if source.parts[0] not in {"fast", "extensions"}:
                raise AssemblyError(f"Overlay source must be owned: {source}")
            if target.parts[0] != "crates" or target.as_posix() in expected:
                raise AssemblyError(f"An overlay cannot replace upstream source: {target}")
            if target in targets:
                raise AssemblyError(f"Duplicate overlay target: {target}")
            targets.add(target)
            if not (root / source).is_file() or (root / source).is_symlink():
                raise AssemblyError(f"Missing or symlinked overlay source: {source}")
        with tempfile.TemporaryDirectory(prefix="assemble-", dir=dest.parent) as directory:
            staging = Path(directory)
            for name, (mode, _) in expected.items():
                source = root / name
                # Materialize upstream license symlinks on Windows as well.
                if mode == "120000":
                    source = source.parent / read_source(source).decode()
                target = staging / name
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)
            for name in ["apps", "compat", "extensions/cad", "assets"]:
                copy_tree(root / name, staging / name)
            cargo_config = staging / ".cargo/config.toml"
            cargo_config.parent.mkdir()
            target_dir = Path(os.path.relpath(root / "target", dest)).as_posix()
            cargo_config.write_text(f"[build]\ntarget-dir = {json.dumps(target_dir)}\n", newline="\n")
            for name in ["Cargo.toml", "Cargo.lock"]:
                shutil.copy2(root / "integration/workspace" / name, staging / name)
            if not native_gpu:
                manifest = staging / "Cargo.toml"
                manifest.write_text(manifest.read_text().replace('    "extensions/cad",\n', ""), newline="\n")
            for name in ["LICENSE", "LICENSE-APACHE", "README.md", "clippy.toml", "rust-toolchain.toml"]:
                shutil.copy2(root / name, staging / name)
            for entry in overlays:
                source, target = root / entry["source"], staging / entry["target"]
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)
            for entry in plan["patches"]:
                if entry["layer"] in layers:
                    apply_patch(staging, root / relative_path(entry["path"]))
            files = {
                file.relative_to(staging).as_posix(): digest(file.read_bytes())
                for file in source_files(staging)
            }
            # Synchronize only after every patch passes, preserving mtimes for Cargo.
            dest.mkdir(parents=True, exist_ok=True)
            for file in list(source_files(dest, exclude=frozenset({"target"}))):
                name = file.relative_to(dest).as_posix()
                if name not in files and name != STATE:
                    file.unlink()
            for file in source_files(staging):
                write_if_changed(file, dest / file.relative_to(staging))
            metadata = {"version": 1, "native_gpu": native_gpu, "files": files}
            state = dest / STATE
            serialized = json.dumps(metadata, indent=2, sort_keys=True) + "\n"
            if not state.exists() or state.read_text() != serialized:
                state.write_text(serialized, newline="\n")
    return dest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--without-native-gpu", action="store_true", help="assemble performance/composition only")
    parser.add_argument("--dest", help="alternate workspace path inside target/integration/")
    parser.add_argument("--discard-generated-changes", action="store_true", help="replace edits to generated files")
    args = parser.parse_args()
    try:
        dest = assemble(
            dest=ROOT / args.dest if args.dest else None,
            native_gpu=not args.without_native_gpu,
            discard_generated_changes=args.discard_generated_changes,
        )
        print(f"Assembled workspace: {dest}")
    except (AssemblyError, OSError, ValueError) as error:
        print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
