#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""Plot per-strategy and portfolio equity curves as interactive matplotlib windows.

Usage: python plot_portfolio_performance.py <results_folder>

For every ``equity_curve_<strategy>.csv`` in the folder (except
``equity_curve_portfolio.csv``) one 2x2 figure is opened in the style of
``python/visual.py``; an additional 2x2 figure shows the portfolio with every
strategy's returns overlaid as thin black lines on the top-right subplot.
All figures are shown at the end with a single ``plt.show()`` call; nothing is
written to disk.
"""

import datetime
import os
import sys

import matplotlib.pyplot as plt
import pandas as pd


def _to_datetime_index(index):
    return [datetime.datetime.strptime(d, "%Y-%m-%d %H:%M:%S") for d in index]


def plot_equity(data, title, overlay=None):
    """Plot a 2x2 figure (PnL / Returns / Drawdown_pct / Drawdown).

    ``data`` is the parsed equity CSV (indexed by ``datetime``, columns
    ``capital``, ``drawdown``, ``drawdown_pct``). ``overlay`` is an optional
    list of ``(x, returns)`` pairs plotted as thin black lines on the
    top-right (Returns) subplot.
    """
    x = _to_datetime_index(data.index)

    capital = data["capital"]
    pnl = capital - capital.iloc[0]
    returns = capital.pct_change().cumsum()
    drawdown_pct = data["drawdown_pct"]
    drawdowns = data["drawdown"]

    fig, ax = plt.subplots(2, 2, figsize=(14, 8))
    fig.canvas.manager.set_window_title(title)
    fig.suptitle("Cumulative PnL vs Drawdown_pct", fontsize=10)
    plt.subplots_adjust(hspace=0, wspace=0)

    color = "tab:blue"
    ax[0, 0].plot(x, pnl, color)
    ax[0, 0].set_ylabel("PnL", color=color)
    ax[0, 0].tick_params(axis="y", labelcolor=color)
    ax[0, 0].grid(True)
    ax[0, 0].fill_between(x, pnl, 0, where=(pnl >= 0), interpolate=True, color=color)
    ax[0, 0].fill_between(x, pnl, 0, where=(pnl < 0), interpolate=True, color="red")
    ax[0, 0].set_xticklabels([])

    ax[0, 1].plot(x, returns, color)
    ax[0, 1].set_ylabel("Returns", color=color, rotation=-90, labelpad=15)
    ax[0, 1].yaxis.set_label_position("right")
    ax[0, 1].yaxis.tick_right()
    ax[0, 1].tick_params(axis="y", labelcolor=color)
    ax[0, 1].grid(True)
    ax[0, 1].fill_between(x, returns, 0, where=(returns >= 0), interpolate=True, color=color)
    ax[0, 1].fill_between(x, returns, 0, where=(returns < 0), interpolate=True, color="red")
    ax[0, 1].set_xticklabels([])

    if overlay:
        for x_overlay, returns_overlay in overlay:
            ax[0, 1].plot(x_overlay, returns_overlay, lw=0.25, color="black")

    color = "tab:red"
    ax[1, 0].plot(x, drawdown_pct, color)
    ax[1, 0].set_xlabel("dates")
    ax[1, 0].set_ylabel("Drawdown_pct", color=color)
    ax[1, 0].tick_params(axis="y", labelcolor=color)
    ax[1, 0].fill_between(x, drawdown_pct, color=color)
    ax[1, 0].grid(True)
    for label in ax[1, 0].get_xticklabels():
        label.set_fontsize(9)
        label.set_rotation(-15)

    ax[1, 1].plot(x, drawdowns, color)
    ax[1, 1].set_xlabel("dates")
    ax[1, 1].set_ylabel("Drawdown_prc, %", color=color, rotation=-90, labelpad=15)
    ax[1, 1].yaxis.set_label_position("right")
    ax[1, 1].yaxis.tick_right()
    ax[1, 1].tick_params(axis="y", labelcolor=color)
    ax[1, 1].fill_between(x, drawdowns, color=color)
    ax[1, 1].grid(True)
    for label in ax[1, 1].get_xticklabels():
        label.set_fontsize(9)
        label.set_rotation(-15)


def _read_equity(path):
    return pd.read_csv(path, sep=";", header=0, index_col="datetime")


def main():
    if len(sys.argv) != 2:
        print("Usage: python plot_portfolio_performance.py <results_folder>")
        sys.exit(1)

    folder = sys.argv[1]
    if not os.path.isdir(folder):
        print("Provide a folder with optimization results!")
        sys.exit(1)

    portfolio_path = os.path.join(folder, "equity_curve_portfolio.csv")
    has_portfolio = os.path.isfile(portfolio_path)
    if not has_portfolio:
        print("Warning: {} not found; the portfolio figure will be skipped.".format(
            portfolio_path))

    strategy_files = sorted(
        f for f in os.listdir(folder)
        if f.startswith("equity_curve_") and f.endswith(".csv")
        and f != "equity_curve_portfolio.csv"
    )

    if not strategy_files:
        print("No equity_curve_*.csv strategy files found in {}".format(folder))
        sys.exit(1)

    strategies = [
        (os.path.splitext(fname)[0], _read_equity(os.path.join(folder, fname)))
        for fname in strategy_files
    ]

    for title, data in strategies:
        plot_equity(data, title)

    if has_portfolio:
        overlay = [
            (_to_datetime_index(data.index), data["capital"].pct_change().cumsum())
            for _, data in strategies
        ]
        plot_equity(_read_equity(portfolio_path), "equity_curve_portfolio",
                    overlay=overlay)

    plt.show()


if __name__ == "__main__":
    main()
