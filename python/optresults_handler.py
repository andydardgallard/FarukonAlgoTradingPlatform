#!/usr/bin/python
# -*- coding: utf-8 -*-

import math
import argparse
import numpy as np
import pandas as pd
import matplotlib.pyplot as plt

#     import matplotlib.pyplot as plt
# from mpl_toolkits.mplot3d import Axes3D
# import numpy as np

def args_parser():
    parser = argparse.ArgumentParser(description="Flags of Command-Line options")
    parser.add_argument(
        "-f", "--file",                                      # указывающий путь к папке с данными
        default= '',                                         # Значение по умолчанию
        required= True,                                      # Необязательный параметр
        type= str,                                           # Тип строковый
        help= "Path to folder with data"
    )
    parser.add_argument(
        "-x", "--xaxis",                                 
        default= '',                                         # Значение по умолчанию
        required= True,                                      # Необязательный параметр
        type= str,                                           # Тип строковый
        help= "Parameter to obtain"
    )
    parser.add_argument(
        "-y", "--yaxis",                                 
        default= '',                                         # Значение по умолчанию
        required= True,                                      # Необязательный параметр
        type= str,                                           # Тип строковый
        help= "Parameter to obtain"
    )
    parser.add_argument(
        "-d", "--dimension",                                 
        default= '',                                         # Значение по умолчанию
        required= True,                                      # Необязательный параметр
        type= str,                                           # Тип строковый
        choices= ["2D", "3D"],
        help= "Dimension of axsis"
    )
    parser.add_argument(
        "-m", "--mode",                                 
        default= '',                                         # Значение по умолчанию
        required= True,                                      # Необязательный параметр
        type= str,                                           # Тип строковый
        choices= ["visual", "select"],
        help= "The mode of handler. Visual = plot graphs. Select = selection of results by mask."
    )
    return parser.parse_args()

def two_dimensions(args) -> None:
    print("in TODO list")
    pass

def prepare_data(args) -> dict:
    data_path = f"{args.file}/optimization_results.csv"
    with open(data_path, 'r+') as fin:
        plot_data = pd.read_csv(
            fin,
            header= 0,
            sep= ';',
        )
    
    strategies = set(plot_data["strategy_name"])
    result = {}
    for stratagy in strategies:
        data_for_strategy = plot_data[plot_data["strategy_name"] == stratagy].reset_index(drop= True)
        xaxis = set(data_for_strategy[args.xaxis])
        coordinates_x = []
        coordinates_y = []
        for x in xaxis:
            data_for_yaxis = data_for_strategy[data_for_strategy[args.xaxis] == x]
            y = data_for_yaxis[args.yaxis].max()
            coordinates_x.append(x)
            coordinates_y.append(y)

        result[stratagy] = (coordinates_x, coordinates_y)

    return result

def three_dimensions(args) -> None:
    data_dict = prepare_data(args)
    strategies = sorted(data_dict.keys())

    ## Plot slice of 3D graph with max values
    slice_graph = {}
    for strategy in strategies:
        max_value_for_strategy = max(data_dict[strategy][1])
        position_of_max_value = data_dict[strategy][1].index(max_value_for_strategy)
        slice_graph[strategy] = (data_dict[strategy][0][position_of_max_value], max_value_for_strategy)
    
    plt.figure("Otimization Results", figsize=(14, 8))
    plt.subplots_adjust(
        hspace= 0.3,
        top= 0.95,
        bottom= 0.05,
        left= 0.05,
        right= 0.95,
        wspace= 0.15)
    
    x = strategies
    y_xaxis = [slice_graph[strategy][0] for strategy in strategies]
    y_yaxis = [slice_graph[strategy][1] for strategy in strategies]

    min_y_xaxis = min(y_xaxis)

    ax1_slice_graph = plt.subplot()
    ax1_slice_graph.plot(x, y_xaxis, 'b', label= args.xaxis)
    ax1_slice_graph.set_ylabel(args.xaxis)
    ax1_slice_graph.fill_between(x, min_y_xaxis, y_xaxis, color= 'b', alpha= 0.5)

    ax2_slice_graph = ax1_slice_graph.twinx()
    ax2_slice_graph.plot(x, y_yaxis, 'r', label= args.yaxis)
    ax2_slice_graph.set_ylabel(args.yaxis)

    ## pot hist of frequency of xaxes values
    bins = int(np.sqrt(len(y_xaxis)))

    plt.figure(f"Frequency of {args.xaxis} hist", figsize=(14, 8))
    plt.subplots_adjust(
        hspace= 0.3,
        top= 0.95,
        bottom= 0.05,
        left= 0.05,
        right= 0.95,
        wspace= 0.15)

    plt.hist(y_xaxis, bins= bins, edgecolor='black') # 'bins' defines the number of groups
    plt.xlabel(f'{args.xaxis}')
    plt.ylabel('Frequency (Count)')


    ## plot 3D graph
    all_x = sorted(set(float(x) for xs, _ in data_dict.values() for x in xs))
    
    x_to_mean = {}
    for x_val in all_x:
        y_vals_for_x = []
        for xs, ys in data_dict.values():
            for x, y in zip(xs, ys):
                if float(x) == float(x_val):
                    y_vals_for_x.append(y)
        x_to_mean[x_val] = np.mean(y_vals_for_x) if y_vals_for_x else 0.0

    Z = []
    for strategy in strategies:
        xs, ys = data_dict[strategy]
        xy_map = {float(x): y for x, y in zip(xs, ys)}
        
        row = []
        for x_val in all_x:
            if float(x_val) in xy_map:
                row.append(xy_map[float(x_val)])
            else:
                row.append(x_to_mean[float(x_val)])
        Z.append(row)

    Z = np.array(Z)
    
    X_grid, Y_grid = np.meshgrid(all_x, np.arange(len(strategies)))
    
    fig = plt.figure(figsize=(14, 8))
    ax = fig.add_subplot(111, projection='3d')
    
    surf = ax.plot_surface(X_grid, Y_grid, Z, 
                          cmap='viridis',                   # plasma, coolwarm, viridus
                          edgecolor='none',
                          alpha=0.9)
    
    # for i, strategy in enumerate(strategies):
    #     xs, ys = data_dict[strategy]
    #     xs_float = [float(x) for x in xs]
    #     ax.scatter(xs_float, [i]*len(xs), ys, c='red', s=30, label=f'{strategy} (orig)')
    
    ax.set_xlabel(args.xaxis, fontsize=9)
    ax.set_ylabel('Strategy', fontsize=9)
    ax.set_zlabel(args.yaxis, fontsize=9)
    ax.set_xticks(all_x)
    ax.set_yticks(np.arange(len(strategies)))
    
    ax.set_yticklabels(strategies, fontsize=7, rotation=15)
    
    # ax.legend(loc='upper left', fontsize=6, bbox_to_anchor=(1.05, 1))
    
    cbar = fig.colorbar(surf, ax=ax, shrink=0.45, aspect=10, pad=0.15, label=args.yaxis)
    cbar.ax.tick_params(labelsize=7)
    
    ax.set_title(f'3D Plot: {args.xaxis} (missing values filled with mean)', 
                fontsize=10, pad=15)
    
    ax.view_init(elev=25, azim=45)
    
    plt.subplots_adjust(
        left=0.01,    
        right=0.99,    
        bottom=0.02,  
        top=0.95,      
        wspace=0.1,
        hspace=0.1
    )
    
    plt.show()


def selection() -> None:
    print("in TODO list")
    pass

def plot_set_get_xaxis(args) -> pd.DataFrame:
    with open(args.file) as fin:
        data = pd.read_csv(
            fin,
            header=0,
            sep=';',
            )
    
    return data

def plot_set(args) -> None:
    results = plot_set_get_xaxis(args)
    results = results.drop_duplicates().reset_index(drop= True)
    
    results_list = results.columns.to_list()[-8:]
    sorted_results = results.sort_values(
        by=args.xaxis,
        ascending=True
    )
    x = sorted_results[args.xaxis]
    subplots_cell = math.ceil(np.sqrt(len(results_list)))
    
    plt.figure("Otimization Results", figsize=(14, 8))
    plt.subplots_adjust(
        hspace= 0.3,
        top= 0.95,
        bottom= 0.05,
        left= 0.05,
        right= 0.95,
        wspace= 0.15)

    for n in range(0, len(results_list)):
        ax = plt.subplot(subplots_cell, subplots_cell, n + 1)
        y = sorted_results[results_list[n]]
        ax.plot(x, y)
        ax.set_title(results_list[n], fontsize=10)
        ax.set_xlabel("")
        ax.grid(True)

    plt.show()

if __name__ == "__main__":
    args = args_parser()
    if args.mode == "visual":
        if args.dimension == "2D":
            if args.yaxis == "set":
                plot_set(args)
            else:
                two_dimensions(args)
        elif args.dimension == "3D":
            three_dimensions(args)
    elif args.mode == "select":
        selection()
