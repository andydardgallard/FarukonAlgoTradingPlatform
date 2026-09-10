<!-- code-factory-memory: summary -->

# Farukon — сводка долгосрочной памяти

## Текущее состояние
Проект Farukon (Rust workspace: `farukon_core` lib, `Farukon_2` bin, `strategy_lib` cdylib; Python-инструменты в `python/`). Версия `2.1.0`.

## Ключевые решения (последний прогон, 2026-09-10)
- LSHADE: лучшая особь определяется по `fitness_direction` («max»/«min»); `evaluate` возвращает сырой фитнес, направление применяется внутри `run`.
- Нумерация итераций LSHADE 1-based (начальная популяция = «Итерация 1»).
- Статистика `lshade_optimization_results.csv` пишется append'ом после каждой итерации (заголовок один раз в начале запуска).
- Кэш особей LSHADE — in-memory `HashMap` (ключ включает `pos_sizer_name`), очищается + `shrink_to_fit` через `BankClearGuard`.
- `Max_Drawdown_DateTime` (относительная просадка) добавлено в `optimization_results.csv` и портфельный CSV.
- `python/optresults_handler.py` получил режим сравнения двух CSV (`-fc/--file_compare`, `-y set_cmp`).

## Последняя история
- 2026-09-10: LSHADE доработка + кэш + обработчики — success, 34 теста, review approve, v2.1.0.
