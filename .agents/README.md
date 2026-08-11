# Code Factory (for Kimi Code CLI + Kimi Agents SDK)

Автономная фабрика по написанию кода. Принимает бизнес-задачу от пользователя, который не
разбирается в программировании, анализирует проект, планирует изменения, уточняет только
бизнес-логику, получает согласование плана, а затем сама пишет код, гоняет
интеграционные / регрессионные / бизнес-тесты с автоматическим откатом при любой неудаче и
проверяет критерии приёмки.

Поддерживает **любые языки** (стек определяется автоматически) и работает с существующими
проектами или создаёт проекты с нуля.

## Структура

```
.agents/
├── README.md                        # этот файл
├── skills/
│   └── code-factory/
│       ├── SKILL.md                 # Flow skill — главный оркестратор (тип: flow)
│       ├── references/
│       │   ├── planning-guide.md    # анализ проекта, DAG-план, вопросы по бизнес-тестам
│       │   ├── verification-strategy.md  # интеграционные/регресс/бизнес-тесты + откат
│       │   ├── error-routing.md     # детерминированная маршрутизация ошибок + Diagnostician
│       │   └── tech-stack-detection.md  # определение стека + Scout pipeline
│       └── assets/
│           └── task-template.yaml   # шаблон бизнес-задачи
├── agents/
│   ├── code-factory.yaml            # главный агент (legacy 1.12, --agent-file YAML)
│   ├── code-factory.md              # главный агент (Kimi Code 0.34+, --agent-file Markdown)
│   ├── system.md                    # системный промпт главного агента (legacy)
│   ├── models.yaml                  # модели для ролей (deepseek/qwen/kimi и др.)
│   └── sub-agents/
│       ├── analyzer.{yaml,md}       # сабагент: анализ проекта (read-only)
│       ├── coder.{yaml,md}          # сабагент: реализация кода
│       ├── tester.{yaml,md}         # сабагент: тесты и проверка результатов
│       └── diagnostician.{yaml,md}  # сабагент: глубокий анализ ошибок (read-only)
└── examples/
    └── run_factory_sdk.py           # пример запуска через Kimi Agent SDK (legacy)
```

Рантайм-состояние фабрики живёт в `.code-factory/` внутри проекта:
`state/` (задача, план), `backups/` (бэкапы изменяемых файлов), `manifest.json` (список
изменённых/созданных файлов), `logs/` (ошибки, результаты тестов).

## Две версии Kimi CLI

Фабрика поддерживает обе версии Kimi:

| | Kimi CLI 1.12 (Python, `~/.kimi`) | Kimi Code 0.34+ (Node, `~/.kimi-code`) |
|---|---|---|
| Flow-скилл | `/flow:code-factory` | `/skill:code-factory` (flow вызывается так же) |
| Агент | `--agent-file .agents/agents/code-factory.yaml` | `--agent-file .agents/agents/code-factory.md` |
| Сабагенты | `sub-agents/*.yaml` | `sub-agents/*.md` |
| Модели | `models.yaml` → `model=` в Agent tool | `config.toml` (`default_model` + `[secondary_model]`) |
| Автономность | `--yolo` | `--yolo` / `--auto` |

### Запуск в новой версии (Kimi Code 0.34+)

```bash
kimi
# в чате:
/skill:code-factory        # запуск flow-скилла (без текста после!)
# или:
kimi --agent-file .agents/agents/code-factory.md "прочитай task.yaml и выполни"
```

Полный автономный режим: `kimi --auto` → `/skill:code-factory`. Модели: `default_model` и
`[secondary_model]` в `~/.kimi-code/config.toml`; сабагентам — `model_preference:
primary|secondary` в `.md`-файлах (см. `agents/models.yaml`).

> Примечание: warning про `system.md` (Missing frontmatter) при старте — безвреден
> (старый файл для legacy-версии; новая версия использует `code-factory.md`).

## Формат бизнес-задачи

Минимальный формат — свободный текст. Рекомендуемый — `task.yaml` (см. шаблон в
`.agents/skills/code-factory/assets/task-template.yaml`):

```yaml
title: "Стратегия не генерирует сигналы для CNY"
repo_path: ./repo            # только для существующих проектов
description: |
  Опишите проблему бизнес-языком, без технических деталей.
priority: high               # high | medium | low
mode: hitl                   # hitl (по умолчанию) | auto
acceptance_criteria:
  - "Стратегия генерирует не менее 5 сигналов LONG/SHORT для CNY"
```

## Запуск

### 1) CLI, интерактивный режим (рекомендуется)

```sh
cd <проект>
kimi
```

Затем в чате:
- **Новая версия (Kimi Code 0.34+):** `/skill:code-factory`
- **Старая версия (Kimi CLI 1.12):** `/flow:code-factory`

Фабрика спросит бизнес-вопросы и покажет план на согласование. Полный автомат:
`kimi --auto` (новая) или `kimi --yolo` (обе) → команда выше.

Или сразу с готовой задачей:

```sh
# новая версия:
kimi --agent-file .agents/agents/code-factory.md "Прочитай task.yaml и реши задачу"
# старая версия:
kimi --agent-file .agents/agents/code-factory.yaml "Прочитай task.yaml и реши задачу"
```

### 2) Kimi Agents SDK (автоматизация)

```sh
python3 .agents/examples/run_factory_sdk.py task.yaml
```

Скрипт использует `kimi_agent_sdk.prompt()` с `agent_file=code-factory.yaml`,
`skills_dir=.agents/skills` и `yolo=True` (полный автомат). Для HITL поменяйте `yolo=False` и
передайте `approval_handler_fn` (пример в скрипте).

## Режимы

- **hitl (по умолчанию)** — фабрика уточняет у пользователя бизнес-сценарий, конфиги для
  запуска и ожидаемые бизнес-результаты, затем показывает план на согласование.
- **auto** — фабрика принимает разумные допущения (записывает их в план как assumptions) и
  работает без вопросов.

## Откат изменений и маршрутизация ошибок

При неудаче любого теста фабрика НЕ откатывается вслепую:
1. **Классифицирует ошибку** детерминированно (regex, ~90% случаев, 0 токенов) —
   compile→coder, missing file→BA, bad command→Planner, инфраструктура→автофикс,
   wrong results/unknown→Diagnostician (см. `references/error-routing.md`).
2. **Роллбэк**: восстанавливает файлы из `.code-factory/backups/`, удаляет созданные файлы
   (по `manifest.json`), возвращает проект в до-изменённое состояние.
3. **Retry-бюджеты**: coder=1, BA=2, Planner=2, Diagnostician=1. При исчерпании — эскалация
   на Diagnostician (LLM-анализ, пишет `.code-factory/logs/diagnostic.md`).
4. **Human**: если Diagnostician рекомендует — показать пользователю и спросить.
5. **FAILED**: только при исчерпании всех бюджетов, с полным логом в
   `.code-factory/logs/errors.md`. Фабрика никогда не падает молча.

Checkpoint/resume: после каждой фазы пишется `.code-factory/state/pipeline.yaml` — при
перезапуске фабрика продолжает с того же места.

Итоговый отчёт: при завершении (успех или FAILED) фабрика пишет `.code-factory/report.md` —
один самодостаточный файл со всей историей прогона (задача, план, изменения, тесты,
ошибки, диагностика, acceptance). Рядом генерируется `.code-factory/report_code_changes.md` —
наглядный отчёт «было → стало» по изменённым строкам коммита (скриптом
`scripts/gen_code_changes_report.py`, без затрат LLM). Оба файла достаточно переслать
разработчику фабрики для анализа — читать весь `.code-factory/` не нужно.

## Бизнес-тесты

Бизнес-тест = запуск реальной (исправленной/созданной) программы с конфигами пользователя и
проверка, что **бизнес-результаты** совпадают с ожидаемыми. Сценарий, конфиги и ожидаемые
результаты фабрика уточняет у пользователя на этапе планирования (в режиме hitl).

Практические приёмы (обкатаны на реальном прогоне):
- читать `exit_results_path` из конфига и проверять сгенерированные отчёты (строки, колонки);
- если пользователь говорит «неправильный результат — только N вариантов», проверять, что после
  фикса уникальных вариантов результатов стало БОЛЬШЕ N (проверка отличий результатов);
- отличать «фильтр работает» от «сломано»: 0 сделок при неподходящем пороге — норма; доказать
  работу фильтра повторным прогоном с порогом, соразмерным цене инструмента;
- бизнес-тесты выполнять именно теми командами, что дал пользователь (build + run);
- побочные артефакты прогона (папки результатов) удалять после проверки или игнорировать в git.

## Модели

Роли фабрики используют разные модели — конфигурация в `.agents/agents/models.yaml`
(analyzer/coder/tester — быстрые, planner/diagnostician — рассуждающие).
Можно менять провайдеров и семейства (deepseek, qwen, kimi/moonshot и др.):
- CLI: `kimi -m <model>` или `/model` в сессии;
- SDK: `model=` в `prompt(...)` / `Session.create(...)`;
- в задаче: поле `models:` в `task.yaml` (см. шаблон).

**Проверка фактических моделей**: главный агент ОБЯЗАН передавать `model=` каждому сабагенту
из `models.yaml` и записывать фактическую модель в `.code-factory/state/pipeline.yaml`
(`models_used`) и в `report.md` (раздел «Models used»). Если после прогона в `pipeline.yaml`
все роли показывают одну модель — значит, модели не разделялись; проверяй, что запуск CLI
использует ожидаемую модель по умолчанию (`kimi -m …`).

## Коммиты

Фабрика коммитит изменения в feature-ветку. Поле `commit_exclude` в задаче позволяет
исключить файлы из коммита (например, личную стратегию) — фабрика всё равно может их
менять (бэкапы/тесты/откат), но в git-коммит они не попадут.


