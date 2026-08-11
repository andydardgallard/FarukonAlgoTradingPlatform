import os
import sys
import numpy as np
import pandas as pd
import matplotlib.pyplot as plt

def plot_portfolio_lshade_results(folder) -> None:
    data_path = os.path.join(folder, "lshade_optimization_results.csv")
    if not os.path.isfile(data_path):
        print(f"File not found: {data_path}")
        return

    with open(data_path, 'r') as fin:
        plot_data = pd.read_csv(
            fin,
            header=0,
            sep=';',
        )

    strategies = set(plot_data["strategy_name"])

    for strategy in strategies:
        df = plot_data[plot_data["strategy_name"] == strategy].reset_index(drop=True)

        fig, (ax1, ax2) = plt.subplots(
            2, 1,
            figsize=(14, 10),
            num=f"{strategy}_LSHADE-RSP Results",
        )
        fig.suptitle(f"LSHADE-RSP Results — {strategy}", fontsize=14, fontweight="bold")
        fig.subplots_adjust(
            hspace=0.3,
            top=0.92,
            bottom=0.06,
            left=0.07,
            right=0.95,
        )

        x = df["iteration"]

        # Subplot 1: Fitness vs Iteration
        ax1.plot(x, df["best_fitness"], label="Best Fitness", linewidth=1.5)
        ax1.plot(x, df["mean_fitness"], label="Mean Fitness", linewidth=1.5)
        ax1.set_xlabel("Iteration")
        ax1.set_ylabel("Fitness")
        ax1.set_title("Convergence (Best & Mean Fitness)")
        ax1.legend(loc="best")
        ax1.grid(True)

        # Subplot 2: Population Size vs Iteration (LPSR)
        ax2.plot(x, df["population_size"], label="Population Size",
                 color="green", linewidth=1.5)
        ax2.set_xlabel("Iteration")
        ax2.set_ylabel("Population Size")
        ax2.set_title("LPSR — Population Size Reduction")
        ax2.legend(loc="best")
        ax2.grid(True)

        # Save or show
        out_path = os.path.join(folder, "lshade_convergence.png")
        try:
            fig.savefig(out_path, dpi=150)
            print(f"Saved: {out_path}")
        except Exception as e:
            print(f"Could not save plot: {e}")
            plt.show()

    plt.show()


if __name__ == "__main__":
    if len(sys.argv) == 2:
        if os.path.isdir(sys.argv[1]):
            plot_portfolio_lshade_results(sys.argv[1])
        else:
            print("Provide a folder with lshade optimization results!")
    else:
        print("Wrong argument! Usage: python plot_portfolio_lshade_results.py <path_to_results_folder>")
