#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""Regression tests for ``python/optresults_handler.py`` (read-only CSV access).

Runs with the standard library only: numpy/pandas/matplotlib are replaced by tiny
fakes before the module is imported.

Run directly:  python python/tests/test_optresults_handler.py
Under pytest:  pytest python/tests/test_optresults_handler.py
"""

import ast
import builtins
import importlib
import os
import stat
import sys
import tempfile
import types

HERE = os.path.dirname(os.path.abspath(__file__))
PYTHON_DIR = os.path.dirname(HERE)
if PYTHON_DIR not in sys.path:
    sys.path.insert(0, PYTHON_DIR)

CSV_TEXT = ("strategy_name;number_of_generation;best_individ;mean;best_hromosome_ID\n"
            "MA_cross_1;0;12.5;11.0;id_0\n")


class FakeFrame(object):
    """Sentinel returned by the fake ``pandas.read_csv``."""

    def __init__(self, content, kwargs):
        self.content = content
        self.kwargs = kwargs


def _load_handler():
    """Import optresults_handler against fakes; returns (module, recorded read_csv calls)."""
    calls = []

    def read_csv(fin, **kwargs):
        calls.append({"mode": getattr(fin, "mode", None),
                      "name": getattr(fin, "name", None),
                      "content": fin.read(),
                      "kwargs": kwargs})
        return FakeFrame(calls[-1]["content"], kwargs)

    pandas = types.ModuleType("pandas")
    pandas.read_csv = read_csv
    numpy = types.ModuleType("numpy")
    pyplot = types.ModuleType("matplotlib.pyplot")
    matplotlib = types.ModuleType("matplotlib")
    matplotlib.pyplot = pyplot
    sys.modules["pandas"] = pandas
    sys.modules["numpy"] = numpy
    sys.modules["matplotlib"] = matplotlib
    sys.modules["matplotlib.pyplot"] = pyplot
    sys.modules.pop("optresults_handler", None)
    module = importlib.import_module("optresults_handler")
    return module, calls


def _write_results(directory):
    path = os.path.join(directory, "optimization_results.csv")
    with open(path, "w", newline="") as fout:
        fout.write(CSV_TEXT)
    return path


def test_read_optimization_csv_opens_read_only():
    """Regression: the CSV must be opened with mode 'r', never 'r+'."""
    module, calls = _load_handler()
    modes = []
    real_open = builtins.open

    def recording_open(file, mode="r", *args, **kwargs):
        modes.append(mode)
        return real_open(file, mode, *args, **kwargs)

    with tempfile.TemporaryDirectory() as folder:
        path = _write_results(folder)
        builtins.open = recording_open
        try:
            frame = module.read_optimization_csv(path)
        finally:
            builtins.open = real_open

    assert modes == ["r"], "unexpected open modes: %r" % (modes,)
    assert isinstance(frame, FakeFrame)
    assert frame.kwargs == {"header": 0, "sep": ";"}
    assert frame.content.startswith("strategy_name;")


def test_read_optimization_csv_works_on_read_only_file():
    """A genuinely read-only results file (mode 0444) must still be readable."""
    module, _ = _load_handler()
    with tempfile.TemporaryDirectory() as folder:
        path = _write_results(folder)
        os.chmod(path, stat.S_IRUSR | stat.S_IRGRP | stat.S_IROTH)
        try:
            frame = module.read_optimization_csv(path)
        finally:
            os.chmod(path, stat.S_IWUSR | stat.S_IRUSR)
    assert isinstance(frame, FakeFrame)


def test_read_optimization_csv_needs_no_write_access():
    """Simulate a read-only file system: any non-'r' open raises PermissionError."""
    module, _ = _load_handler()
    attempted = []
    real_open = builtins.open

    def read_only_open(file, mode="r", *args, **kwargs):
        attempted.append(mode)
        if mode != "r":
            raise PermissionError("read-only file system (simulated)")
        return real_open(file, mode, *args, **kwargs)

    with tempfile.TemporaryDirectory() as folder:
        path = _write_results(folder)
        builtins.open = read_only_open
        try:
            frame = module.read_optimization_csv(path)
        finally:
            builtins.open = real_open

    assert attempted == ["r"]
    assert isinstance(frame, FakeFrame)


def test_no_write_mode_open_calls_in_module():
    """Vaccine: no ``open(..., 'r+'/'w'/'a')`` may creep back into the module."""
    module, _ = _load_handler()
    with open(module.__file__, "r", encoding="utf-8") as fin:
        tree = ast.parse(fin.read())

    offenders = []
    for node in ast.walk(tree):
        if not (isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                and node.func.id == "open"):
            continue
        mode = None
        if len(node.args) > 1 and isinstance(node.args[1], ast.Constant):
            mode = node.args[1].value
        for keyword in node.keywords:
            if keyword.arg == "mode" and isinstance(keyword.value, ast.Constant):
                mode = keyword.value.value
        if isinstance(mode, str) and any(flag in mode for flag in "+wa"):
            offenders.append((node.lineno, mode))

    assert not offenders, "write-mode open() calls found: %r" % (offenders,)


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
