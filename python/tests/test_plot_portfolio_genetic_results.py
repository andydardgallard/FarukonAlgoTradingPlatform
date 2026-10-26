#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""Regression tests for ``python/plot_portfolio_genetic_results.py``.

Runs with the standard library only: matplotlib is replaced by a tiny recorder before
the module is imported, and the GA CSV is written/read with ``csv``.

Run directly:  python python/tests/test_plot_portfolio_genetic_results.py
Under pytest:  pytest python/tests/test_plot_portfolio_genetic_results.py
"""

import builtins
import csv
import importlib
import os
import sys
import tempfile
import types

HERE = os.path.dirname(os.path.abspath(__file__))
PYTHON_DIR = os.path.dirname(HERE)
if PYTHON_DIR not in sys.path:
    sys.path.insert(0, PYTHON_DIR)

HEADER = ["strategy_name", "number_of_generation", "best_individ", "mean",
          "best_hromosome_ID"]


class FakePyplot(object):
    """Minimal ``matplotlib.pyplot`` stand-in that records every call."""

    def __init__(self):
        self.plots = []
        self.texts = []
        self.figures = []

    def figure(self, *args, **kwargs):
        self.figures.append((args, kwargs))

    def subplots_adjust(self, **kwargs):
        pass

    def plot(self, x, y, **kwargs):
        self.plots.append((list(x), list(y), kwargs))

    def text(self, x, y, label, **kwargs):
        self.texts.append((x, y, label, kwargs))

    def legend(self):
        pass

    def grid(self, *args, **kwargs):
        pass

    def show(self):
        pass


def _load_plotter():
    """Import the plotter against a fake matplotlib and return (module, fake pyplot)."""
    pyplot = FakePyplot()
    matplotlib = types.ModuleType("matplotlib")
    matplotlib.pyplot = pyplot
    sys.modules["matplotlib"] = matplotlib
    sys.modules["matplotlib.pyplot"] = pyplot
    sys.modules.pop("plot_portfolio_genetic_results", None)
    module = importlib.import_module("plot_portfolio_genetic_results")
    module.plt = pyplot
    return module, pyplot


def _write_ga_csv(directory, rows):
    path = os.path.join(directory, "ga_optimization_results.csv")
    with open(path, "w", newline="") as fout:
        writer = csv.writer(fout, delimiter=";")
        writer.writerow(HEADER)
        writer.writerows(rows)
    return path


def test_label_row_index_guards_single_row():
    """The text-label anchor must stay in range for one-generation files."""
    module, _ = _load_plotter()
    assert module.label_row_index(1) == 0
    assert module.label_row_index(2) == 1
    assert module.label_row_index(5) == 1


def test_one_generation_csv_plots_without_raising():
    """A single-row GA CSV must plot: two series, one label, no IndexError."""
    module, pyplot = _load_plotter()
    with tempfile.TemporaryDirectory() as folder:
        _write_ga_csv(folder, [["MA_cross_1", 0, 12.5, 11.0, "id_0"]])
        module.plot_portfolio_genetic_results(folder)

    assert len(pyplot.plots) == 2, "expected one plot call per series"
    assert [len(x) for x, _, _ in pyplot.plots] == [1, 1]
    assert len(pyplot.texts) == 1
    x_anchor, y_anchor, label, _ = pyplot.texts[0]
    assert x_anchor == 0.0
    assert y_anchor == 12.5 * 0.98
    assert label == "id_0"


def test_two_plot_calls_per_series_for_multiple_generations():
    """Each series gets its own plot call; the label uses the second generation."""
    module, pyplot = _load_plotter()
    rows = [["MA_cross_1", gen, 10.0 + gen, 9.0 + gen, "id_%d" % gen]
            for gen in range(3)]
    with tempfile.TemporaryDirectory() as folder:
        _write_ga_csv(folder, rows)
        module.plot_portfolio_genetic_results(folder)

    assert [list(x) for x, _, _ in pyplot.plots] == [[0.0, 1.0, 2.0], [0.0, 1.0, 2.0]]
    assert [list(y) for _, y, _ in pyplot.plots] == [[10.0, 11.0, 12.0],
                                                     [9.0, 10.0, 11.0]]
    assert [kwargs.get("label") for _, _, kwargs in pyplot.plots] == ["best_individ", "mean"]
    assert pyplot.texts[0][0] == 1.0
    assert pyplot.texts[0][2] == "id_2"


def test_rows_are_grouped_per_strategy():
    """read_ga_results groups rows by strategy and keeps numeric columns numeric."""
    module, pyplot = _load_plotter()
    rows = [["S1", 0, 1.5, 1.0, "a"], ["S2", 0, 2.5, 2.0, "b"],
            ["S1", 1, 3.5, 3.0, "c"]]
    with tempfile.TemporaryDirectory() as folder:
        path = _write_ga_csv(folder, rows)
        results = module.read_ga_results(path)
        module.plot_portfolio_genetic_results(folder)

    assert sorted(results) == ["S1", "S2"]
    assert results["S1"]["number_of_generation"] == [0.0, 1.0]
    assert results["S1"]["best_individ"] == [1.5, 3.5]
    assert results["S1"]["best_hromosome_ID"] == ["a", "c"]
    assert len(pyplot.figures) == 2
    assert len(pyplot.plots) == 4


def test_results_csv_is_opened_read_only():
    """Regression: the results CSV must be opened with mode 'r', never 'r+'."""
    module, _ = _load_plotter()
    modes = []
    real_open = builtins.open

    def recording_open(file, mode="r", *args, **kwargs):
        modes.append(mode)
        return real_open(file, mode, *args, **kwargs)

    with tempfile.TemporaryDirectory() as folder:
        path = _write_ga_csv(folder, [["S1", 0, 1.0, 1.0, "a"]])
        builtins.open = recording_open
        try:
            module.read_ga_results(path)
        finally:
            builtins.open = real_open

    assert modes == ["r"], "unexpected open modes: %r" % (modes,)


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
