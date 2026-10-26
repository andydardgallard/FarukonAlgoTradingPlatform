#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""Regression tests for the instrument-date memoization in ``Strategies/rs/MA_cross.rs``.

``Strategies/rs/MA_cross.rs`` is the source of truth of the sample MA-cross strategy, but no
crate in the workspace builds it, so a change to it is invisible to ``cargo test``. This test
pins the change two ways:

  * it *compiles that exact source file* with ``rustc`` (type-check only, ``--emit=metadata``)
    against the ``farukon_core`` rlib already built in ``target/release/deps``, so a change
    that no longer matches the host ABI fails here instead of at runtime;
  * it statically asserts the memoization contract: the per-symbol ``expiration_date`` /
    ``trade_from_date`` strings are parsed once inside ``new()`` (or a constructor helper),
    ``calculate_signals`` never calls ``string_to_date_time``, and a malformed date is reported
    with an error naming the symbol.

The compile step needs ``rustc`` plus built rlibs; when either is unavailable the test reports
a clear SKIP instead of failing (a fresh checkout has no ``target/``).

Run directly:  python python/tests/test_ma_cross_memoization.py
Under pytest:  pytest python/tests/test_ma_cross_memoization.py
"""

import glob
import os
import re
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
PYTHON_DIR = os.path.dirname(HERE)
REPO_ROOT = os.path.dirname(PYTHON_DIR)

MA_CROSS = os.path.join(REPO_ROOT, "Strategies", "rs", "MA_cross.rs")
WORKSPACE_CARGO_TOML = os.path.join(REPO_ROOT, "Cargo.toml")
RELEASE_DEPS = os.path.join(REPO_ROOT, "target", "release", "deps")

# Signature markers used to slice the source into the two functions under test.
NEW_MARKER = "pub fn new("
CALCULATE_SIGNALS_MARKER = "fn calculate_signals("

EXPIRATION_CONTEXT = "Failed to parse 'expiration_date' for symbol '{}'"
TRADE_FROM_CONTEXT = "Failed to parse 'trade_from_date' for symbol '{}'"


def _read(path):
    with open(path, "r", encoding="utf-8") as fin:
        return fin.read()


def _function_body(source, marker):
    """Return the brace-balanced body of the first function starting at ``marker``."""
    start = source.find(marker)
    assert start >= 0, "function %r not found in MA_cross.rs" % marker
    brace = source.find("{", start)
    assert brace >= 0, "no body found for %r" % marker
    depth = 0
    for pos in range(brace, len(source)):
        char = source[pos]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return source[brace:pos + 1]
    raise AssertionError("unbalanced braces in the body of %r" % marker)


def _constructor_body(source):
    """Return the body of ``new()``.

    The memoization may live in ``new()`` itself or in a helper it calls; in the helper case the
    helper's body is used, because that is where the per-symbol parse has to happen.
    """
    body = _function_body(source, NEW_MARKER)
    if "string_to_date_time" in body:
        return body
    helper_marker = "fn parse_symbol_dates"
    if helper_marker in source:
        return _function_body(source, helper_marker)
    return body


def test_strategy_struct_memoizes_parsed_dates():
    """The struct carries one pre-parsed (expiration_date, trade_from_date) pair per symbol."""
    source = _read(MA_CROSS)
    assert "parsed_symbol_dates" in source, \
        "MA_cross.rs must memoize the parsed instrument dates in a field"
    match = re.search(r"parsed_symbol_dates\s*:\s*std::collections::HashMap<", source)
    assert match, "parsed_symbol_dates must be a HashMap keyed by the symbol name"
    declaration_end = source.find(">,", match.start())
    declaration = source[match.start():declaration_end]
    assert "chrono::DateTime" in declaration, \
        "the memoized value must hold chrono datetimes, not re-parsed strings"


def test_constructor_parses_instrument_dates_once():
    """`new()` parses both instrument dates once per symbol and names the symbol on failure."""
    source = _read(MA_CROSS)
    body = _constructor_body(source)
    assert body.count("string_to_date_time") == 2, \
        "new() must parse exactly two fields per symbol (expiration_date, trade_from_date); " \
        "found %d calls" % body.count("string_to_date_time")
    assert "expiration_date" in body and "trade_from_date" in body
    assert EXPIRATION_CONTEXT in body, \
        "a malformed expiration_date must produce an error naming the symbol"
    assert TRADE_FROM_CONTEXT in body, \
        "a malformed trade_from_date must produce an error naming the symbol"
    # The memoized map has to be handed to the constructed instance, otherwise it is unused.
    assert "parsed_symbol_dates" in body


def test_calculate_signals_never_reparses_dates():
    """The hot per-bar path reads the memoized dates and contains no chrono parse call."""
    source = _read(MA_CROSS)
    body = _function_body(source, CALCULATE_SIGNALS_MARKER)
    assert "string_to_date_time" not in body, \
        "calculate_signals must not parse instrument dates per symbol per bar"
    assert "parsed_symbol_dates" in body, \
        "calculate_signals must read the dates memoized in new()"
    assert "get(symbol)" in body, \
        "the memoized lookup must be keyed by the symbol"
    # Guard against a silent re-parse through the raw metadata strings.
    for field in ("expiration_date", "trade_from_date"):
        assert "instrument_info.%s" % field not in body, \
            "calculate_signals must not touch instrument_info.%s" % field


def _workspace_edition():
    """Edition of the workspace, read from Cargo.toml (fallback: 2024)."""
    try:
        text = _read(WORKSPACE_CARGO_TOML)
    except OSError:
        return "2024"
    match = re.search(r'^\s*edition\s*=\s*"([^"]+)"', text, re.MULTILINE)
    return match.group(1) if match else "2024"


def _newest(directory, pattern):
    """Newest file matching ``pattern`` inside ``directory`` (hashed names), or None."""
    matches = [path for path in glob.glob(os.path.join(directory, pattern))
               if os.path.isfile(path)]
    if not matches:
        return None
    return max(matches, key=os.path.getmtime)


def _crate_rlib(directory, crate):
    """Locate ``lib<crate>-<hash>.rlib`` in ``directory``."""
    return _newest(directory, "lib%s-*.rlib" % crate)


def test_source_compiles_against_built_farukon_core():
    """rustc type-checks the exact MA_cross.rs source against the workspace's farukon_core."""
    rustc = shutil.which("rustc")
    if rustc is None:
        print("SKIP: rustc is not on PATH - cannot compile Strategies/rs/MA_cross.rs")
        return
    if not os.path.isdir(RELEASE_DEPS):
        print("SKIP: %s is missing - run `cargo build --release` first" % RELEASE_DEPS)
        return

    externs = {}
    for crate in ("farukon_core", "anyhow", "chrono"):
        rlib = _crate_rlib(RELEASE_DEPS, crate)
        if rlib is None:
            print("SKIP: no built rlib for `%s` in %s - run `cargo build --release` first"
                  % (crate, RELEASE_DEPS))
            return
        externs[crate] = rlib

    out_dir = tempfile.mkdtemp(prefix="ma_cross_compile_")
    try:
        command = [rustc,
                   "--edition=%s" % _workspace_edition(),
                   "--crate-type=cdylib",
                   "--emit=metadata",
                   "-o", os.path.join(out_dir, "ma_cross.rmeta"),
                   "-L", "dependency=%s" % RELEASE_DEPS]
        for crate, rlib in externs.items():
            command += ["--extern", "%s=%s" % (crate, rlib)]
        command.append(MA_CROSS)
        result = subprocess.run(command, cwd=REPO_ROOT, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, universal_newlines=True)
        output = result.stdout or ""
        assert result.returncode == 0, \
            "Strategies/rs/MA_cross.rs does not compile against farukon_core:\n%s" % output
        assert "error[" not in output, \
            "rustc reported errors for Strategies/rs/MA_cross.rs:\n%s" % output
    finally:
        shutil.rmtree(out_dir, ignore_errors=True)


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
