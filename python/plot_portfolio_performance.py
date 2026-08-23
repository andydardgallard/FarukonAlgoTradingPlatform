#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""Plot the portfolio equity curve from equity_curve_portfolio.csv.

The input CSV uses semicolon-separated columns and the header
``datetime;capital;drawdown;drawdown_pct`` (same format as the per-strategy
``equity_curve_<strategy_name>.csv`` files produced by the Farukon engine).
"""

import argparse
import os
import sys

import matplotlib

matplotlib.use("Agg")  # headless: save to file, no display required

import matplotlib.pyplot as plt
import pandas as pd


def plot_portfolio_curve(csv_path, output_path=None):
    df = pd.read_csv(csv_path, sep=";", header=0)
    df["datetime"] = pd.to_datetime(df["datetime"], format="%Y-%m-%d %H:%M:%S")

    fig, (ax_capital, ax_drawdown) = plt.subplots(
        2, 1, figsize=(14, 8), sharex=True
    )

    ax_capital.plot(df["datetime"], df["capital"], color="tab:blue", label="Capital")
    ax_capital.set_ylabel("Capital")
    ax_capital.set_title("Portfolio equity curve")
    ax_capital.grid(True)
    ax_capital.legend(loc="upper left")

    ax_drawdown.plot(
        df["datetime"], df["drawdown_pct"], color="tab:red", label="Drawdown (abs)"
    )
    ax_drawdown.set_ylabel("Drawdown")
    ax_drawdown.set_xlabel("datetime")
    ax_drawdown.grid(True)
    ax_drawdown.legend(loc="lower left")

    fig.autofmt_xdate()
    fig.tight_layout()

    if output_path is None:
        output_path = os.path.splitext(csv_path)[0] + ".png"

    fig.savefig(output_path, dpi=120)
    plt.close(fig)
    print("Saved portfolio plot to {}".format(output_path))


def main():
    parser = argparse.ArgumentParser(
        description="Plot the portfolio equity curve from equity_curve_portfolio.csv."
    )
    parser.add_argument(
        "csv",
        nargs="?",
        default="equity_curve_portfolio.csv",
        help="Path to equity_curve_portfolio.csv (default: %(default)s)",
    )
    parser.add_argument(
        "-o",
        "--output",
        help="Output PNG path (default: <csv>.png)",
    )
    args = parser.parse_args()

    if not os.path.exists(args.csv):
        sys.exit("CSV not found: {}".format(args.csv))

    plot_portfolio_curve(args.csv, args.output)


if __name__ == "__main__":
    main()
