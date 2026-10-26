#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""Regression tests for the platform selection in ``start.sh``.

``start.sh`` builds the sample strategy and then runs a portfolio config. Cargo names the
cdylib differently per platform (``libstrategy_lib.so`` on Linux, ``libstrategy_lib.dylib`` on
macOS, ``strategy_lib.dll`` on Windows), while the committed portfolio JSONs carry the Windows
path. ``start.sh`` bridges the two by patching the ``strategy_path`` values of a throwaway copy
next to the committed config and deleting that copy on exit.

These tests run the real script in a sandbox with stub ``uname`` / ``cargo`` on ``PATH`` (and a
``sed`` stub that forwards to the real ``sed``), so no build, no optimizer run and no change to
the committed portfolio files happens. They assert, for Linux / Darwin / MINGW:

  * the exit status is 0 and the runtime config that was handed to ``cargo run`` points at
    ``target/release/libstrategy_lib.so`` / ``libstrategy_lib.dylib`` / ``strategy_lib.dll``;
  * the rest of the runtime config is byte-identical to the committed JSON;
  * the committed ``portfolios/lshade_si_25_apr_dd.json`` is unchanged (SHA-256);
  * the runtime config is removed again on exit;
  * a build that produces no library exits 1 and leaves no runtime config behind.

``bash`` (or ``sed``) may be absent on the machine - the test then reports a clear SKIP.

Run directly:  python python/tests/test_start_sh_platforms.py
Under pytest:  pytest python/tests/test_start_sh_platforms.py
"""

import glob
import hashlib
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
PYTHON_DIR = os.path.dirname(HERE)
REPO_ROOT = os.path.dirname(PYTHON_DIR)

START_SH = os.path.join(REPO_ROOT, "start.sh")
COMMITTED_PORTFOLIO = os.path.join(REPO_ROOT, "portfolios", "lshade_si_25_apr_dd.json")
PORTFOLIO_NAME = "lshade_si_25_apr_dd.json"
RUNTIME_CONFIG_NAME = ".runtime_" + PORTFOLIO_NAME

UNAME_STUB = """#!/usr/bin/env bash
# Stub `uname`: reports the platform start.sh is being tested against.
printf '%s\\n' "$STUB_UNAME"
"""

CARGO_STUB = """#!/usr/bin/env bash
# Stub `cargo`: `build` fakes the cdylib, `run` snapshots the --config file it was given.
set -e
if [ "${1:-}" = "build" ]; then
    if [ "${STUB_CREATE_LIB:-1}" = "1" ]; then
        mkdir -p target/release
        : > "target/release/${STUB_LIB}"
    fi
    exit 0
fi
if [ "${1:-}" = "run" ]; then
    printf '%s\\n' "$@" > "$STUB_ARGV"
    shift
    while [ "$#" -gt 0 ]; do
        if [ "$1" = "--config" ]; then
            shift
            cp "$1" "$STUB_CAPTURE"
            exit 0
        fi
        shift
    done
    echo "stub cargo: no --config argument" >&2
    exit 2
fi
echo "stub cargo: unexpected invocation: $*" >&2
exit 2
"""

# `start.sh` needs a real `sed` for the substitution; the stub keeps the invocation visible to
# the test while forwarding to the real binary resolved before PATH was changed.
SED_STUB_TEMPLATE = """#!/usr/bin/env bash
printf '%s\\n' "$@" > "$STUB_SED_ARGV"
exec "__REAL_SED__" "$@"
"""

PLATFORMS = [
    ("MINGW64_NT-10.0-19045", "strategy_lib.dll"),
    ("Darwin", "libstrategy_lib.dylib"),
    ("Linux", "libstrategy_lib.so"),
]


def _sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as fin:
        for chunk in iter(lambda: fin.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _write_executable(path, text):
    with open(path, "w", encoding="utf-8", newline="\n") as fout:
        fout.write(text)
    mode = os.stat(path).st_mode
    os.chmod(path, mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def _make_stub_dir(real_sed):
    stub_dir = tempfile.mkdtemp(prefix="start_sh_stubs_")
    _write_executable(os.path.join(stub_dir, "uname"), UNAME_STUB)
    _write_executable(os.path.join(stub_dir, "cargo"), CARGO_STUB)
    _write_executable(os.path.join(stub_dir, "sed"),
                      SED_STUB_TEMPLATE.replace("__REAL_SED__",
                                                real_sed.replace("\\", "/")))
    return stub_dir


def _make_sandbox():
    """A minimal copy of the repo layout start.sh needs (script + committed portfolio)."""
    sandbox = tempfile.mkdtemp(prefix="start_sh_repo_")
    shutil.copyfile(START_SH, os.path.join(sandbox, "start.sh"))
    os.makedirs(os.path.join(sandbox, "portfolios"))
    shutil.copyfile(COMMITTED_PORTFOLIO,
                    os.path.join(sandbox, "portfolios", PORTFOLIO_NAME))
    return sandbox


def _run_start_sh(platform, strategy_lib, create_lib=True):
    """Run start.sh in a sandbox; returns a dict with status, stdout, stderr and artifacts."""
    bash = shutil.which("bash")
    real_sed = shutil.which("sed")
    if bash is None:
        return None
    if real_sed is None:
        return None

    stub_dir = _make_stub_dir(real_sed)
    sandbox = _make_sandbox()
    capture = os.path.join(stub_dir, "captured_config.json")
    try:
        env = dict(os.environ)
        env["PATH"] = stub_dir + os.pathsep + env.get("PATH", "")
        env["STUB_UNAME"] = platform
        env["STUB_LIB"] = strategy_lib
        env["STUB_CAPTURE"] = capture
        env["STUB_ARGV"] = os.path.join(stub_dir, "cargo_argv.txt")
        env["STUB_SED_ARGV"] = os.path.join(stub_dir, "sed_argv.txt")
        env["STUB_CREATE_LIB"] = "1" if create_lib else "0"
        result = subprocess.run([bash, os.path.join(sandbox, "start.sh")],
                                cwd=sandbox, env=env, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, universal_newlines=True)
        runtime_config = os.path.join(sandbox, "portfolios", RUNTIME_CONFIG_NAME)
        return {
            "returncode": result.returncode,
            "stdout": result.stdout or "",
            "stderr": result.stderr or "",
            "captured": _read_text(capture),
            "cargo_argv": _read_text(os.path.join(stub_dir, "cargo_argv.txt")),
            "sed_argv": _read_text(os.path.join(stub_dir, "sed_argv.txt")),
            "runtime_config": runtime_config,
            "runtime_exists": os.path.isfile(runtime_config),
            "sandbox": sandbox,
        }
    finally:
        # Everything a test asserts on is already copied into the returned dict.
        shutil.rmtree(stub_dir, ignore_errors=True)
        shutil.rmtree(sandbox, ignore_errors=True)


_RUN_CACHE = {}


def _read_text(path):
    try:
        with open(path, "r", encoding="utf-8") as fin:
            return fin.read()
    except OSError:
        return ""


def _run(platform, strategy_lib, create_lib=True):
    key = (platform, strategy_lib, create_lib)
    if key not in _RUN_CACHE:
        _RUN_CACHE[key] = _run_start_sh(platform, strategy_lib, create_lib)
    return _RUN_CACHE[key]


def test_runtime_config_uses_the_platform_library_path():
    """Each platform patches in the file name Cargo actually emits there."""
    committed = json.loads(_read_text(COMMITTED_PORTFOLIO))
    committed_path = committed["portfolio"]["Strategy_1"]["strategy_path"]
    committed_digest = _sha256(COMMITTED_PORTFOLIO)

    for platform, expected_lib in PLATFORMS:
        run = _run(platform, expected_lib)
        if run is None:
            print("SKIP: bash or sed is not available - cannot run start.sh")
            return
        assert run["returncode"] == 0, \
            "%s: start.sh failed (%d): %s" % (platform, run["returncode"], run["stderr"])
        assert run["captured"], \
            "%s: cargo run was never invoked, no runtime config was captured" % platform
        configured = json.loads(run["captured"])
        strategy_path = configured["portfolio"]["Strategy_1"]["strategy_path"]
        expected_path = "target/release/" + expected_lib
        assert strategy_path == expected_path, \
            "%s: strategy_path %r, expected %r" % (platform, strategy_path, expected_path)
        assert configured["portfolio"]["Strategy_1"]["strategy_path"] != committed_path \
            or expected_lib == "strategy_lib.dll", \
            "%s: the Windows path was not replaced" % platform

        # Only strategy_path may differ: data paths and every other key stay untouched.
        expected_config = json.loads(_read_text(COMMITTED_PORTFOLIO))
        expected_config["portfolio"]["Strategy_1"]["strategy_path"] = expected_path
        assert configured == expected_config, \
            "%s: start.sh changed more than strategy_path" % platform

        assert run["runtime_exists"] is False, \
            "%s: the runtime config was not cleaned up" % platform
        assert os.path.dirname(run["runtime_config"]).endswith("portfolios")
        assert _sha256(COMMITTED_PORTFOLIO) == committed_digest, \
            "%s: the committed portfolio config was modified" % platform


def test_runtime_config_is_written_next_to_the_committed_one():
    """The patched copy lives at portfolios/.runtime_<name> so relative paths still resolve."""
    run = _run("Linux", "libstrategy_lib.so")
    if run is None:
        print("SKIP: bash or sed is not available - cannot run start.sh")
        return
    assert os.path.basename(run["runtime_config"]) == RUNTIME_CONFIG_NAME
    # sed was invoked with the strategy_path substitution on the committed config.
    sed_argv = run["sed_argv"]
    assert "strategy_path" in sed_argv, "start.sh must patch the strategy_path values"
    argv_lines = run["cargo_argv"].splitlines()
    assert argv_lines and argv_lines[0] == "run", \
        "cargo run must receive the patched runtime config, got %r" % (argv_lines,)
    assert "--release" in argv_lines and "--config" in argv_lines
    assert argv_lines[argv_lines.index("--config") + 1].endswith(RUNTIME_CONFIG_NAME)


def test_missing_library_exits_1_and_leaves_no_runtime_config():
    """A build that produces no library must fail loudly before the optimizer runs."""
    run = _run("Linux", "libstrategy_lib.so", create_lib=False)
    if run is None:
        print("SKIP: bash or sed is not available - cannot run start.sh")
        return
    assert run["returncode"] == 1, \
        "expected exit 1 for a missing library, got %d" % run["returncode"]
    assert "target/release/libstrategy_lib.so" in run["stderr"], \
        "the error message must name the missing library: %r" % run["stderr"]
    assert run["captured"] == "", "cargo run must not be reached when the library is missing"
    assert run["runtime_exists"] is False, "no runtime config may be left behind"


def test_committed_portfolio_configs_are_untouched():
    """No run may create a leftover .runtime_* file in the real portfolios directory."""
    leftovers = glob.glob(os.path.join(REPO_ROOT, "portfolios", ".runtime_*"))
    assert not leftovers, "start.sh left runtime configs behind: %r" % (leftovers,)
    assert os.path.isfile(COMMITTED_PORTFOLIO), "the committed portfolio config is missing"


def _run_all():
    failures = 0
    for name in sorted(globals()):
        func = globals()[name]
        if name.startswith("test_") and callable(func):
            try:
                func()
                print("PASS %s" % name)
            except Exception as exc:  # noqa: BLE001 - test runner
                failures += 1
                print("FAIL %s -> %r" % (name, exc))
    print("%d failed" % failures)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(_run_all())
