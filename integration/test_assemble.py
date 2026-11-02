"""Exercise overlay composition and failure isolation using a real Git import."""
from pathlib import Path
import difflib
import json
import subprocess
import tempfile
import unittest

from assemble import AssemblyError, assemble, verify_pristine


class AssemblyTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="gpui-assembly-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.write("crates/demo/src/lib.rs", "pub fn upstream() {}\n")
        self.git("init", "-q")
        self.git("add", "crates")
        self.git("-c", "user.name=Overlay Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "import")
        commit = self.git("rev-parse", "HEAD").strip()
        self.write("UPSTREAM", f'import_commit = "{commit}"\ntracked = ["crates/demo"]\n')
        for directory in ["apps", "compat", "extensions/cad", "assets", ".cargo"]:
            (self.root / directory).mkdir(parents=True, exist_ok=True)
        for name in ["LICENSE", "LICENSE-APACHE", "README.md", "clippy.toml", "rust-toolchain.toml"]:
            self.write(name, "fixture\n")
        self.write("integration/workspace/Cargo.toml", '[workspace]\nmembers = [\n    "extensions/cad",\n]\n')
        self.write("integration/workspace/Cargo.lock", "lock fixture\n")
        self.write("fast/demo/src/module.rs", "// performance source\n")
        self.write("extensions/native-gpu/demo/src/draw.rs", "// native drawing\n")
        self.base = "pub fn upstream() {}\nmod fast;\n"
        self.native = self.base + "mod extensions;\n"
        self.patch("base", "crates/demo/src/lib.rs", "pub fn upstream() {}\n", self.base)
        self.patch("native-gpu", "crates/demo/src/lib.rs", self.base, self.native)
        self.plan = {"version": 1, "patches": [
            {"layer": layer, "path": f"integration/patches/{layer}.patch"}
            for layer in ["base", "native-gpu"]
        ], "overlays": [
            {"layer": "base", "source": "fast/demo/src", "target": "crates/demo/src/fast"},
            {"layer": "native-gpu", "source": "extensions/native-gpu/demo/src", "target": "crates/demo/src/extensions"},
        ]}
        self.write_plan()

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, stderr=subprocess.PIPE).decode()

    def write(self, path, text):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, newline="\n")
        return target

    def patch(self, layer, path, before, after):
        patch = "".join(difflib.unified_diff(
            before.splitlines(keepends=True), after.splitlines(keepends=True),
            fromfile=f"a/{path}", tofile=f"b/{path}",
        ))
        self.write(f"integration/patches/{layer}.patch", patch)

    def write_plan(self):
        self.write("integration/overlays.json", json.dumps(self.plan))

    def test_layers_compose_without_modifying_upstream_and_retain_mtimes(self):
        dest = assemble(self.root)
        file = dest / "crates/demo/src/lib.rs"
        self.assertEqual(file.read_text(), self.native)
        self.assertEqual((self.root / "crates/demo/src/lib.rs").read_text(), "pub fn upstream() {}\n")
        self.assertTrue((dest / "crates/demo/src/extensions/draw.rs").exists())
        modified = file.stat().st_mtime_ns
        assemble(self.root)
        self.assertEqual(file.stat().st_mtime_ns, modified)
        verify_pristine(self.root)

    def test_without_native_gpu_removes_native_sources_and_workspace_member(self):
        dest = assemble(self.root)
        assemble(self.root, native_gpu=False)
        self.assertEqual((dest / "crates/demo/src/lib.rs").read_text(), self.base)
        self.assertFalse((dest / "crates/demo/src/extensions/draw.rs").exists())
        self.assertNotIn('"extensions/cad"', (dest / "Cargo.toml").read_text())

    def test_upstream_edits_and_new_files_are_rejected(self):
        self.write("crates/demo/src/extra.rs", "// accidental upstream edit\n")
        with self.assertRaisesRegex(AssemblyError, "added: crates/demo/src/extra.rs"):
            assemble(self.root)
        (self.root / "crates/demo/src/extra.rs").unlink()
        self.write("crates/demo/src/lib.rs", "pub fn changed() {}\n")
        with self.assertRaisesRegex(AssemblyError, "modified: crates/demo/src/lib.rs"):
            assemble(self.root)

    def test_failed_patch_preserves_the_previous_workspace(self):
        dest = assemble(self.root)
        self.patch("native-gpu", "crates/demo/src/lib.rs", "missing context\n", self.native)
        with self.assertRaisesRegex(AssemblyError, "Integration patch failed"):
            assemble(self.root)
        self.assertEqual((dest / "crates/demo/src/lib.rs").read_text(), self.native)

    def test_generated_edits_require_explicit_discard(self):
        dest = assemble(self.root)
        file = dest / "crates/demo/src/lib.rs"
        file.write_text("// save my work\n")
        with self.assertRaisesRegex(AssemblyError, "Generated sources were edited"):
            assemble(self.root)
        self.assertEqual(file.read_text(), "// save my work\n")
        assemble(self.root, discard_generated_changes=True)
        self.assertEqual(file.read_text(), self.native)

    def test_cargo_build_artifacts_are_preserved_without_becoming_owned_sources(self):
        dest = assemble(self.root)
        artifact = dest / "target/debug/build-output"
        artifact.parent.mkdir(parents=True)
        artifact.write_bytes(b"compiled artifact")
        assemble(self.root)
        self.assertEqual(artifact.read_bytes(), b"compiled artifact")

    def test_new_owned_files_are_copied_and_removed_owned_files_are_pruned(self):
        dest = assemble(self.root)
        source = self.write("fast/demo/src/new.rs", "// new topic\n")
        assemble(self.root)
        target = dest / "crates/demo/src/fast/new.rs"
        self.assertTrue(target.exists())
        source.unlink()
        assemble(self.root)
        self.assertFalse(target.exists())

    def test_overlay_cannot_replace_an_upstream_file(self):
        self.plan["overlays"][0] = {
            "layer": "base", "source": "fast/demo/src/module.rs", "target": "crates/demo/src/lib.rs",
        }
        self.write_plan()
        with self.assertRaisesRegex(AssemblyError, "cannot replace upstream"):
            assemble(self.root)

    def test_destinations_and_overlay_paths_cannot_escape(self):
        with self.assertRaisesRegex(AssemblyError, "destinations must be inside"):
            assemble(self.root, self.root / "crates")
        self.plan["overlays"][0]["target"] = "../outside.rs"
        self.write_plan()
        with self.assertRaisesRegex(AssemblyError, "repository-relative path"):
            assemble(self.root)


if __name__ == "__main__":
    unittest.main()
