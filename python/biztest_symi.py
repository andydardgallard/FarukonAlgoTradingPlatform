#!/usr/bin/python
# -*- coding: utf-8 -*-
"""
Automated business test for the SYMI_Ch_SMA_up_lmt strategy fix
(instruments with fractional prices, e.g. CNY-3.23).

Checks:
  1. CNY grid search (width_channel 0.05..1.505 step 0.05, 30 combos) produces
     MORE than 2 unique result variants (pre-fix it was exactly 2: no deals,
     or identical deals). Different thresholds must yield different outcomes.
  2. Si grid search (width_channel [100,150,200,5000,10000]) produces results
     IDENTICAL to the pre-fix baseline (.code-factory/logs/pre-fix/Si_before_fix.csv).

Usage:
    python3 python/biztest_symi.py
Exit code 0 on success, 1 on failure.
"""

import csv
import glob
import os
import shutil
import subprocess
import sys

PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
STRATEGY_SRC = os.path.join(PROJECT_ROOT, "strategy_lib", "src", "SYMI_Ch_SMA_up_lmt.rs")
STRATEGY_OUT = os.path.join(PROJECT_ROOT, "target", "release", "SYMI_Ch_SMA_Up.so")
BIN = os.path.join(PROJECT_ROOT, "target", "release", "Farukon_2")

CNY_CONFIG = os.path.join(PROJECT_ROOT, ".code-factory", "biztest", "cny_config.json")
SI_CONFIG = os.path.join(PROJECT_ROOT, ".code-factory", "biztest", "si_config_full.json")
SI_BASELINE = os.path.join(PROJECT_ROOT, ".code-factory", "logs", "pre-fix", "Si_before_fix.csv")

CNY_RESULTS = os.path.join(PROJECT_ROOT, "opt_results", "symi_cny_test", "optimization_results.csv")
SI_RESULTS = os.path.join(PROJECT_ROOT, "opt_results", "symi_si_test", "optimization_results.csv")

METRIC_COLS = ["APR", "Total_Return", "Deals_Count", "Max_Drawdown"]


def read_results(path):
    if not os.path.exists(path):
        return []
    with open(path, newline="") as f:
        return list(csv.DictReader(f, delimiter=";"))


def unique_variants(rows):
    seen = set()
    for r in rows:
        key = tuple(r.get(c, "") for c in METRIC_COLS)
        seen.add(key)
    return seen


def rebuild_strategy():
    """Rebuild SYMI_Ch_SMA_Up.so from source (matches documented manual build)."""
    deps = os.path.join(PROJECT_ROOT, "target", "release", "deps")
    rlib = os.path.join(PROJECT_ROOT, "target", "release", "libfarukon_core.rlib")
    anyhow = glob.glob(os.path.join(deps, "libanyhow-*.rlib"))
    chrono = glob.glob(os.path.join(deps, "libchrono-*.rlib"))
    if not os.path.exists(rlib) or not anyhow or not chrono:
        print("FAIL: missing rlib deps — run `cargo build --release` first")
        return False
    cmd = [
        "rustc", "--edition", "2024", "--crate-type", "cdylib",
        "--extern", f"farukon_core={rlib}",
        "--extern", f"anyhow={anyhow[0]}",
        "--extern", f"chrono={chrono[0]}",
        "-L", f"dependency={deps}",
        "-o", STRATEGY_OUT,
        STRATEGY_SRC,
    ]
    res = subprocess.run(cmd, cwd=PROJECT_ROOT, capture_output=True, text=True)
    if res.returncode != 0:
        print("FAIL: rustc build of SYMI_Ch_SMA_Up.so failed")
        print(res.stderr[-2000:])
        return False
    return True


def run_backtest(config, results_path):
    if os.path.exists(results_path):
        os.remove(results_path)
    res = subprocess.run([BIN, "-c", config], cwd=PROJECT_ROOT, capture_output=True, text=True)
    if res.returncode != 0:
        print("FAIL: backtest run failed for", config)
        print(res.stderr[-2000:])
        return False
    return True


def main():
    if not os.path.exists(BIN):
        print("FAIL: Farukon_2 binary missing — run `cargo build --release` first")
        return 1
    if not rebuild_strategy():
        return 1

    ok = True

    # --- 1. CNY: more than 2 unique variants ---
    if not run_backtest(CNY_CONFIG, CNY_RESULTS):
        return 1
    cny_rows = read_results(CNY_RESULTS)
    cny_variants = unique_variants(cny_rows)
    cny_deals = {r["Deals_Count"] for r in cny_rows}
    print(f"CNY: {len(cny_rows)} combos, {len(cny_variants)} unique result variants, "
          f"deals counts seen: {sorted(cny_deals, key=lambda x: int(x))}")
    if len(cny_variants) <= 2:
        print("FAIL: CNY produced <= 2 unique variants (fix did not take effect)")
        ok = False
    elif len(cny_variants) == len(cny_rows):
        print("OK: CNY produces a distinct outcome per width_channel value")
    else:
        print("OK: CNY produced > 2 unique variants (partially distinct outcomes)")

    # --- 2. Si: identical to pre-fix baseline ---
    if not run_backtest(SI_CONFIG, SI_RESULTS):
        return 1
    si_rows = read_results(SI_RESULTS)
    si_baseline = read_results(SI_BASELINE)
    norm = lambda rows: sorted(
        [tuple(r.get(c, "") for c in METRIC_COLS) for r in rows]
    )
    if norm(si_rows) == norm(si_baseline):
        print(f"OK: Si results identical to pre-fix baseline ({len(si_baseline)} rows)")
    else:
        print("FAIL: Si results differ from pre-fix baseline")
        print("  before:", norm(si_baseline))
        print("  after :", norm(si_rows))
        ok = False

    print("BUSINESS TEST:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
