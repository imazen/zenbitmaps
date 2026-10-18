#!/usr/bin/env python3
"""Integration check: failed verification/dry-run must make run.py fail.

Real staging and dependency metadata run; controlled executable fixtures replace
compilation. These are error-propagation tests, not codec or performance evidence.
All generated inputs, logs and identities remain in the caller's new scratch dir.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import sys

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--scratch", required=True)
args = parser.parse_args()
root = Path(args.scratch).resolve()
if root.exists():
    parser.error("scratch must be a new directory")
if any(root.is_relative_to(p) for p in [Path("/tmp"), Path("/run"), Path("/dev/shm")]):
    parser.error("scratch must be disk-backed outside refused directories")
root.mkdir(parents=True)

for scenario in ["verify-failure", "dryrun-failure"]:
    spec = importlib.util.spec_from_file_location("ff_driver", Path(__file__).with_name("run.py"))
    driver = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(driver)
    scratch = root / scenario
    original = driver.run_logged

    def controlled(cmd, log, cwd=None, env=None):
        argv = [str(a) for a in cmd]
        if argv[0] == "cargo":
            if argv[1] == "build":
                target = Path(argv[argv.index("--target-dir") + 1]) / "release"
                (target / "deps").mkdir(parents=True)
                verify = target / "verify"
                code = 7 if scenario == "verify-failure" else 0
                verify.write_text(f"#!{sys.executable}\nraise SystemExit({code})\n")
                verify.chmod(0o755)
                bench = target / "deps" / "ff-controlled-fixture"
                bench.write_text(f"#!{sys.executable}\nraise SystemExit(0)\n")
                bench.chmod(0o755)
                Path(log).write_text("Controlled compilation fixture; not a Rust build.\n")
                return 0
            if argv[1] == "bench" and "--no-run" in argv:
                Path(log).write_text("Controlled compilation fixture; not a Rust build.\n")
                return 0
            if argv[1] == "bench" and "--dry-run" in argv:
                (scratch / "dryrun-entered").write_text("controlled failing dry-run\n")
                Path(log).write_text("Controlled dry-run failure: exit 9.\n")
                return 9
        return original(cmd, log, cwd=cwd, env=env)

    driver.run_logged = controlled
    sys.argv = [str(Path(__file__).with_name("run.py")), "--scratch", str(scratch),
                "--baseline", driver.BASELINE, "--candidate", driver.BASELINE,
                "--verify", "--dry-run", "--offline"]
    status = 0
    try:
        driver.main()
    except SystemExit as error:
        status = error.code
    assert status != 0, f"{scenario}: driver accepted a failed action"
    identity = json.loads((scratch / "identity.json").read_text())
    if scenario == "verify-failure":
        assert identity["actions"]["verify"] == "FAIL"
        assert not (scratch / "dryrun-entered").exists(), "driver continued after failed verification"
    else:
        assert identity["actions"]["verify"] == "PASS"
        assert identity["actions"]["dry_run"] == "rc=9"
    print(f"{scenario}: PASS (driver rejected the controlled failure)")
