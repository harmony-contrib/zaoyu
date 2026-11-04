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
UPSTREAM_URL = "https://github.com/harmony-contrib/vectorcraft.git"
UPSTREAM_CRATES = ("vectorcraft-engine", "vectorcraft-ui-egui", "vectorcraft-text", "vectorcraft-format")
TARGET = "aarch64-unknown-linux-ohos"
PRODUCTS = ("vecrune", "nodune")
DEVICE_TYPES = ("tablet", "2in1")


class Project:
    def __init__(self, mode: str, jobs: int, features: str = ""):
        self.mode = mode
        self.jobs = jobs
        self.features = features
        self.env = os.environ.copy()
        self.env.setdefault("CARGO_NET_GIT_FETCH_WITH_CLI", "true")
        self.initial_encoded_flags = self.env.get("CARGO_ENCODED_RUSTFLAGS", "")
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
        dependencies = manifest["workspace"]["dependencies"]
        revisions = set()
        for name in UPSTREAM_CRATES:
            dependency = dependencies[name]
            revision = dependency.get("rev", "")
            if (dependency.get("git") != UPSTREAM_URL or "path" in dependency
                    or len(revision) != 40 or any(char not in "0123456789abcdef" for char in revision)):
                raise RuntimeError(f"{name} must use the upstream Git URL and a full commit revision")
            revisions.add(revision)
        if len(revisions) != 1:
            raise RuntimeError("VectorCraft crates must use the same Git revision")
        self.source_revision = revisions.pop()

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
        catalog = json.loads((ROOT / "config/apps.json").read_text())
        apps = catalog["apps"]
        if len({app["bundleName"] for app in apps}) != len(apps):
            raise RuntimeError("Duplicate bundle name in config/apps.json")
        for app in apps:
            if app["bundleName"] != "com.richerfu." + app["id"]:
                raise RuntimeError(f"Unexpected bundle name for {app['id']}")
        items = [{"id": a["id"], "english": a["english"], "chinese": a["chinese"],
                  "status": "已接入" if a["status"] == "integrated" else "待迁移"} for a in apps]
        source = (
            "export interface AppItem {\n  id: string;\n  english: string;\n  chinese: string;\n  status: string;\n}\n\n"
            "export const APPS: AppItem[] = " + json.dumps(items, ensure_ascii=False, indent=2) + ";\n"
        )
        (ROOT / "entries/nodune/src/main/ets/catalog.ets").write_text(source)

    def setup(self) -> None:
        self.catalog()

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

    def rust(self) -> Path:
        self.native_environment()
        args = ["cargo", "build", "--locked", "-p", "vecrune-ability", "--target", TARGET,
                "--lib", "-j", str(self.jobs)]
        if self.mode == "release":
            args.append("--release")
        if self.features:
            args.extend(["--features", self.features])
        self.run(args)
        profile = "release" if self.mode == "release" else "debug"
        metadata = json.loads(self.run(["cargo", "metadata", "--format-version", "1", "--no-deps"], capture=True))
        source = Path(metadata["target_directory"]) / TARGET / profile / "libvecrune_ability.so"
        expected = ROOT / "target/native" / self.mode / "arm64-v8a/libvecrune_ability.so"
        expected.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, expected)
        # arkdown strips packaged native libraries. Normalize a separate copy
        # for each build mode; Cargo retains symbols for native debugging.
        self.run([str(self.sdk() / "native/llvm/bin/llvm-strip"), "--strip-all", str(expected)])
        self.check_elf(expected.read_bytes(), str(expected))
        destination = ROOT / "entries/vecrune/libs/arm64-v8a/libvecrune_ability.so"
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(expected, destination)
        return destination

    @staticmethod
    def check_elf(data: bytes, label: str) -> None:
        if len(data) < 20 or data[:6] != b"\x7fELF\x02\x01" or int.from_bytes(data[18:20], "little") != 183:
            raise RuntimeError(f"{label} is not a 64-bit ARM ELF library")

    def verify(self, product: str, hap: Path) -> dict:
        native_hash: str | None = None
        with zipfile.ZipFile(hap) as archive:
            manifest = json.loads(archive.read("module.json"))
            bundle = manifest["app"]["bundleName"]
            module = manifest["module"]["name"]
            if bundle != "com.richerfu." + product or module != product:
                raise RuntimeError(f"Package identity mismatch: {bundle}, {module}")
            device_types = manifest["module"]["deviceTypes"]
            if sorted(device_types) != sorted(DEVICE_TYPES):
                raise RuntimeError(f"Unexpected device scope: {device_types}; expected {DEVICE_TYPES}")
            if product == "vecrune":
                library = "libs/arm64-v8a/libvecrune_ability.so"
                self.check_elf(archive.read(library), library)
                source = ROOT / "target/native" / self.mode / "arm64-v8a/libvecrune_ability.so"
                if archive.read(library) != source.read_bytes():
                    raise RuntimeError("Packaged native library differs from the current build")
                native_hash = hashlib.sha256(archive.read(library)).hexdigest()
            elif any(name.endswith("libvecrune_ability.so") for name in archive.namelist()):
                raise RuntimeError("Nodune unexpectedly contains the Vecrune native editor")
        return {"product": product, "bundleName": bundle, "module": module, "mode": self.mode,
                "deviceTypes": device_types,
                "signed": False, "sha256": hashlib.sha256(hap.read_bytes()).hexdigest(), "bytes": hap.stat().st_size,
                "sourceRepository": UPSTREAM_URL, "sourceRevision": self.source_revision,
                "nativeLibrarySha256": native_hash}

    def build(self, product: str) -> None:
        self.setup()
        if product == "vecrune":
            self.rust()
        self.run([self.arkdown(), "build", "--project", str(ROOT), "--product", product,
                  "--module", product, "--target", "hap", "--mode", self.mode, "--clearScreen", "false"])
        outputs = ROOT / "entries" / product / "build" / product / "outputs/default"
        source = outputs / f"{product}-default-unsigned.hap"
        report = self.verify(product, source)
        report["rustFeatures"] = self.features.split(",") if product == "vecrune" and self.features else []
        destination = ROOT / "dist" / self.mode / f"{product}-unsigned.hap"
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
        destination.with_suffix(".json").write_text(json.dumps(report, indent=2) + "\n")
        print(f"Verified {report['bundleName']}: {destination}")

    def check(self) -> None:
        self.setup()
        self.run(["cargo", "fmt", "-p", "vecrune-host", "-p", "vecrune-ability", "--", "--check"])
        self.run(["cargo", "test", "--locked", "-p", "vecrune-host", "--test", "host_policy"])
        self.native_environment()
        self.run(["cargo", "clippy", "--locked", "--workspace", "--lib", "--all-features",
                  "--target", TARGET, "-j", str(self.jobs), "--", "-D", "warnings"])
        for product in PRODUCTS:
            self.build(product)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["setup", "doctor", "deps", "rust", "build", "check", "verify"])
    parser.add_argument("product", choices=[*PRODUCTS, "all"], default="vecrune", nargs="?")
    parser.add_argument("--mode", choices=["debug", "release"], default="debug")
    parser.add_argument("--jobs", type=int, default=2)
    parser.add_argument("--features", choices=["ohos-control"], default="", help="Optional Vecrune Rust feature; disabled by default")
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error("--jobs must be positive")
    try:
        project = Project(args.mode, args.jobs, args.features)
        if args.command == "build":
            for product in PRODUCTS if args.product == "all" else (args.product,):
                project.build(product)
        elif args.command == "verify":
            for product in PRODUCTS if args.product == "all" else (args.product,):
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
