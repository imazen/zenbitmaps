#!/usr/bin/env python3
"""Farbfeld Rgba8 parity: a git-archived baseline against a candidate revision.

Generates both source trees, the harness manifest and every build output in a
new scratch directory outside the repository, then runs the requested groups.
Git is used read-only (rev-parse, archive). See README.md.
"""

import argparse
import datetime
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import tomllib
from pathlib import Path

BASELINE = "a92accff501d22af2b75405a7b7b99c9520f2fb4"
I686 = "i686-unknown-linux-gnu"
GROUPS = ("core", "mutants", "f6", "imagemagick", "i686", "i686-f6")
RAM_FILESYSTEMS = {"tmpfs", "ramfs"}
REFUSED_PREFIXES = ("/tmp", "/run", "/dev/shm")
TOOL = Path(__file__).resolve().parent


def fail(msg):
    print(f"farbfeld-parity: {msg}", file=sys.stderr)
    sys.exit(2)


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def capture(cmd, **kw):
    return subprocess.run(
        cmd, check=True, capture_output=True, text=True, **kw
    ).stdout.strip()


def run_logged(cmd, log, env=None):
    """Runs cmd, streaming combined output to the console and to log."""
    log.parent.mkdir(parents=True, exist_ok=True)
    with open(log, "a", encoding="utf-8") as out:
        out.write(f"$ {json.dumps([str(c) for c in cmd])}\n")
        out.flush()
        try:
            proc = subprocess.Popen(
                [str(c) for c in cmd],
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                env=env,
            )
        except OSError as e:
            out.write(f"could not execute: {e}\n")
            print(f"could not execute {cmd[0]}: {e}")
            return 127
        for line in proc.stdout:
            text = line.decode("utf-8", "replace")
            out.write(text)
            sys.stdout.write(text)
        rc = proc.wait()
        out.write(f"exit status {rc}\n")
    return rc


def mount_fstype(path):
    """Filesystem type of the mount holding path, from /proc/self/mounts."""
    try:
        with open("/proc/self/mounts", encoding="utf-8") as f:
            mounts = [line.split() for line in f]
    except OSError:
        fail(
            "cannot read /proc/self/mounts to check that the scratch directory is disk-backed"
        )

    def unescape(s):
        # The kernel writes space, tab, newline and backslash as \ooo octal.
        return re.sub(r"\\([0-7]{3})", lambda m: chr(int(m.group(1), 8)), s)

    best, fstype = "", None
    for fields in mounts:
        point = unescape(fields[1])
        inside = path == point or path.startswith(point.rstrip("/") + "/")
        if inside and len(point) >= len(best):
            best, fstype = point, fields[2]
    return fstype


def make_scratch(requested, repo):
    path = Path(os.path.abspath(requested))
    if os.path.lexists(path):
        fail(f"refusing existing path {path}: the scratch directory must be new")
    ancestor = path.parent
    while not ancestor.exists():
        ancestor = ancestor.parent
    real = os.path.realpath(ancestor)
    for prefix in REFUSED_PREFIXES:
        if real == prefix or real.startswith(prefix + "/"):
            fail(f"refusing {path}: under {prefix}, which is not durable disk")
    fstype = mount_fstype(real)
    if fstype in RAM_FILESYSTEMS:
        fail(f"refusing {path}: {real} is on {fstype}, not disk")
    if real == str(repo) or real.startswith(str(repo) + "/"):
        fail(f"refusing {path}: generated sources must stay outside the repository")
    path.mkdir(parents=True, exist_ok=False)
    return Path(os.path.realpath(path)), fstype


def resolve(repo, rev):
    try:
        return capture(
            [
                "git",
                "-C",
                repo,
                "rev-parse",
                "--verify",
                "--end-of-options",
                rev + "^{commit}",
            ]
        )
    except subprocess.CalledProcessError as e:
        fail(f"cannot resolve revision {rev!r}: {e.stderr.strip()}")


def archive(repo, sha, dest):
    dest.mkdir()
    proc = subprocess.Popen(
        ["git", "-C", repo, "archive", "--format=tar", sha], stdout=subprocess.PIPE
    )
    with tarfile.open(fileobj=proc.stdout, mode="r|") as tar:
        tar.extractall(dest, filter="data")
    if proc.wait() != 0:
        fail(f"git archive {sha} failed")


def rename_old_package(manifest):
    """Renames the baseline package so both trees build in one graph."""
    if tomllib.loads(manifest.read_text())["package"]["name"] != "zenbitmaps":
        fail(f"{manifest}: baseline package is not named zenbitmaps")
    lines = manifest.read_text().splitlines(keepends=True)
    section, done = None, False
    for i, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("["):
            section = stripped
        elif (
            section == "[package]"
            and not done
            and stripped.replace(" ", "") == 'name="zenbitmaps"'
        ):
            lines[i] = 'name = "zenbitmaps_old"\n'
            done = True
    manifest.write_text("".join(lines))
    if (
        not done
        or tomllib.loads(manifest.read_text())["package"]["name"] != "zenbitmaps_old"
    ):
        fail(f"{manifest}: could not rename the baseline package")


def main():
    if sys.version_info < (3, 12):
        fail("Python 3.12 or newer is required (tarfile data filter)")
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument(
        "--scratch", required=True, help="new directory for all generated files"
    )
    ap.add_argument(
        "--baseline", default=BASELINE, help=f"baseline revision (default {BASELINE})"
    )
    ap.add_argument(
        "--candidate", default="HEAD", help="candidate revision (default HEAD)"
    )
    ap.add_argument(
        "--groups", default="core", help=f"comma-separated, from: {', '.join(GROUPS)}"
    )
    ap.add_argument("--offline", action="store_true", help="pass --offline to cargo")
    args = ap.parse_args()

    groups = [g for g in args.groups.split(",") if g]
    unknown = [g for g in groups if g not in GROUPS]
    if unknown or not groups:
        fail(f"unknown or empty groups {unknown}; choose from {', '.join(GROUPS)}")

    repo = Path(capture(["git", "-C", TOOL, "rev-parse", "--show-toplevel"])).resolve()
    base_sha = resolve(repo, args.baseline)
    cand_sha = resolve(repo, args.candidate)
    scratch, fstype = make_scratch(args.scratch, repo)
    logs = scratch / "logs"
    print(f"baseline  {args.baseline} -> {base_sha}")
    print(f"candidate {args.candidate} -> {cand_sha}")
    print(f"scratch   {scratch} ({fstype})")

    archive(repo, base_sha, scratch / "old")
    archive(repo, cand_sha, scratch / "new")
    rename_old_package(scratch / "old" / "Cargo.toml")
    harness = scratch / "harness"
    (harness / "src").mkdir(parents=True)
    shutil.copyfile(TOOL / "harness" / "Cargo.toml.in", harness / "Cargo.toml")
    shutil.copyfile(TOOL / "harness" / "src" / "main.rs", harness / "src" / "main.rs")
    lock = scratch / "new" / "Cargo.lock"
    if not lock.is_file():
        fail(f"candidate {cand_sha} has no Cargo.lock to pin shared dependencies")
    shutil.copyfile(lock, harness / "Cargo.lock")

    encoder = "src/farbfeld/encode.rs"
    identity = {
        "utc": datetime.datetime.now(datetime.UTC).isoformat(timespec="seconds"),
        "argv": sys.argv,
        "groups": groups,
        "baseline": {"declared": args.baseline, "commit": base_sha},
        "candidate": {"declared": args.candidate, "commit": cand_sha},
        "encoder_sha256": {
            "baseline": sha256(scratch / "old" / encoder),
            "candidate": sha256(scratch / "new" / encoder),
        },
        "tool_sha256": {
            p: sha256(TOOL / p)
            for p in ("run.py", "harness/Cargo.toml.in", "harness/src/main.rs")
        },
        "host": {"machine": platform.machine(), "system": platform.system()},
        "rustc": capture(["rustc", "-vV"]),
        "cargo": capture(["cargo", "-V"]),
        "results": {},
    }

    cargo_base = [
        "cargo",
        "build",
        "--manifest-path",
        harness / "Cargo.toml",
        "--target-dir",
        scratch / "target",
    ]
    if args.offline:
        cargo_base.append("--offline")
    built = {}

    def binary(profile, target=None):
        key = (profile, target)
        if key not in built:
            cmd = (
                cargo_base
                + ["--profile", profile]
                + (["--target", target] if target else [])
            )
            log = logs / f"build-{profile}-{target or 'host'}.log"
            if run_logged(cmd, log) != 0:
                built[key] = None
                print(f"build failed ({profile}, {target or 'host'}): see {log}")
            else:
                parts = (
                    [scratch / "target"]
                    + ([target] if target else [])
                    + [profile, "farbfeld-parity"]
                )
                built[key] = Path(*parts)
        return built[key]

    def run_group(name, legs):
        """legs: (profile, target, harness args, log name). Every leg must exit 0."""
        ok = True
        for profile, target, hargs, logname in legs:
            exe = binary(profile, target)
            if exe is None:
                ok = False
                continue
            print(f"== {name}: {profile} {target or 'host'} {' '.join(hargs)}")
            ok &= run_logged([exe] + hargs, logs / logname) == 0
        return ok

    def imagemagick():
        magick = shutil.which("magick")
        if magick is None:
            print("imagemagick: FAIL (requested, but no `magick` on PATH)")
            return False
        identity["magick"] = capture([magick, "-version"]).splitlines()[0]
        oracle = scratch / "oracle"
        oracle.mkdir()
        if not run_group(
            "imagemagick",
            [("release", None, ["write-oracle", str(oracle)], "imagemagick.log")],
        ):
            return False
        files = sorted(oracle.glob("*.ff"))
        ok = bool(files)
        for ff in files:
            got16, got8 = ff.with_suffix(".magick16be"), ff.with_suffix(".magick8")
            for cmd in (
                [magick, ff, "-depth", "16", "-endian", "MSB", f"rgba:{got16}"],
                [magick, ff, "-depth", "8", f"rgba:{got8}"],
            ):
                ok &= run_logged(cmd, logs / "imagemagick.log") == 0
            r16 = (
                got16.is_file()
                and got16.read_bytes() == ff.with_suffix(".expect16be").read_bytes()
            )
            r8 = (
                got8.is_file()
                and got8.read_bytes() == ff.with_suffix(".input8").read_bytes()
            )
            print(
                f"{ff.name}: 16-bit read-back {'== spec' if r16 else 'MISMATCH'}, "
                f"8-bit read-back {'== input' if r8 else 'MISMATCH'}"
            )
            ok &= r16 and r8
        return ok

    plan = {
        "core": lambda: run_group("core", [("release", None, ["core"], "core.log")]),
        "mutants": lambda: run_group(
            "mutants", [("release", None, ["mutants"], "mutants.log")]
        ),
        "f6": lambda: run_group(
            "f6",
            [
                ("release", None, ["f6"], "f6-checked-host.log"),
                ("relnocheck", None, ["f6"], "f6-unchecked-host.log"),
            ],
        ),
        "imagemagick": imagemagick,
        "i686": lambda: run_group(
            "i686", [("release", I686, ["core"], "core-i686.log")]
        ),
        "i686-f6": lambda: run_group(
            "i686-f6",
            [
                ("release", I686, ["f6"], "f6-checked-i686.log"),
                ("relnocheck", I686, ["f6"], "f6-unchecked-i686.log"),
            ],
        ),
    }
    for g in groups:
        identity["results"][g] = "PASS" if plan[g]() else "FAIL"

    identity["harness_lock_sha256"] = sha256(harness / "Cargo.lock")
    identity["binaries_sha256"] = {
        f"{p}/{t or 'host'}": sha256(b)
        for (p, t), b in built.items()
        if b is not None and b.is_file()
    }
    (scratch / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    print(f"identity: {scratch / 'identity.json'}")
    for g, r in identity["results"].items():
        print(f"{g}: {r}")
    sys.exit(0 if all(r == "PASS" for r in identity["results"].values()) else 1)


if __name__ == "__main__":
    main()
