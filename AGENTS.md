# Project: Autonomous Code Factory

Этот проект содержит автономную фабрику по написанию кода для Kimi Code CLI + Kimi Agents SDK.
Фабрика принимает бизнес-задачу от нетехнического пользователя, анализирует проект, планирует
изменения, уточняет только бизнес-логику, согласует план, реализует код и гоняет
интеграционные / регрессионные / бизнес-тесты с автоматическим откатом при неудаче.

## Структура

- `task.yaml` — пример бизнес-задачи (эталон формата)
- `.agents/skills/code-factory/` — flow skill фабрики (`SKILL.md`), справочники
  (`references/`: planning-guide, verification-strategy, error-routing, tech-stack-detection)
  и шаблон задачи (`assets/`)
- `.agents/agents/` — главный агент фабрики: legacy YAML (`code-factory.yaml`, `system.md`) и
  Markdown для новой версии (`code-factory.md`), модели (`models.yaml`) и сабагенты
  (`sub-agents/analyzer|coder|tester|diagnostician.{yaml,md}`)
- `.agents/examples/run_factory_sdk.py` — пример запуска через Kimi Agent SDK
- `.agents/README.md` — полная инструкция по использованию фабрики

## Как использовать

- Новая версия (Kimi Code 0.34+): `kimi`, затем `/skill:code-factory`; либо
  `kimi --agent-file .agents/agents/code-factory.md "задача"`
- Старая версия (Kimi CLI 1.12): `kimi`, затем `/flow:code-factory`; либо
  `kimi --agent-file .agents/agents/code-factory.yaml "задача"`
- SDK: `python3 .agents/examples/run_factory_sdk.py task.yaml`

## Соглашения

- Рантайм-состояние фабрики — `.code-factory/` внутри проекта (не коммитить):
  `state/` (задача, план, pipeline.yaml), `logs/` (baseline, ошибки, результаты), `backups/`,
  `manifest.json`.
- Общение с пользователем — только на бизнес-языке.
- **Артефакты до изменений**: перед правкой любого исходника в `.code-factory/` должны уже
  существовать `state/task.yaml`, `state/plan.md`, `logs/baseline.md`, `backups/`, `manifest.json`.
- **Репо-гейт**: если задача ссылается на отсутствующие в репозитории файлы/символы/конфиги —
  в режиме hitl остановиться и спросить пользователя, в режиме auto зафиксировать допущение.
- **Маршрутизация ошибок**: детерминированный regex → Diagnostician (LLM) → Human → FAILED;
  ретраи по бюджетам ролей (coder=1, ba=2, planner=2, diagnostician=1).
- **Git-native**: если нет git-репозитория — `git init`; изменения идут через git
  (feature-ветка на задачу), откат к базовому коммиту при неудаче.
- **Модели**: `.agents/agents/models.yaml` — per-role конфигурация (deepseek/qwen/kimi и др.).
  В новой версии модели задаются в `config.toml` (`default_model` + `[secondary_model]`),
  сабагентам — `model_preference: primary|secondary`. Фактические модели логируются в
  `pipeline.yaml`/`report.md` (`models_used`).
- **Отчёты**: `report.md` (история прогона) + `report_code_changes.md` (diff «было→стало»)
  генерируются автоматически в `.code-factory/` при завершении.
- **Коммиты**: поле `commit_exclude` в задаче исключает файлы из git-коммита
  (например, личную стратегию); ядро и документация коммитятся.
- При изменении файлов фабрики обновлять `.agents/README.md` и эти инструкции.
