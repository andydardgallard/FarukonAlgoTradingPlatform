# Tech Stack Detection

Goal: quickly and reliably determine the programming languages, frameworks, build tools and test
commands of a project — for any language or combination of languages. The factory is
language-agnostic; never assume a stack.

## 1. Fast signals (parallel Glob/Grep)

Run these in parallel (use subagents when the project is large):

| Signal file(s) | Stack |
|----------------|-------|
| `Cargo.toml` + `*.rs` | Rust |
| `pyproject.toml`, `requirements.txt`, `setup.py`, `setup.cfg`, `Pipfile`, `poetry.lock`, `uv.lock` | Python |
| `package.json` (check `devDependencies`: typescript? vitest? jest?) | JavaScript / TypeScript |
| `go.mod` + `*.go` | Go |
| `pom.xml`, `build.gradle`, `build.gradle.kts`, `*.java` | Java (Maven / Gradle) |
| `*.csproj`, `*.sln`, `*.cs` | C# / .NET |
| `CMakeLists.txt`, `Makefile`, `*.c`, `*.cpp`, `*.h` | C / C++ |
| `Gemfile`, `*.rb` | Ruby |
| `composer.json`, `*.php` | PHP |
| `mix.exs`, `*.ex`, `*.exs` | Elixir |
| `*.swift`, `Package.swift` | Swift |
| `go.work` | Go workspace |
| `Dockerfile`, `docker-compose.yml` | Containerized (still inspect the language files) |
| `.github/workflows/`, `.gitlab-ci.yml`, `Jenkinsfile`, `.circleci/` | CI — read to learn canonical build/test commands |
| `Makefile` (root) | Often the canonical entry point: `make test`, `make build` |

## 2. Multi-language projects

- A project may mix languages (e.g., Rust core + Python bindings, TS frontend + Go backend).
- Identify each component, its test command, and its relation to the business task.
- Keep the analysis per component: `frontend/` vs `backend/` vs `core/`.

## 3. Test / build / run commands per stack

| Stack | Install deps | Build | Run tests | Run app |
|-------|-------------|-------|-----------|---------|
| Rust | `cargo build` | `cargo build` | `cargo test` | `cargo run [-- args]` or the binary |
| Python | `pip install -e .` / `uv sync` / `poetry install` | — | `pytest` / `python -m pytest` | `python -m <package>` / entry script |
| Node/TS | `npm ci` / `pnpm install` / `yarn` | `npm run build` / `tsc` | `npm test` / `npx vitest run` / `npx jest` | `npm start` / `node dist/index.js` |
| Go | `go mod download` | `go build ./...` | `go test ./...` | `go run .` / built binary |
| Java/Maven | `mvn dependency:resolve` | `mvn compile` | `mvn test` | `mvn exec:java` / `java -jar target/*.jar` |
| Java/Gradle | `./gradlew build` | `./gradlew build` | `./gradlew test` | `./gradlew run` |
| C#/.NET | `dotnet restore` | `dotnet build` | `dotnet test` | `dotnet run` |
| C/C++ | `cmake -B build && cmake --build build` / `make` | same | `ctest` / custom | built binary |
| Ruby | `bundle install` | — | `bundle exec rspec` / `rake test` | `bundle exec ruby main.rb` |
| PHP | `composer install` | — | `vendor/bin/phpunit` | `php -S ...` / `php index.php` |
| Elixir | `mix deps.get` | `mix compile` | `mix test` | `mix run -e "..."` / `mix phx.server` |
| Swift | `swift package resolve` | `swift build` | `swift test` | `swift run` |

For anything unusual, read the CI workflow or README to find the canonical commands. If tests
cannot run in this environment (missing toolchain), say so explicitly and record it as a
constraint instead of guessing.

## 4. Baseline (regression reference)

Before any change:

1. Run the full test suite with the canonical command.
2. Save the result (pass/fail + summary) to `.code-factory/logs/baseline.md`.
3. Note the runtime/toolchain versions (e.g., `cargo --version`, `python --version`).

The baseline is the definition of "no regression" for the whole task.

## 6. Entry points and configs (needed for business tests)

- Identify how the program is launched and which configuration / input files it consumes
  (e.g., config.toml, CLI args, data files).
- List them — the business-test phase will ask the user which specific configs to run.

## 7. Scout pipeline (Phase 1)

After the deterministic steps above, run the Scout pipeline to produce a durable project
model + AGENTS.md that every later phase can rely on:

1. **Deterministic collection (no LLM)**: stack (from sections 1–3), top-level structure
   (source dirs, config files), known issues (e.g. `git ls-files target/` → build artifacts
   tracked in git).
2. **LLM refinement (cheap model)**: explore README, build configs, entry points,
   cross-component dependencies; refine the model.
3. **Generate AGENTS.md** in the repo root with EXACTLY these 8 `##` sections:
   `Project Overview`, `Technology Stack`, `Architecture Overview`, `Directory Structure`,
   `Key Configuration Files`, `Build & Run Instructions`, `Dependencies & Integrations`,
   `Known Constraints & Limitations`. Copy build/test/run commands VERBATIM from config files;
   record observed bugs/debt in section 8; keep sections self-contained.
4. **Run native `/init kimi`** so Kimi adapts AGENTS.md to its own tooling (skip gracefully
   if the CLI is unavailable).

Cache: keep the project model in `.code-factory/state/project-model.yaml`. Skip the whole
Scout phase if AGENTS.md already exists and the project is unchanged (the file is the Single
Source of Truth for BA/Planner/Coder).

