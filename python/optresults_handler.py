#!/usr/bin/python
# -*- coding: utf-8 -*-

import math
import argparse
import numpy as np
import pandas as pd
import matplotlib.pyplot as plt

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

def three_dimensions(args) -> None:
    print("in TODO list")
    pass

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
