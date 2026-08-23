# Project: Autonomous Code Factory

Этот проект содержит автономную фабрику по написанию кода для Kimi Code CLI.
Фабрика принимает бизнес-задачу от нетехнического пользователя, анализирует проект, планирует
изменения, уточняет только бизнес-логику, согласует план, реализует код и гоняет
интеграционные / регрессионные / бизнес-тесты с автоматическим откатом при неудаче, проводит
обязательное code review перед приёмкой.

## Структура

- `task.yaml` — пример бизнес-задачи (эталон формата)
- `.agents/skills/code-factory/` — flow skill фабрики (`SKILL.md`), справочники
  (`references/`: planning-guide, verification-strategy, error-routing, tech-stack-detection,
  code-review) и шаблон задачи (`assets/`)
- `.agents/agents/` — главный агент фабрики (Markdown `code-factory.md`) и сабагенты
  (`sub-agents/analyzer|coder|tester|diagnostician|code-reviewer.md`)
- `.agents/README.md` — полная инструкция по использованию фабрики

## Как использовать

- Kimi Code 0.34+: `kimi`, затем `/skill:code-factory`; либо
  `kimi --agent-file .agents/agents/code-factory.md "задача"`
- Полный автомат: `kimi --auto` → `/skill:code-factory`

## Соглашения

- Рантайм-состояние фабрики — `.code-factory/` внутри проекта (не коммитить):
  `state/` (задача, план, pipeline.yaml), `logs/` (baseline, ошибки, результаты, code-review),
  `backups/`, `manifest.json`.
- Общение с пользователем — только на бизнес-языке.
- **Артефакты до изменений**: перед правкой любого исходника в `.code-factory/` должны уже
  существовать `state/task.yaml`, `state/plan.md`, `logs/baseline.md`, `backups/`, `manifest.json`.
- **Репо-гейт**: если задача ссылается на отсутствующие в репозитории файлы/символы/конфиги —
  в режиме hitl остановиться и спросить пользователя, в режиме auto зафиксировать допущение.
- **Маршрутизация ошибок**: детерминированный regex → Diagnostician (LLM) → Human → FAILED;
  ретраи по бюджетам ролей (coder=1, ba=2, planner=2, diagnostician=1, infrastructure=3,
  reviewer=2).
- **Code review**: каждая задача проходит через сабагента `factory-code-reviewer` перед
  приёмкой. Обычная задача — ревью diff изменений; `task_type: review` — ревью всего кода в
  начале, замечания становятся планом. Задача не принимается при открытом `request_changes`.
- **Git-native**: если нет git-репозитория — `git init`; изменения идут через git
  (feature-ветка на задачу), откат к базовому коммиту при неудаче.
- **Модели**: модели задаются в `config.toml` (`default_model` + `[secondary_model]`),
  сабагентам — `model_preference: primary|secondary`. Фактические модели логируются в
  `pipeline.yaml`/`report.md` (`models_used`). Для разделения моделей обязателен
  `export KIMI_CODE_EXPERIMENTAL_SECONDARY_MODEL=1` — фабрика проверяет его в pre-flight и при
  отсутствии пишет `models_warning` в pipeline.yaml/report.md.
- **Отчёты**: `report.md` (история прогона) + `report_code_changes.md` (diff «было→стало»)
  генерируются автоматически в `.code-factory/` при завершении.
- **Коммиты**: поле `commit_exclude` в задаче исключает файлы из git-коммита
  (например, личную стратегию); ядро и документация коммитятся.
- При изменении файлов фабрики обновлять `.agents/README.md` и эти инструкции.
