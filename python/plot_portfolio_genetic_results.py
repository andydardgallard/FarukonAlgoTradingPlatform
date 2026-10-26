"""Plot the Genetic Algorithm results written by the optimizer (``ga_optimization_results.csv``).

The CSV parsing and the label-row selection live in small, dependency-light helpers
(standard library only), so they can be regression-tested without pandas/matplotlib.
"""

import csv
import os
import sys

import matplotlib.pyplot as plt

# Columns that carry text; every other column is numeric.
TEXT_COLUMNS = ("strategy_name", "best_hromosome_ID")


def _to_number(value):
    """Convert a CSV cell to float; empty or invalid cells become NaN."""
    try:
        return float(value)
    except (TypeError, ValueError):
        return float("nan")


def read_ga_results(data_path):
    """Read ``ga_optimization_results.csv`` into ``{strategy_name: {column: [values]}}``.

    The file is opened read-only (visualization never writes results) and parsed with
    the standard library, so the grouping logic is testable without pandas.
    """
    grouped = {}
    with open(data_path, 'r', newline='') as fin:
        reader = csv.DictReader(fin, delimiter=';')
        fieldnames = list(reader.fieldnames or [])
        for row in reader:
            strategy = row["strategy_name"]
            columns = grouped.setdefault(strategy, {name: [] for name in fieldnames})
            for name in fieldnames:
                value = row.get(name, "")
                columns[name].append(value if name in TEXT_COLUMNS else _to_number(value))
    return grouped


def label_row_index(row_count):
    """Row used as the anchor of the best-chromosome text label.

    A one-generation CSV holds a single row, so index 1 would be out of range.
    """
    return 1 if row_count > 1 else 0


def plot_portfolio_genetic_results(folder) -> None:
    data_path = f"{folder}/ga_optimization_results.csv"
    results = read_ga_results(data_path)

    for strategy in sorted(results):
        columns = results[strategy]
        plt.figure(f"{strategy}_Genetic Algorythm Results", figsize=(14, 8))
        plt.subplots_adjust(
            hspace= 0.3,
            top= 0.95,
            bottom= 0.05,
            left= 0.05,
            right= 0.95,
            wspace= 0.15)

        x = columns["number_of_generation"]
        y1 = columns["best_individ"]
        y2 = columns["mean"]

        plt.plot(x, y1, label= "best_individ")
        plt.plot(x, y2, label= "mean")
        label_row = label_row_index(len(x))
        plt.text(
            x[label_row],
            y1[label_row] * 0.98,
            f'{columns["best_hromosome_ID"][-1]}',
            fontsize= 8)
        plt.legend()
        plt.grid(True)

    plt.show()

if __name__ == "__main__":
    if len(sys.argv) == 2:
        if os.path.isdir(sys.argv[1]):
            plot_portfolio_genetic_results(sys.argv[1])
        else:
            print("Provide a folder with ga optimization results!")
    else:
        print("Wrong argument!")
