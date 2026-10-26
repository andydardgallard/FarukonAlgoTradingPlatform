#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""Regression tests for the plotting/metric-selection fixes in ``python/``.

Covers:
  * C18 - ``optresults_handler.COMPARE_METRIC_COLUMNS`` is a metric whitelist, so strategy
    parameters (``short_window``, ``long_window``, ...) are never treated as metrics.
  * C19 - ``plot_portfolio_lshade_results`` writes one file per strategy, and the file name
    carries a digest of the original strategy name so sanitized-name collisions cannot
    overwrite each other.
  * C20 - cumulative returns are compounded (``capital / capital.iloc[0] - 1``), not summed
    per-bar percentage changes.
  * C21 - ``visual.plot_graphs`` opens the equity CSV read-only.
  * C27 - the absolute drawdown subplot is labeled ``Drawdown``, not a percentage.

Runs with the standard library only: numpy/pandas/matplotlib are replaced by tiny fakes
before the module is imported (``plot_graphs`` raises a sentinel from the fake
``pandas.read_csv``, so nothing past the ``open`` call is executed).

Run directly:  python python/tests/test_plot_script_fixes.py
Under pytest:  pytest python/tests/test_plot_script_fixes.py
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

VISUAL = os.path.join(PYTHON_DIR, "visual.py")
PERFORMANCE = os.path.join(PYTHON_DIR, "plot_portfolio_performance.py")
LSHADE = os.path.join(PYTHON_DIR, "plot_portfolio_lshade_results.py")

EXPECTED_METRICS = {
    "Total_Return",
    "Total_Return_%",
    "APR",
    "Max_Drawdown",
    "Max_Drawdown_DateTime",
    "Max_Drawdown_pct",
    "APR/Drawdown_factor",
    "Recovery_Factor",
    "Composite",
    "Deals_Count",
}

STRATEGY_PARAMETERS = ["short_window", "long_window", "sma_period", "channel_period",
                       "prct_width_channel", "width_channel", "avg_price_period",
                       "pos_sizer_name", "pos_sizer_value", "slippage"]


class _StopPlotting(Exception):
    """Sentinel raised by the fake ``pandas.read_csv`` to stop ``plot_graphs`` early."""


def _install_fakes(read_csv):
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


def _load(name):
    """Import a script module against fakes (pandas/numpy/matplotlib are not installed)."""
    _install_fakes(lambda fin, **kwargs: _StopPlotting())
    sys.modules.pop(name, None)
    return importlib.import_module(name)


class _FakeMask:
    """Boolean row mask returned by ``<frame>[column] == value``."""

    def __init__(self, flags):
        self.flags = list(flags)


class _FakeColumn:
    """Minimal column: iterable (``set(df[col])``) and comparable (``df[col] == v``)."""

    def __init__(self, values):
        self._values = list(values)

    def __eq__(self, other):
        return _FakeMask([value == other for value in self._values])

    def __iter__(self):
        return iter(self._values)


class _FakeFrame:
    """Just enough of ``pandas.DataFrame`` for ``plot_portfolio_lshade_results``."""

    def __init__(self, columns):
        self._columns = {name: list(values) for name, values in columns.items()}

    def __getitem__(self, key):
        if isinstance(key, _FakeMask):
            rows = [i for i, flag in enumerate(key.flags) if flag]
            return _FakeFrame({
                name: [values[i] for i in rows]
                for name, values in self._columns.items()
            })
        return _FakeColumn(self._columns[key])

    def reset_index(self, drop=False):
        return self


class _FakeAxis:
    def plot(self, *args, **kwargs):
        return []

    def set_xlabel(self, *args, **kwargs):
        return None

    def set_ylabel(self, *args, **kwargs):
        return None

    def set_title(self, *args, **kwargs):
        return None

    def legend(self, *args, **kwargs):
        return None

    def grid(self, *args, **kwargs):
        return None


class _FakeFigure:
    def __init__(self, saved):
        self._saved = saved

    def suptitle(self, *args, **kwargs):
        return None

    def subplots_adjust(self, *args, **kwargs):
        return None

    def savefig(self, path, **kwargs):
        self._saved.append(path)


def _install_plot_fakes(frame, saved):
    """Fakes whose ``savefig`` records the output paths instead of writing an image."""
    pyplot = types.ModuleType("matplotlib.pyplot")
    figure = _FakeFigure(saved)
    pyplot.subplots = lambda *args, **kwargs: (figure, (_FakeAxis(), _FakeAxis()))
    pyplot.show = lambda *args, **kwargs: None
    pandas = types.ModuleType("pandas")
    pandas.read_csv = lambda fin, **kwargs: frame
    matplotlib = types.ModuleType("matplotlib")
    matplotlib.pyplot = pyplot
    sys.modules["pandas"] = pandas
    sys.modules["numpy"] = types.ModuleType("numpy")
    sys.modules["matplotlib"] = matplotlib
    sys.modules["matplotlib.pyplot"] = pyplot
    return pyplot


def _tree(path):
    with open(path, "r", encoding="utf-8") as fin:
        return ast.parse(fin.read(), filename=path)


def _set_ylabel_calls(tree):
    """Literal ``set_ylabel(<constant>)`` arguments with their line numbers."""
    calls = []
    for node in ast.walk(tree):
        if not (isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
                and node.func.attr == "set_ylabel"):
            continue
        if node.args and isinstance(node.args[0], ast.Constant):
            calls.append((node.lineno, node.args[0].value))
    return calls


def _is_iloc_first(node):
    """True for ``<something>.iloc[0]``."""
    return (isinstance(node, ast.Subscript)
            and isinstance(node.value, ast.Attribute)
            and node.value.attr == "iloc")


def _compounded_returns_expressions(tree):
    """Line numbers of ``... / <series>.iloc[0] - 1`` expressions (compounded returns)."""
    lines = []
    for node in ast.walk(tree):
        if not (isinstance(node, ast.BinOp) and isinstance(node.op, ast.Sub)):
            continue
        if not (isinstance(node.right, ast.Constant) and node.right.value == 1):
            continue
        left = node.left
        if (isinstance(left, ast.BinOp) and isinstance(left.op, ast.Div)
                and _is_iloc_first(left.right)):
            lines.append(node.lineno)
    return lines


def _pct_change_cumsum_lines(tree):
    """Line numbers of ``<series>.pct_change().cumsum()`` chains (summed, not compounded)."""
    lines = []
    for node in ast.walk(tree):
        if (isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
                and node.func.attr == "cumsum"
                and isinstance(node.func.value, ast.Call)
                and isinstance(node.func.value.func, ast.Attribute)
                and node.func.value.func.attr == "pct_change"):
            lines.append(node.lineno)
    return lines


def _open_modes(tree):
    """``open()`` modes used in the module (literal second argument or ``mode=``)."""
    modes = []
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
        modes.append(mode)
    return modes


def test_metric_whitelist_excludes_strategy_parameters():
    """C18: only the documented performance metrics may be compared/plotted as metrics."""
    module = _load("optresults_handler")
    whitelist = set(module.COMPARE_METRIC_COLUMNS)

    assert whitelist == EXPECTED_METRICS, "unexpected metric whitelist: %r" % (
        sorted(whitelist ^ EXPECTED_METRICS),)
    for parameter in STRATEGY_PARAMETERS:
        assert parameter not in whitelist, "%r must not be a metric" % (parameter,)
    assert not hasattr(module, "COMPARE_STRUCTURAL_COLUMNS"), \
        "the parameter blacklist must be gone (a whitelist replaces it)"


def test_visual_opens_equity_csv_read_only():
    """C21: the equity CSV is opened with mode 'r', never '+r'."""
    modes = []
    real_open = builtins.open

    def recording_open(file, mode="r", *args, **kwargs):
        modes.append(mode)
        return real_open(file, mode, *args, **kwargs)

    def read_csv(fin, **kwargs):
        modes.append(getattr(fin, "mode", None))
        raise _StopPlotting()

    _install_fakes(read_csv)
    sys.modules.pop("visual", None)
    module = importlib.import_module("visual")

    with tempfile.TemporaryDirectory() as folder:
        path = os.path.join(folder, "equity_series.csv")
        with open(path, "w", newline="") as fout:
            fout.write("datetime;capital;drawdown;drawdown_pct\n"
                       "2020-01-01 00:00:00;100;0;0\n")
        os.chmod(path, stat.S_IRUSR | stat.S_IRGRP | stat.S_IROTH)
        builtins.open = recording_open
        try:
            try:
                module.plot_graphs(folder)
            except _StopPlotting:
                pass
        finally:
            builtins.open = real_open
            os.chmod(path, stat.S_IWUSR | stat.S_IRUSR)

    assert modes == ["r", "r"], "unexpected open/Pandas file modes: %r" % (modes,)


def test_no_write_mode_open_in_visual():
    """C21 vaccine: no ``open(..., 'r+'/'w'/'a')`` may creep back into visual.py."""
    offenders = [
        mode for mode in _open_modes(_tree(VISUAL))
        if isinstance(mode, str) and any(flag in mode for flag in "+wa")
    ]
    assert not offenders, "write-mode open() calls found: %r" % (offenders,)


def test_returns_are_compounded():
    """C20: cumulative returns are ``capital / capital.iloc[0] - 1`` in both plot scripts."""
    for path in (VISUAL, PERFORMANCE):
        tree = _tree(path)
        assert not _pct_change_cumsum_lines(tree), \
            "%s still sums per-bar returns" % (os.path.basename(path),)
        assert _compounded_returns_expressions(tree), \
            "%s has no compounded `capital / capital.iloc[0] - 1` expression" % (
                os.path.basename(path),)


def test_drawdown_axis_is_not_labeled_as_percentage():
    """C27: the absolute drawdown subplot is labeled 'Drawdown' in both plot scripts."""
    for path in (VISUAL, PERFORMANCE):
        labels = [value for _, value in _set_ylabel_calls(_tree(path))]
        assert "Drawdown_prc, %" not in labels, \
            "%s still labels absolute drawdown as a percentage" % (
                os.path.basename(path),)
        assert "Drawdown" in labels, "%s is missing the 'Drawdown' label" % (
            os.path.basename(path),)
        for expected in ("PnL", "Returns", "Drawdown_pct", "Drawdown"):
            assert expected in labels, "%s is missing the %r label" % (
                os.path.basename(path), expected)


def test_lshade_plots_are_per_strategy():
    """C19: the LSHADE convergence file name carries a sanitized strategy name."""
    module = _load("plot_portfolio_lshade_results")

    first = module.sanitize_file_name("MA_cross_1")
    second = module.sanitize_file_name("MA_cross_2")
    assert first == "MA_cross_1"
    assert second == "MA_cross_2"
    assert first != second
    assert module.sanitize_file_name("../evil/../name") == "evil_.._name"
    assert "/" not in module.sanitize_file_name("a/b")
    assert "\\" not in module.sanitize_file_name("a\\b")
    assert module.sanitize_file_name("..") == "strategy"

    with open(LSHADE, "r", encoding="utf-8") as fin:
        source = fin.read()
    assert "lshade_convergence_" in source, \
        "the output file name must include the strategy name"
    assert "plot_file_name(strategy)" in source, \
        "the save path must go through the unique-name helper"


def test_colliding_strategy_names_get_distinct_plot_files():
    """C19: ``sanitize_file_name`` is many-to-one, so the digest keeps the files apart."""
    module = _load("plot_portfolio_lshade_results")

    # Both sanitize to the same stem - the raw names are what must stay distinguishable.
    assert module.sanitize_file_name("a/b") == module.sanitize_file_name("a\\b")

    names = ["a/b", "a\\b"]
    paths = [module.plot_file_name(name) for name in names]
    assert paths[0] != paths[1], "colliding strategy names share one output file: %r" % (
        paths,)
    for name, path in zip(names, paths):
        assert path.startswith("lshade_convergence_")
        assert path.endswith(".png")
        assert "/" not in path and "\\" not in path
        assert module.name_digest(name) in path

    # The digest is stable for one name and derived from the ORIGINAL (not the sanitized) one.
    assert module.name_digest("MA_cross_1") == module.name_digest("MA_cross_1")
    assert len(module.name_digest("MA_cross_1")) == 8
    assert module.plot_file_name("MA_cross_1") != module.plot_file_name("MA_cross_1 ")


def test_lshade_writes_one_file_per_colliding_strategy():
    """C19 end-to-end: the plotting loop saves two distinct files for ``a/b`` and ``a\\b``."""
    saved = []
    frame = _FakeFrame({
        "strategy_name": ["a/b", "a\\b"],
        "iteration": [0, 0],
        "best_fitness": [1.0, 2.0],
        "mean_fitness": [0.5, 1.5],
        "population_size": [10, 10],
    })
    pyplot = _install_plot_fakes(frame, saved)
    sys.modules.pop("plot_portfolio_lshade_results", None)
    module = importlib.import_module("plot_portfolio_lshade_results")
    assert module.plt is pyplot

    with tempfile.TemporaryDirectory() as folder:
        # The script bails out early on a missing input file; the fake read_csv ignores it.
        with open(os.path.join(folder, "lshade_optimization_results.csv"), "w",
                  encoding="utf-8") as fout:
            fout.write("strategy_name;iteration;best_fitness;mean_fitness;population_size\n")
        module.plot_portfolio_lshade_results(folder)

    assert len(saved) == 2, "expected one saved plot per strategy, got %r" % (saved,)
    assert saved[0] != saved[1], "the two plots overwrite each other: %r" % (saved,)
    for path in saved:
        assert os.path.dirname(path) == folder
        assert os.path.basename(path).startswith("lshade_convergence_")


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
