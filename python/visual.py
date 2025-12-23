import os
import sys
import datetime
import numpy as np
import pandas as pd
import matplotlib.pyplot as plt

def convert_str_toDateTime(string_dateTime: str) -> datetime.datetime:
    format_ = "%Y-%m-%d %H:%M:%S"
    converted_dateTime = datetime.datetime.strptime(string_dateTime, format_)
    return converted_dateTime

def plot_graphs(path: str) -> None:
    with open(f'{path}/equity_series.csv', '+r') as fin:
        data_equity = pd.read_csv(fin, sep= ';', header= 0, index_col= "datetime")

    x = data_equity.index.to_list()
    x = [convert_str_toDateTime(date) for date in x]

    capital = data_equity["capital"]
    pnl = capital - capital.iloc[0]
    returns = data_equity["capital"].pct_change().cumsum()
    drawdown_pct = data_equity["drawdown_pct"].to_list()
    drawdowns = data_equity["drawdown"]

    fig, ax = plt.subplots(2, 2, figsize=(14, 8))
    fig.canvas.manager.set_window_title(f'visual_opt_results')
    fig.suptitle("Cumulative PnL vs Drawdown_pct", fontsize= 10)
    plt.subplots_adjust(hspace=0, wspace=0)

    color = 'tab:blue'
    ax[0, 0].plot(x, pnl, color)
    ax[0, 0].set_ylabel('PnL', color=color)
    ax[0, 0].tick_params(axis='y', labelcolor=color)
    ax[0, 0].grid(True)
    ax[0, 0].fill_between(x, pnl, 0, where= (pnl >= 0), interpolate=True, color= color)
    ax[0, 0].fill_between(x, pnl, 0, where= (pnl < 0), interpolate=True, color='red')
    ax[0, 0].set_xticklabels([])

    ax[0, 1].plot(x, returns, color)
    ax[0, 1].set_ylabel('Returns', color= color, rotation= -90, labelpad= 15)
    ax[0, 1].yaxis.set_label_position("right")
    ax[0, 1].yaxis.tick_right()
    ax[0, 1].tick_params(axis='y', labelcolor=color)
    ax[0, 1].grid(True)
    ax[0, 1].fill_between(x, returns, 0, where= (returns >= 0), interpolate=True, color= color)
    ax[0, 1].fill_between(x, returns, 0, where= (returns < 0), interpolate=True, color='red')
    ax[0, 1].set_xticklabels([])

    color = "tab:red"
    ax[1, 0].plot(x, drawdown_pct, color)
    ax[1, 0].set_xlabel('dates')
    ax[1, 0].set_ylabel('Drawdown_pct', color=color)
    ax[1, 0].tick_params(axis='y', labelcolor=color)
    ax[1, 0].fill_between(x, drawdown_pct, color= color)
    ax[1, 0].grid(True)
    for label in ax[1, 0].get_xticklabels():
        label.set_fontsize(9)
        label.set_rotation(-15)

    ax[1, 1].plot(x, drawdowns, color)
    ax[1, 1].set_xlabel('dates')
    ax[1, 1].set_ylabel('Drawdown_prc, %', color= color, rotation= -90, labelpad= 15)
    ax[1, 1].yaxis.set_label_position("right")
    ax[1, 1].yaxis.tick_right()
    ax[1, 1].tick_params(axis= 'y', labelcolor= color)
    ax[1, 1].fill_between(x, drawdowns, color= color)
    ax[1, 1].grid(True)
    for label in ax[1, 1].get_xticklabels():
        label.set_fontsize(9)
        label.set_rotation(-15)
    
    plt.show()

if __name__ == "__main__":
    if len(sys.argv) == 2:
        if os.path.isdir(sys.argv[1]):
            plot_graphs(sys.argv[1])
        else:
            print("Provide a folder with optimization results!")
    else:
        print("Wrong argument!")
    