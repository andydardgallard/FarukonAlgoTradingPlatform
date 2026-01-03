import os
import sys
import math
import numpy as np
import pandas as pd
import matplotlib.pyplot as plt

def plot_portfolio_genetic_results(folder) -> None:
    data_path = f"{folder}/ga_optimization_results.csv"
    with open(data_path, 'r+') as fin:
        plot_data = pd.read_csv(
            fin,
            header= 0,
            sep= ',',
        )
    
    plt.figure("Genetic Algorythm Results", figsize=(14, 8))
    plt.subplots_adjust(
        hspace= 0.3,
        top= 0.95,
        bottom= 0.05,
        left= 0.05,
        right= 0.95,
        wspace= 0.15)

    x = plot_data["number_of_generation"]
    y1 = plot_data["best_individ"]
    y2 = plot_data["mean"]
    plt.plot(x, y1, y2)
    plt.text(x[1], y1[1], f'{plot_data["best_hromosome_ID"].iloc[-1]}')
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
