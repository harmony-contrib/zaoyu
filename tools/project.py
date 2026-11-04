#!/usr/bin/env python3
"""One entry point for dependency installation, Rust builds and arkdown packaging."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib
import zipfile


ROOT = Path(__file__).resolve().parents[1]
TARGET = "aarch64-unknown-linux-ohos"
DEVICE_TYPES = ("tablet", "2in1")


class Project:
    def __init__(self, mode: str, jobs: int, features: str = ""):
        self.mode = mode
        self.jobs = jobs
        self.features = features
        self.env = os.environ.copy()
        self.env.setdefault("CARGO_NET_GIT_FETCH_WITH_CLI", "true")
        self.initial_encoded_flags = self.env.get("CARGO_ENCODED_RUSTFLAGS", "")
        native_keys = {"RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER",
                       "RUSTC_WORKSPACE_WRAPPER", "RUSTUP_TOOLCHAIN", "OHOS_NDK_HOME",
                       "OHOS_SDK_HOME", "DEVECO_SDK_HOME", "CC", "CXX", "AR", "CFLAGS", "CXXFLAGS"}
        self.native_build_environment = {
            key: value for key, value in self.env.items()
            if key in native_keys or key.startswith(("CARGO_PROFILE_", "CARGO_TARGET_", "CC_", "CXX_", "AR_", "CFLAGS_", "CXXFLAGS_"))
        }
        self.catalog_data = json.loads((ROOT / "config/apps.json").read_text())
        self.apps = {app["id"]: app for app in self.catalog_data["apps"]}
        if len(self.apps) != len(self.catalog_data["apps"]):
            raise RuntimeError("Duplicate application ID in config/apps.json")
        self.products = tuple(name for name, app in self.apps.items() if app["status"] == "integrated")
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
        dependencies = manifest["workspace"]["dependencies"]
        self.sources: dict[str, dict] = {}
        for product in self.products:
            app = self.apps[product]
            if not app.get("nativeCrate"):
                continue
            if app["nativeCrate"] != f"{product}-ability":
                raise RuntimeError(f"{product} must use its own native crate: {product}-ability")
            revisions = set()
            for name in app["upstreamCrates"]:
                dependency = dependencies[name]
                revision = dependency.get("rev", "")
                if (dependency.get("git") != app["sourceRepository"] or "path" in dependency
                        or len(revision) != 40 or any(char not in "0123456789abcdef" for char in revision)):
                    raise RuntimeError(f"{name} must use the upstream Git URL and a full commit revision")
                revisions.add(revision)
            if len(revisions) != 1:
                raise RuntimeError(f"{app['upstreamName']} crates must use the same Git revision")
            self.sources[product] = {"sourceRepository": app["sourceRepository"],
                                     "sourceRevision": revisions.pop()}

    def run(self, args: list[str], *, cwd: Path = ROOT, capture: bool = False) -> str:
        result = subprocess.run(
            args, cwd=cwd, env=self.env, check=True, text=True,
            stdout=subprocess.PIPE if capture else None,
        )
        return result.stdout.strip() if capture else ""

    def arkdown(self) -> str:
        local = ROOT / "node_modules/.bin/arkdown"
        found = str(local) if local.is_file() else shutil.which("arkdown")
        if not found:
            raise RuntimeError("arkdown is missing; run npm ci first")
        return found

    def sdk(self) -> Path:
        candidates: list[Path] = []
        for key in ("OHOS_NDK_HOME", "OHOS_SDK_HOME", "DEVECO_SDK_HOME"):
            if self.env.get(key):
                base = Path(self.env[key]).expanduser()
                candidates.extend([base, base / "openharmony", base / "default/openharmony"])
        hdc = shutil.which("hdc")
        if hdc:
            candidates.append(Path(hdc).resolve().parents[1])
        candidates.extend(sorted(Path("/Applications").glob("DevEco*.app/Contents/sdk/default/openharmony")))
        candidates.extend(sorted((Path.home() / ".meat/ide").glob("DevEco*.app/Contents/sdk/default/openharmony"), reverse=True))
        for candidate in candidates:
            if (candidate / "native/llvm/bin/clang").is_file():
                return candidate.resolve()
        raise RuntimeError("Set OHOS_NDK_HOME to an OpenHarmony SDK containing native/llvm/bin/clang")

    def native_environment(self) -> None:
        sdk = self.sdk()
        native = sdk / "native"
        self.env["OHOS_NDK_HOME"] = str(sdk)
        self.env["CARGO_TARGET_AARCH64_UNKNOWN_LINUX_OHOS_LINKER"] = str(native / "llvm/bin/clang")
        self.env["CC_aarch64_unknown_linux_ohos"] = str(native / "llvm/bin/clang")
        self.env["AR_aarch64_unknown_linux_ohos"] = str(native / "llvm/bin/llvm-ar")
        self.env["CFLAGS_aarch64_unknown_linux_ohos"] = f'--target=aarch64-linux-ohos --sysroot="{native / "sysroot"}"'
        flags = self.initial_encoded_flags
        if not flags and self.env.get("RUSTFLAGS"):
            import shlex
            flags = "\x1f".join(shlex.split(self.env["RUSTFLAGS"]))
        additional = ["-C", "link-arg=--target=aarch64-linux-ohos", "-C", f"link-arg=--sysroot={native / 'sysroot'}"]
        self.env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(([flags] if flags else []) + additional)

    def catalog(self) -> None:
        apps = self.catalog_data["apps"]
        if len({app["bundleName"] for app in apps}) != len(apps):
            raise RuntimeError("Duplicate bundle name in config/apps.json")
        for app in apps:
            if app["bundleName"] != "com.richerfu." + app["id"]:
                raise RuntimeError(f"Unexpected bundle name for {app['id']}")
        items = [{"id": a["id"], "english": a["english"], "chinese": a["chinese"],
                  "available": a["status"] == "integrated",
                  "status": "可使用" if a["status"] == "integrated" else "尚未推出"} for a in apps]
        source = (
            "export interface AppItem {\n  id: string;\n  english: string;\n  chinese: string;\n  available: boolean;\n  status: string;\n}\n\n"
            "export const APPS: AppItem[] = " + json.dumps(items, ensure_ascii=False, indent=2) + ";\n"
        )
        (ROOT / "entries/nodune/src/main/ets/catalog.ets").write_text(source)

    def setup(self) -> None:
        self.catalog()
        for product in self.products:
            destination = ROOT / "entries" / product / "src/main/resources/rawfile/zaoyu-licenses"
            if destination.exists():
                shutil.rmtree(destination)
            destination.mkdir(parents=True)
            for name in ("LICENSE-MIT", "LICENSE-APACHE", "NOTICE"):
                shutil.copy2(ROOT / name, destination / name)
            if self.apps[product].get("nativeCrate"):
                upstream = self.apps[product]["upstreamName"].lower()
                shutil.copytree(ROOT / "licenses" / upstream, destination / "upstream")

    def deps(self) -> None:
        self.run(["ohpm-rs", "install", "--workspace", "--all"])

    def doctor(self) -> None:
        for exe in ("git", "node", "cargo", "rustc", "ohpm-rs"):
            self.run([exe, "--version"])
        self.run([self.arkdown(), "--version"])
        print(f"SDK: {self.sdk()}")
        targets = self.run(["rustup", "target", "list", "--installed"], capture=True).splitlines()
        if TARGET not in targets:
            raise RuntimeError(f"Install the Rust target: rustup target add {TARGET}")
        print(f"Rust target: {TARGET}")

    def rust(self, product: str) -> Path:
        if not self.apps[product].get("nativeCrate"):
            raise RuntimeError(f"{product} does not contain a Rust library")
        return self.rust_products([product])[product]

    def rust_products(self, products: list[str]) -> dict[str, Path]:
        if not products:
            return {}
        self.native_environment()
        args = ["cargo", "build", "--locked", "--target", TARGET,
                "--lib", "-j", str(self.jobs)]
        for product in products:
            args.extend(["-p", self.apps[product]["nativeCrate"]])
        if self.mode == "release":
            args.append("--release")
        if self.features:
            args.extend(["--features", self.features])
        provenance = {product: self.native_provenance(product) for product in products}
        self.run(args)
        if {product: self.native_provenance(product) for product in products} != provenance:
            raise RuntimeError("Rust inputs changed during compilation; rebuild from a stable workspace")
        profile = "release" if self.mode == "release" else "debug"
        metadata = json.loads(self.run(["cargo", "metadata", "--format-version", "1", "--no-deps"], capture=True))
        target = Path(metadata["target_directory"]) / TARGET / profile
        return {product: self.stage_native(product, target, provenance[product]) for product in products}

    def stage_native(self, product: str, target: Path, provenance: dict) -> Path:
        library_name = f"lib{self.apps[product]['nativeCrate'].replace('-', '_')}.so"
        source = target / library_name
        expected = ROOT / "target/native" / self.mode / "arm64-v8a" / library_name
        expected.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, expected)
        # arkdown strips packaged native libraries. Normalize a separate copy
        # for each build mode; Cargo retains symbols for native debugging.
        self.run([str(self.sdk() / "native/llvm/bin/llvm-strip"), "--strip-all", str(expected)])
        self.check_elf(expected.read_bytes(), str(expected))
        provenance["sha256"] = hashlib.sha256(expected.read_bytes()).hexdigest()
        expected.with_suffix(".json").write_text(json.dumps(provenance, indent=2) + "\n")
        destination = ROOT / "entries" / product / "libs/arm64-v8a" / library_name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(expected, destination)
        return destination

    def native_provenance(self, product: str) -> dict:
        # A standalone verification must not relabel an older binary after edits.
        inputs = [ROOT / "Cargo.toml", ROOT / "Cargo.lock"]
        inputs.extend((ROOT / "crates").glob("**/*.rs"))
        inputs.extend((ROOT / "crates").glob("**/Cargo.toml"))
        inputs.extend((ROOT / "branding").glob("*.png"))
        inputs.extend(path for path in (ROOT / "rust-toolchain", ROOT / "rust-toolchain.toml", ROOT / "tools/project.py") if path.is_file())
        digest = hashlib.sha256()
        for path in sorted(inputs):
            digest.update(path.relative_to(ROOT).as_posix().encode())
            digest.update(b"\0")
            digest.update(hashlib.sha256(path.read_bytes()).digest())
        # Cargo reads project/ancestor configuration as well as CARGO_HOME. Keep
        # these and caller-supplied compiler settings in the build fingerprint.
        cargo_home = Path(self.env.get("CARGO_HOME", str(Path.home() / ".cargo"))).expanduser()
        config_dirs = [base / ".cargo" for base in (ROOT, *ROOT.parents)] + [cargo_home]
        for index, directory in enumerate(config_dirs):
            for name in ("config", "config.toml"):
                config = directory / name
                if config.is_file():
                    digest.update(f"cargo-config/{index}/{name}\0".encode())
                    digest.update(hashlib.sha256(config.read_bytes()).digest())
        digest.update(json.dumps(self.native_build_environment, sort_keys=True).encode())
        fonts = self.env.get("CRAFT_FONTS_DIR")
        if fonts:
            font_root = Path(fonts).expanduser().resolve(strict=True)
            if not font_root.is_dir():
                raise RuntimeError("CRAFT_FONTS_DIR must name a directory")
            digest.update(b"craft-fonts\0")
            for path in sorted(font_root.rglob("*")):
                relative = path.relative_to(font_root)
                if ".git" in relative.parts or not path.is_file():
                    continue
                digest.update(relative.as_posix().encode() + b"\0")
                with path.open("rb") as source:
                    digest.update(hashlib.file_digest(source, "sha256").digest())
        return {**self.sources[product], "mode": self.mode, "target": TARGET,
                "rustFeatures": self.features.split(",") if self.features else [],
                "nativeInputsSha256": digest.hexdigest()}

    @staticmethod
    def check_elf(data: bytes, label: str) -> None:
        if len(data) < 20 or data[:6] != b"\x7fELF\x02\x01" or int.from_bytes(data[18:20], "little") != 183:
            raise RuntimeError(f"{label} is not a 64-bit ARM ELF library")

    def verify(self, product: str, hap: Path) -> dict:
        native_hash: str | None = None
        provenance: dict = {}
        with zipfile.ZipFile(hap) as archive:
            manifest = json.loads(archive.read("module.json"))
            bundle = manifest["app"]["bundleName"]
            module = manifest["module"]["name"]
            if bundle != "com.richerfu." + product or module != product:
                raise RuntimeError(f"Package identity mismatch: {bundle}, {module}")
            device_types = manifest["module"]["deviceTypes"]
            if sorted(device_types) != sorted(DEVICE_TYPES):
                raise RuntimeError(f"Unexpected device scope: {device_types}; expected {DEVICE_TYPES}")
            abilities = manifest["module"].get("abilities", [])
            if not any(ability.get("name") == "EntryAbility" and ability.get("exported")
                       and any("ohos.want.action.home" in skill.get("actions", [])
                               for skill in ability.get("skills", [])) for ability in abilities):
                raise RuntimeError("Package has no exported desktop launcher entry")
            for name in ("LICENSE-MIT", "LICENSE-APACHE", "NOTICE"):
                if f"resources/rawfile/zaoyu-licenses/{name}" not in archive.namelist():
                    raise RuntimeError(f"Package is missing license notice: {name}")
            native_libraries = {name for name in archive.namelist() if name.endswith("_ability.so")}
            crate = self.apps[product].get("nativeCrate")
            if crate:
                library_name = f"lib{crate.replace('-', '_')}.so"
                library = f"libs/arm64-v8a/{library_name}"
                if native_libraries != {library}:
                    raise RuntimeError(f"Unexpected native editor libraries: {native_libraries}")
                if "resources/rawfile/zaoyu-licenses/upstream/LICENSE-MIT" not in archive.namelist():
                    raise RuntimeError("Package is missing upstream license notices")
                self.check_elf(archive.read(library), library)
                source = ROOT / "target/native" / self.mode / "arm64-v8a" / library_name
                if archive.read(library) != source.read_bytes():
                    raise RuntimeError("Packaged native library differs from the current build")
                native_hash = hashlib.sha256(archive.read(library)).hexdigest()
                provenance = json.loads(source.with_suffix(".json").read_text())
                expected_provenance = {**self.native_provenance(product), "sha256": native_hash}
                if provenance != expected_provenance:
                    raise RuntimeError("Native build provenance is stale; rebuild after source, lockfile or feature changes")
            elif native_libraries:
                raise RuntimeError(f"{product} unexpectedly contains native editor libraries")
        return {"product": product, "bundleName": bundle, "module": module, "mode": self.mode,
                "deviceTypes": device_types,
                "licensesBundled": True,
                "signed": False, "sha256": hashlib.sha256(hap.read_bytes()).hexdigest(), "bytes": hap.stat().st_size,
                **self.sources.get(product, {}),
                "nativeInputsSha256": provenance.get("nativeInputsSha256"),
                "rustFeatures": provenance.get("rustFeatures", []),
                "nativeLibrarySha256": native_hash}

    def build(self, product: str, *, native: bool = True) -> None:
        self.setup()
        if native and self.apps[product].get("nativeCrate"):
            self.rust(product)
        self.run([self.arkdown(), "build", "--project", str(ROOT), "--product", product,
                  "--module", product, "--target", "hap", "--mode", self.mode, "--clearScreen", "false"])
        outputs = ROOT / "entries" / product / "build" / product / "outputs/default"
        source = outputs / f"{product}-default-unsigned.hap"
        report = self.verify(product, source)
        destination = ROOT / "dist" / self.mode / f"{product}-unsigned.hap"
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
        destination.with_suffix(".json").write_text(json.dumps(report, indent=2) + "\n")
        print(f"Verified {report['bundleName']}: {destination}")

    def build_all(self) -> None:
        # One Cargo graph shares common renderer/format dependencies across the
        # editors. Each cdylib still goes only into its own product's HAP.
        self.rust_products([product for product in self.products if self.apps[product].get("nativeCrate")])
        for product in self.products:
            self.build(product, native=False)

    def check(self) -> None:
        self.setup()
        self.run([sys.executable, "-m", "unittest", "discover", "-s", "tools", "-p", "test_*.py"])
        self.run(["cargo", "fmt", "--all", "--", "--check"])
        self.run(["cargo", "test", "--locked", "--workspace", "--tests", "-j", str(self.jobs)])
        self.native_environment()
        self.run(["cargo", "clippy", "--locked", "--workspace", "--lib", "--all-features",
                  "--target", TARGET, "-j", str(self.jobs), "--", "-D", "warnings"])
        self.build_all()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["setup", "doctor", "deps", "rust", "build", "check", "verify"])
    parser.add_argument("product", default="vecrune", nargs="?", help="Integrated application ID, or all")
    parser.add_argument("--mode", choices=["debug", "release"], default="debug")
    parser.add_argument("--jobs", type=int, default=2)
    parser.add_argument("--features", choices=["ohos-control"], default="", help="Optional Rust development control channel; disabled by default")
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error("--jobs must be positive")
    try:
        project = Project(args.mode, args.jobs, args.features)
        if args.product not in (*project.products, "all"):
            parser.error(f"Unknown integrated product: {args.product}; choose from {', '.join(project.products)}")
        if args.command == "build":
            if args.product == "all":
                project.build_all()
            else:
                project.build(args.product)
        elif args.command == "rust":
            if args.product == "all":
                project.rust_products([product for product in project.products if project.apps[product].get("nativeCrate")])
            else:
                project.rust(args.product)
        elif args.command == "verify":
            for product in project.products if args.product == "all" else (args.product,):
                path = ROOT / "dist" / args.mode / f"{product}-unsigned.hap"
                print(json.dumps(project.verify(product, path), indent=2))
        else:
            getattr(project, args.command)()
    except (RuntimeError, OSError, subprocess.CalledProcessError, KeyError, zipfile.BadZipFile) as error:
        print(f"Project error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
