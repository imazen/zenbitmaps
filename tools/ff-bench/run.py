#!/usr/bin/env python3
"""ff-bench: build a same-binary baseline/candidate/AA farbfeld benchmark
from two immutable zenbitmaps revisions.

Archives both revisions into a new scratch dir, generates a harness package
(`zenbitmaps_old` = baseline, `zenbitmaps` = candidate) whose bench target
registers baseline/candidate/aa arms per encode cell and
olddecode/newdecode/oldaa for the decode control, then runs the requested
actions: --verify (out-of-timer byte/spec parity + artifact export) and/or
--dry-run (zenbench Confirm plan resolution + executable sha256; no timing).

Every fresh process re-runs the full parity assertions via the shared
preflight module before any benchmark callback is registered.

Real Confirm runs belong to the reviewer on a quiet host; this driver never
launches timing itself. Git is used read-only (rev-parse, archive, show).
"""

import argparse
import datetime
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import tomllib
from pathlib import Path

BASELINE = "a92accff501d22af2b75405a7b7b99c9520f2fb4"
WORKLOAD_ID = "zenbitmaps-farbfeld-encode-v2"
REFUSED_PREFIXES = ("/tmp", "/run", "/dev/shm")
TOOL = Path(__file__).resolve().parent
TOOL_FILES = (
    "run.py",
    "Cargo.toml.in",
    "README.md",
    "cli-check.sh",
    "driver-check.py",
    "harness/lib.rs",
    "harness/verify.rs",
    "harness/ff.rs",
)
# Flags a caller may append to the bench binary. Placement only; anything
# that could relax the frozen confirm plan (mode/processes/rounds/margin/
# confidence/workload/timeout/unrestricted/...) is refused before any
# filesystem mutation.
BENCH_ARG_CPU_POLICY_VALUES = ("auto", "performance")
ENV_RECORDED = (
    "RUSTFLAGS",
    "CARGO_ENCODED_RUSTFLAGS",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "CARGO_BUILD_TARGET",
    "CARGO_TARGET_DIR",
    "CARGO_INCREMENTAL",
    "CARGO_PROFILE_RELEASE_LTO",
    "CARGO_PROFILE_RELEASE_OPT_LEVEL",
    "CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS",
    "CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS",
    "CARGO_PROFILE_RELEASE_CODEGEN_UNITS",
)
CONTROL_MAPPING = {
    "aa": "baseline library vs itself; every AA interval must lie wholly in [-1%,+1%]",
    "ff-rgba16-1024": "untouched layout; must be equivalent within 1%",
    "ff-rgb8-1024": "untouched layout; must be equivalent within 1%",
    "ff-decode-1024/oldaa": "unrelated op, same library; must be equivalent within 1%",
    "ff-decode-1024/newdecode": "unrelated op, candidate library; 'unchanged' claim must hold within 1%",
    "primary": "ff-rgba8-{64,256,1024,4096,63x65,4097x3} require improvement beyond 1%; ff-rgba8-1x1 requires non-inferiority at 1%",
}


def fail(msg):
    print(f"ff-bench: {msg}", file=sys.stderr)
    sys.exit(2)


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def capture(cmd, cwd=None, env=None):
    return subprocess.check_output(cmd, cwd=cwd, env=env, text=True).strip()


def run_logged(cmd, log, cwd=None, env=None):
    with open(log, "w") as lf:
        return subprocess.call(cmd, cwd=cwd, env=env, stdout=lf, stderr=subprocess.STDOUT)


def resolve(repo, rev):
    return capture(["git", "-C", repo, "rev-parse", rev])


def archive(repo, sha, dest):
    dest.mkdir(parents=True)
    data = subprocess.check_output(["git", "-C", repo, "archive", sha])
    import io

    with tarfile.open(fileobj=io.BytesIO(data)) as tf:
        tf.extractall(dest, filter="data")


def rename_old_package(manifest):
    text = manifest.read_text()
    parsed = tomllib.loads(text)
    if parsed.get("package", {}).get("name") != "zenbitmaps":
        fail(f"{manifest}: baseline package is not named zenbitmaps")
    manifest.write_text(text.replace('name = "zenbitmaps"', 'name = "zenbitmaps_old"', 1))


def tool_commit(repo):
    """Strict provenance: resolve the commit carrying tools/ff-bench and
    require every tool source file to match it byte-for-byte. Fails on a
    dirty or uncommitted tool tree rather than recording a best-effort id."""
    try:
        meta = capture(
            ["jj", "-R", repo, "log", "-r", "@-", "--no-graph",
             "-T", 'commit_id ++ " " ++ change_id']
        )
        commit = meta.split()[0]
    except Exception:
        commit = resolve(repo, "HEAD")
        meta = commit
    for rel in TOOL_FILES:
        repo_rel = f"tools/ff-bench/{rel}"
        committed = subprocess.run(
            ["git", "-C", repo, "show", f"{commit}:{repo_rel}"],
            capture_output=True,
        )
        if committed.returncode != 0:
            fail(f"tool file {repo_rel} not in commit {commit}; commit it first")
        if committed.stdout != (TOOL / rel).read_bytes():
            fail(f"tool file {repo_rel} differs from commit {commit}; dirty tree")
    return meta


def env_record():
    rec = {}
    for k in ENV_RECORDED:
        if k in os.environ:
            rec[k] = os.environ[k]
            low = os.environ[k].lower()
            if "target-cpu=native" in low or "target-feature" in low:
                fail(f"{k} forces target features; production defaults required")
    return rec


def cargo_configs(harness):
    """Record every cargo config that can affect builds from `harness`:
    `<ancestor>/.cargo/config[.toml]` for each ancestor plus
    `$CARGO_HOME/config[.toml]`, deduplicated."""
    cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    candidates = [
        c
        for d in (harness, *harness.parents)
        for c in (d / ".cargo" / "config.toml", d / ".cargo" / "config")
    ]
    candidates += [cargo_home / "config.toml", cargo_home / "config"]
    found = []
    seen = set()
    for c in candidates:
        c = c.resolve()
        if c.is_file() and c not in seen:
            seen.add(c)
            found.append({"path": str(c), "sha256": sha256(c)})
    return found


def lock_version(lock_path, package):
    data = tomllib.loads(lock_path.read_text())
    for p in data.get("package", []):
        if p.get("name") == package:
            return p.get("version")
    return None


def validate_bench_args(args):
    for a in args:
        ok = False
        if a.startswith("--cpu-set="):
            v = a.removeprefix("--cpu-set=")
            ok = bool(v) and all(ch.isdigit() or ch in ",-" for ch in v)
        elif a.startswith("--cpu-policy="):
            ok = a.removeprefix("--cpu-policy=") in BENCH_ARG_CPU_POLICY_VALUES
        if not ok:
            fail(f"--bench-arg {a!r} is not a whitelisted placement flag")


def main():
    if sys.version_info < (3, 12):
        fail("Python 3.12 or newer is required (tarfile data filter)")
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--scratch", required=True, help="new directory for generated files")
    ap.add_argument("--baseline", default=BASELINE, help=f"baseline revision (default {BASELINE[:12]} = main+F6)")
    ap.add_argument("--candidate", required=True, help="candidate revision (e.g. the row stack tip)")
    ap.add_argument("--verify", action="store_true", help="run the out-of-timer correctness bin")
    ap.add_argument("--dry-run", action="store_true", help="resolve the Confirm plan; no timing")
    ap.add_argument("--offline", action="store_true", help="pass --offline to cargo")
    ap.add_argument("--bench-arg", action="append", default=[],
                    help="whitelisted placement flag for the bench binary "
                         "(repeatable): --cpu-set=<ids> or "
                         "--cpu-policy=auto|performance. Anything else exits 2.")
    args = ap.parse_args()
    # Refuse non-whitelisted flags before ANY filesystem mutation.
    validate_bench_args(args.bench_arg)

    scratch = Path(args.scratch)
    if any(str(scratch).startswith(p) for p in REFUSED_PREFIXES):
        fail(f"scratch {scratch} is on a refused prefix")
    if scratch.exists() and any(scratch.iterdir()):
        fail(f"scratch {scratch} exists and is not empty")

    repo = Path(capture(["git", "-C", TOOL, "rev-parse", "--show-toplevel"])).resolve()
    base_sha = resolve(repo, args.baseline)
    cand_sha = resolve(repo, args.candidate)
    print(f"baseline  {args.baseline} -> {base_sha}")
    print(f"candidate {args.candidate} -> {cand_sha}")

    scratch.mkdir(parents=True)
    logs = scratch / "logs"
    logs.mkdir()
    archive(repo, base_sha, scratch / "old")
    archive(repo, cand_sha, scratch / "new")
    rename_old_package(scratch / "old" / "Cargo.toml")

    harness = scratch / "harness"
    (harness / "src" / "bin").mkdir(parents=True)
    (harness / "benches").mkdir()
    shutil.copyfile(TOOL / "Cargo.toml.in", harness / "Cargo.toml")
    shutil.copyfile(TOOL / "harness" / "lib.rs", harness / "src" / "common.rs")
    shutil.copyfile(TOOL / "harness" / "verify.rs", harness / "src" / "bin" / "verify.rs")
    shutil.copyfile(TOOL / "harness" / "ff.rs", harness / "benches" / "ff.rs")
    (harness / "src" / "revisions.rs").write_text(
        "//! Generated by tools/ff-bench/run.py — resolved source identities.\n"
        f'pub(crate) const BASELINE: &str = "{base_sha}";\n'
        f'pub(crate) const CANDIDATE: &str = "{cand_sha}";\n'
    )
    lock = scratch / "new" / "Cargo.lock"
    if not lock.is_file():
        fail(f"candidate {cand_sha} has no Cargo.lock to pin shared dependencies")
    shutil.copyfile(lock, harness / "Cargo.lock")
    artifacts = scratch / "artifacts"

    identity = {
        "utc": datetime.datetime.now(datetime.UTC).isoformat(timespec="seconds"),
        "argv": sys.argv,
        "baseline": {"declared": args.baseline, "commit": base_sha},
        "candidate": {"declared": args.candidate, "commit": cand_sha},
        "workload_id": WORKLOAD_ID,
        "control_mapping": CONTROL_MAPPING,
        "encoder_sha256": {
            "baseline": sha256(scratch / "old" / "src/farbfeld/encode.rs"),
            "candidate": sha256(scratch / "new" / "src/farbfeld/encode.rs"),
        },
        "tool_sha256": {p: sha256(TOOL / p) for p in TOOL_FILES},
        "tool_commit": tool_commit(repo),
        "host": {"machine": platform.machine(), "system": platform.system()},
        "rustc": capture(["rustc", "-vV"]),
        "cargo": capture(["cargo", "-V"]),
        "build_env": env_record(),
        "cargo_configs": cargo_configs(harness),
        "actions": {},
    }

    offline = ["--offline"] if args.offline else []
    # The bench binary and verify export unique inputs/outputs here
    # (SHA256 CAS, dedup across the five fresh children). Inherited by
    # every subprocess including zenbench's re-exec'd confirm children.
    os.environ["FF_BENCH_ARTIFACT_DIR"] = str(artifacts)
    identity["artifact_env"] = "FF_BENCH_ARTIFACT_DIR"
    identity["artifact_dir"] = str(artifacts)

    def cargo(args_, log):
        # --offline must precede the bench-args "--" separator. Builds run
        # with cwd=harness so the primary repo's .cargo config cannot leak in.
        if "--" in args_:
            i = args_.index("--")
            args_ = args_[:i] + offline + args_[i:]
        else:
            args_ = args_ + offline
        return run_logged(["cargo"] + args_, log, cwd=harness)

    # First resolution writes harness/Cargo.lock (candidate lock pins the
    # shared deps; zenbench/sha2 resolve here). Record the resolved lock and
    # dependency identities, then pin every later cargo call with --locked.
    try:
        meta_text = capture(
            ["cargo", "metadata", "--format-version=1", "--manifest-path",
             "Cargo.toml"] + offline,
            cwd=harness,
        )
    except subprocess.CalledProcessError as e:
        (logs / "metadata.log").write_text(f"{e}\n{e.output or ''}")
        identity["actions"]["metadata"] = "FAIL"
        (scratch / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
        print(f"metadata failed: see {logs / 'metadata.log'}")
        sys.exit(1)
    meta = json.loads(meta_text)
    meta_path = scratch / "metadata.json"
    meta_path.write_text(json.dumps(meta, indent=1) + "\n")
    identity["metadata_json_sha256"] = sha256(meta_path)
    identity["dependencies"] = sorted(
        [
            {"name": p["name"], "version": p["version"], "source": p.get("source")}
            for p in meta["packages"]
        ],
        key=lambda d: (d["name"], d["version"]),
    )
    identity["harness_lock_sha256"] = sha256(harness / "Cargo.lock")

    if cargo(["build", "--manifest-path", "Cargo.toml", "--target-dir",
              str(scratch / "target"), "--release", "--locked"],
             logs / "build.log") != 0:
        identity["actions"]["build"] = "FAIL"
        (scratch / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
        print(f"build failed: see {logs / 'build.log'}")
        sys.exit(1)
    if cargo(["bench", "--manifest-path", "Cargo.toml", "--target-dir",
              str(scratch / "target"), "--bench", "ff", "--no-run", "--locked"],
             logs / "build-bench.log") != 0:
        identity["actions"]["build"] = "FAIL(bench)"
        (scratch / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
        print(f"bench build failed: see {logs / 'build-bench.log'}")
        sys.exit(1)
    identity["actions"]["build"] = "ok"

    target = scratch / "target"
    verify_exe = target / "release" / "verify"
    bench_exes = sorted(
        p for p in (target / "release" / "deps").glob("ff-*")
        if p.is_file() and p.stat().st_mode & 0o111
    )
    if not verify_exe.is_file():
        fail(f"verify executable missing: {verify_exe}")
    if len(bench_exes) != 1:
        fail(f"expected exactly one ff bench executable, found {len(bench_exes)}")
    identity["binaries_sha256"] = {
        "verify": sha256(verify_exe),
        "ff_bench": {bench_exes[0].name: sha256(bench_exes[0])},
    }

    if args.verify:
        rc = run_logged([verify_exe], logs / "verify.log")
        identity["actions"]["verify"] = "PASS" if rc == 0 else "FAIL"
        print(f"verify: {identity['actions']['verify']} ({logs / 'verify.log'})")
        if rc != 0:
            (scratch / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
            sys.exit(1)
        if artifacts.is_dir():
            identity["artifacts"] = {
                "dir": str(artifacts),
                "files": sorted(p.name for p in artifacts.iterdir()),
            }

    if args.dry_run:
        rc = cargo(
            ["bench", "--manifest-path", "Cargo.toml", "--target-dir",
             str(target), "--bench", "ff", "--locked", "--",
             "--mode=confirm", f"--workload-id={WORKLOAD_ID}",
             "--processes=5", "--rounds=30", "--margin=1", "--confidence=0.95",
             "--dry-run"] + list(args.bench_arg),
            logs / "dry-run.log",
        )
        identity["actions"]["dry_run"] = "ok" if rc == 0 else f"rc={rc}"
        print(f"dry-run rc={rc} ({logs / 'dry-run.log'})")

    identity["zenbench_version"] = lock_version(harness / "Cargo.lock", "zenbench")
    (scratch / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    print(f"identity: {scratch / 'identity.json'}")
    if any(value not in ("ok", "PASS") for value in identity["actions"].values()):
        sys.exit(1)


if __name__ == "__main__":
    main()
