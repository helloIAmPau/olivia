# Olivia Harness

A config-driven harness that exposes an LLM agent through pluggable triggers. You
describe your agent and the services that should invoke it in a single YAML file,
and the harness spins up the corresponding endpoints at runtime.

> **Status:** early / work-in-progress. The service scaffolding, config loading, and
> HTTP routing are in place; wiring incoming triggers through to the agent is still
> under construction.

## How it works

The harness reads a configuration file at startup (`/config.yml`), which defines:

- an **agent** with a system prompt, backed by an LLM (served via
  [LiteLLM](https://github.com/BerriAI/litellm)), and
- one or more **services** that expose **triggers** — entry points that hand an
  incoming request to the agent.

The only service type currently implemented is `http`: each HTTP service binds to an
address/port and registers a route per trigger.

```
                ┌─────────────────────────────────────────┐
   HTTP request │                Harness                   │
  ─────────────►│  service (http) ──► trigger ──► agent ──┐ │
                │                                         │ │
                └─────────────────────────────────────────┘
                                                          ▼
                                                   LiteLLM ──► Ollama
```

By default the bundled stack points LiteLLM at [Ollama](https://ollama.com/) so
models run locally; swap the `model_list` in the LiteLLM config to point at a
hosted provider instead if you'd rather not run models locally.

## Configuration

The harness loads its configuration from `/config.yml`. Example:

```yaml
agent:
  prompt: 'You are my best friend and you must handle my entire life'

services:
  # An HTTP service listening on the default address/port (0.0.0.0:80)
  test_http:
    type: 'http'
    triggers:
      incoming_request:
        path: '/testolo'
        prompt: 'You received the following message from the user. Please do something'

  # A second HTTP service on a custom port with multiple triggers
  second_http:
    type: 'http'
    port: 9000
    triggers:
      put_incoming_request:
        method: 'put'
        path: '/puttolo'
        prompt: 'You received the following message from the user. Please do something'
      post_incoming_request:
        method: 'post'
        path: '/postolo'
        prompt: 'You received the following message from the user. Please do something'
```

### Reference

**`agent`**

| Field    | Type   | Required | Description                          |
| -------- | ------ | -------- | ------------------------------------ |
| `prompt` | string | yes      | System prompt given to the agent.    |

**`services.<name>`** — keyed by an arbitrary service name.

| Field   | Type   | Required | Default | Description                          |
| ------- | ------ | -------- | ------- | ------------------------------------ |
| `type`  | string | yes      | —       | Service type. Only `http` is supported. |

**`http` service** (when `type: http`)

| Field      | Type   | Required | Default     | Description                     |
| ---------- | ------ | -------- | ----------- | ------------------------------- |
| `port`     | u16    | no       | `80`        | Port to bind.                   |
| `address`  | string | no       | `0.0.0.0`   | Address to bind.                |
| `triggers` | map    | yes      | —           | Triggers keyed by name.         |

**`http` trigger** — keyed by an arbitrary trigger name.

| Field    | Type   | Required | Default | Description                                            |
| -------- | ------ | -------- | ------- | ------------------------------------------------------ |
| `path`   | string | no       | `/`     | Route path.                                            |
| `method` | string | no       | `GET`   | HTTP method: `GET`, `POST`, `PUT`, or `DELETE`.        |
| `prompt` | string | yes      | —       | Prompt passed to the agent when the trigger fires.     |

## Running

### With Docker Compose (recommended)

The stack runs the harness alongside a LiteLLM proxy and a local Ollama instance.
Startup order is enforced via healthchecks: `ollama` must report healthy before
`litellm` starts, and `litellm` must report healthy before the `harness` starts.

1. Provide the required environment variables (see [Environment](#environment)).
2. Start it:

   ```sh
   make up
   ```

   This runs `docker compose up -d --build`.

3. Pull the model(s) referenced in the LiteLLM config into Ollama (only needed
   once — it's persisted under `data/ollama`):

   ```sh
   docker compose exec ollama ollama pull gemma4:12b
   ```

### Development

The development target mounts the source, enables debug logging, and rebuilds on
change via `cargo-watch`. It also injects an inline test configuration
(see `docker-compose.develop.yml`).

```sh
make develop
```

This sources `./.env.develop` and runs both compose files
(`docker-compose.yml` + `docker-compose.develop.yml`).

### Environment

| Variable             | Used by          | Description                                   |
| -------------------- | ---------------- | --------------------------------------------- |
| `LITELLM_MASTER_KEY` | harness, litellm | Master key for the LiteLLM proxy.             |
| `LITELLM_HOST`       | harness          | Base URL of the LiteLLM proxy. Defaults to `http://litellm:4000`. |
| `RUST_LOG`           | harness          | Log filter (e.g. `info`, `debug`). Defaults to `info`. |

> **Note:** do not commit real secrets. Keep them in a local, git-ignored env file
> and share a placeholder template (e.g. `.env.develop.example`) instead.

## Project layout

```
.
├── docker-compose.yml           # Base stack: harness + litellm + ollama
├── docker-compose.develop.yml   # Dev overrides + inline test config
├── Makefile                     # `make up`, `make develop`
├── data/ollama/                 # Ollama's persisted models/cache (volume mount)
└── harness/                     # Rust service
    ├── Cargo.toml
    ├── Dockerfile
    └── src/
        ├── main.rs              # Entrypoint: logging, config load, startup
        ├── config.rs            # Top-level config model + loader
        ├── agent/               # Agent + LLM client
        │   ├── mod.rs
        │   └── llm_client.rs    # reqwest client for the LiteLLM proxy
        ├── services/            # Service runtime
        │   ├── mod.rs           # Service dispatch (spawns each service)
        │   └── http.rs          # HTTP service + trigger routing
        └── trigger.rs           # Shared trigger config
```

## Tech stack

- **Rust** (edition 2024), async via [Tokio](https://tokio.rs/)
- [axum](https://github.com/tokio-rs/axum) for HTTP
- [reqwest](https://github.com/seanmonstar/reqwest) as the LiteLLM HTTP client
- [serde](https://serde.rs/) + `serde_yaml` for configuration
- [tracing](https://github.com/tokio-rs/tracing) for structured logs
- [LiteLLM](https://github.com/BerriAI/litellm) as the LLM gateway
- [Ollama](https://ollama.com/) as the default local model runtime
