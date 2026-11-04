"""Reject packages with the wrong application, device scope, or native binary."""

import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import project


class PackageVerificationTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.addCleanup(patch.stopall)
        patch.object(project, "ROOT", self.root).start()
        (self.root / "config").mkdir()
        self.revision = "a" * 40
        self.app = {"id": "editor", "status": "integrated", "nativeCrate": "editor-ability",
                    "upstreamName": "ExampleCraft", "upstreamCrates": ["example-engine"],
                    "sourceRepository": "https://github.com/harmony-contrib/example.git"}
        (self.root / "config/apps.json").write_text(json.dumps({"apps": [self.app, {
            "id": "launcher", "status": "integrated"}, {"id": "future", "status": "planned"}]}))
        (self.root / "Cargo.toml").write_text(
            '[workspace.dependencies]\nexample-engine = { git = "' + self.app["sourceRepository"]
            + '", rev = "' + self.revision + '" }\n')
        (self.root / "Cargo.lock").write_text("version = 4\n")
        self.runner = project.Project("debug", 2)
        self.library = b"\x7fELF\x02\x01" + bytes(12) + (183).to_bytes(2, "little")
        library_path = self.root / "target/native/debug/arm64-v8a/libeditor_ability.so"
        library_path.parent.mkdir(parents=True)
        library_path.write_bytes(self.library)
        library_path.with_suffix(".json").write_text(json.dumps({
            **self.runner.native_provenance("editor"), "sha256": hashlib.sha256(self.library).hexdigest()}))

    def package(self, *, product="editor", devices=None, libraries=None, bundle=None,
                launcher_action="ohos.want.action.home", licenses=True):
        hap = self.root / "test.hap"
        manifest = {"app": {"bundleName": bundle or "com.richerfu." + product},
                    "module": {"name": product, "deviceTypes": devices or ["tablet", "2in1"],
                               "abilities": [{"name": "EntryAbility", "exported": True,
                                              "skills": [{"actions": [launcher_action]}]}]}}
        if libraries is None:
            libraries = {"libs/arm64-v8a/libeditor_ability.so": self.library}
        with zipfile.ZipFile(hap, "w") as archive:
            archive.writestr("module.json", json.dumps(manifest))
            if licenses:
                for name in ("LICENSE-MIT", "LICENSE-APACHE", "NOTICE", "upstream/LICENSE-MIT"):
                    archive.writestr("resources/rawfile/zaoyu-licenses/" + name, "License fixture")
            for name, data in libraries.items():
                archive.writestr(name, data)
        return hap

    def test_verified_package_records_its_own_source(self):
        report = self.runner.verify("editor", self.package())
        self.assertEqual(report["sourceRevision"], self.revision)
        self.assertEqual(report["sourceRepository"], self.app["sourceRepository"])
        self.assertEqual(self.runner.products, ("editor", "launcher"))

    def test_rejects_wrong_bundle(self):
        with self.assertRaisesRegex(RuntimeError, "identity mismatch"):
            self.runner.verify("editor", self.package(bundle="com.richerfu.other"))

    def test_rejects_phone_support(self):
        with self.assertRaisesRegex(RuntimeError, "device scope"):
            self.runner.verify("editor", self.package(devices=["phone", "tablet", "2in1"]))

    def test_rejects_missing_desktop_launcher(self):
        with self.assertRaisesRegex(RuntimeError, "desktop launcher"):
            self.runner.verify("editor", self.package(launcher_action="action.system.home"))

    def test_rejects_missing_licenses(self):
        with self.assertRaisesRegex(RuntimeError, "license notice"):
            self.runner.verify("editor", self.package(licenses=False))

    def test_rejects_another_editors_native_library(self):
        libraries = {"libs/arm64-v8a/libeditor_ability.so": self.library,
                     "libs/arm64-v8a/libother_ability.so": self.library}
        with self.assertRaisesRegex(RuntimeError, "Unexpected native editor"):
            self.runner.verify("editor", self.package(libraries=libraries))

    def test_rejects_stale_native_build(self):
        with self.assertRaisesRegex(RuntimeError, "differs from the current build"):
            self.runner.verify("editor", self.package(libraries={
                "libs/arm64-v8a/libeditor_ability.so": self.library + b"stale"}))

    def test_launcher_has_no_editor_source_or_library(self):
        report = self.runner.verify("launcher", self.package(product="launcher", libraries={}))
        self.assertNotIn("sourceRepository", report)
        self.assertIsNone(report["nativeLibrarySha256"])
        with self.assertRaisesRegex(RuntimeError, "unexpectedly contains"):
            self.runner.verify("launcher", self.package(product="launcher"))

    def test_rejects_relabeling_an_old_binary_after_dependency_change(self):
        manifest = self.root / "Cargo.toml"
        manifest.write_text(manifest.read_text().replace(self.revision, "b" * 40))
        changed_project = project.Project("debug", 2)
        with self.assertRaisesRegex(RuntimeError, "provenance is stale"):
            changed_project.verify("editor", self.package())

    def test_rejects_changed_rust_sources_and_features(self):
        source = self.root / "crates/editor-ability/src/host.rs"
        source.parent.mkdir(parents=True)
        source.write_text("// New host implementation\n")
        with self.assertRaisesRegex(RuntimeError, "provenance is stale"):
            self.runner.verify("editor", self.package())
        source.unlink()
        feature_project = project.Project("debug", 2, "ohos-control")
        with self.assertRaisesRegex(RuntimeError, "provenance is stale"):
            feature_project.verify("editor", self.package())

    def test_rejects_unpinned_dependency(self):
        manifest = self.root / "Cargo.toml"
        manifest.write_text(manifest.read_text().replace(self.revision, "main"))
        with self.assertRaisesRegex(RuntimeError, "full commit revision"):
            project.Project("debug", 2)

    def test_rejects_another_products_crate_mapping(self):
        catalog = self.root / "config/apps.json"
        catalog.write_text(catalog.read_text().replace("editor-ability", "another-ability"))
        with self.assertRaisesRegex(RuntimeError, "own native crate"):
            project.Project("debug", 2)

    def test_rejects_changed_cargo_configuration(self):
        config = self.root / ".cargo/config.toml"
        config.parent.mkdir()
        config.write_text('[build]\nrustflags = ["-C", "opt-level=1"]\n')
        with self.assertRaisesRegex(RuntimeError, "provenance is stale"):
            self.runner.verify("editor", self.package())

    def test_rejects_changed_compiler_flags(self):
        for key, value in {"RUSTFLAGS": "-C opt-level=1", "RUSTUP_TOOLCHAIN": "different-toolchain",
                           "OHOS_NDK_HOME": "/different-sdk"}.items():
            with self.subTest(key=key), patch.dict("os.environ", {key: value}):
                changed = project.Project("debug", 2)
                with self.assertRaisesRegex(RuntimeError, "provenance is stale"):
                    changed.verify("editor", self.package())

    def test_fingerprints_external_font_contents(self):
        fonts = self.root / "test-fonts"
        fonts.mkdir()
        font = fonts / "fixture.ttf"
        font.write_bytes(b"first fixture font")
        self.runner.env["CRAFT_FONTS_DIR"] = str(fonts)
        original = self.runner.native_provenance("editor")
        font.write_bytes(b"changed fixture font")
        self.assertNotEqual(original, self.runner.native_provenance("editor"))
        with self.assertRaisesRegex(RuntimeError, "provenance is stale"):
            self.runner.verify("editor", self.package())

    def test_batch_build_keeps_each_native_library_separate(self):
        catalog = self.root / "config/apps.json"
        data = json.loads(catalog.read_text())
        data["apps"].append({**self.app, "id": "second", "nativeCrate": "second-ability"})
        catalog.write_text(json.dumps(data))
        runner = project.Project("debug", 2)
        build_target = self.root / "cargo-target"
        native_target = build_target / project.TARGET / "debug"
        native_target.mkdir(parents=True)
        for product in ("editor", "second"):
            (native_target / f"lib{product}_ability.so").write_bytes(self.library + product.encode())

        def run(args, **kwargs):
            if args[:2] == ["cargo", "metadata"]:
                return json.dumps({"target_directory": str(build_target)})
            return ""

        with patch.object(runner, "native_environment"), patch.object(runner, "sdk", return_value=self.root / "sdk"), \
                patch.object(runner, "run", side_effect=run) as command:
            staged = runner.rust_products(["editor", "second"])
        builds = [call.args[0] for call in command.call_args_list if call.args[0][:2] == ["cargo", "build"]]
        self.assertEqual(len(builds), 1)
        self.assertIn("editor-ability", builds[0])
        self.assertIn("second-ability", builds[0])
        for product, path in staged.items():
            self.assertEqual(path, self.root / "entries" / product / f"libs/arm64-v8a/lib{product}_ability.so")
            self.assertEqual(path.read_bytes(), self.library + product.encode())
            report = self.root / f"target/native/debug/arm64-v8a/lib{product}_ability.json"
            self.assertEqual(json.loads(report.read_text())["sha256"], hashlib.sha256(path.read_bytes()).hexdigest())


if __name__ == "__main__":
    unittest.main()
