<div align="center">
  <h1><code>Olivia Harness</code></h1>

  <strong>A config-driven harness that runs an LLM as a safe, sandboxed task coordinator</strong>

  <p>
    An untrusted model that only <em>routes</em> requests, and tools that run as
    isolated WebAssembly components.
  </p>

  <p>
    <img src="https://img.shields.io/badge/rust-edition%202024-orange" alt="Rust edition 2024">
    <img src="https://img.shields.io/badge/plugins-WebAssembly%20components-654ff0" alt="WebAssembly components">
    <img src="https://img.shields.io/badge/runtime-wasmtime-4d4dff" alt="wasmtime">
    <img src="https://img.shields.io/badge/status-WIP-yellow" alt="Status: WIP">
  </p>
</div>

## About

Olivia is a small, auditable runtime for putting an LLM in front of real actions
**without handing it a shell**. You describe an agent and the services that
invoke it in a single TOML file; the harness exposes the matching endpoints and
routes every request through the agent. The agent never *does* the work itself —
it only decides which **tool** to call, and every tool runs inside an isolated
WebAssembly sandbox.

> The "ia" in Olivia stands for *intelligenza artificiale* — Italian for
> artificial intelligence.

Two ideas drive the whole design:

1. **The model is a router, not a solver.** It is instructed to perform *zero*
   internal logic and to accomplish everything by delegating to declared tools.
2. **Tools are untrusted code, so they run sandboxed.** Each tool is a
   WebAssembly component executed under a capability-based WASI sandbox that
   grants *nothing* by default.

Together these bound the blast radius of both a misbehaving model and a
misbehaving tool — see [Security model](#security-model).

```
                              tool registry ─► pick + run in a wasm + WASI sandbox ─► SearXNG · Browserless · Postgres · ClickHouse · S3
                                 ▲   │
  HTTP request ─┐                │   ▼
  Telegram msg ─┼─► service ─►  agent (router) ─► done / error
  cron tick    ─┘                │   ▲
                                 ▼   │
                              LiteLLM ─► Ollama / Anthropic
```

The agent (self-identified to the model as "OlivIA") wraps each request in a
developer prompt that forces the model to answer **only** with a small JSON
envelope: `{"state":"tool", …}` to call a tool, `{"state":"done", …}` on
success, or `{"state":"error", …}` to give up. Each iteration the agent asks the
model what to do next; when the model asks for a tool, the harness runs it in its
sandbox and appends the result to the conversation, looping until the model
returns `done`/`error` or `MAX_ITERATIONS` (200) is reached.

> [!NOTE]
> **Status:** early / work-in-progress. Config loading, the HTTP and Telegram
> services, service-to-agent wiring, the WASM tool runtime (wasmtime component
> model + WASI), harness-side dispatch of a chosen tool (`ToolRegistry::run`),
> and the auto-generated tool catalogue in the system prompt are all in place.
> Tools can now reach the network over WASI-HTTP, which powers the bundled
> `web_search` and `web` tools. Tools can also talk to data stores declared
> under `[agent.stores]` — the bundled `postgres_client` (wire protocol over raw
> TCP), `clickhouse_client` (HTTP interface) and `s3_client` (S3-compatible
> object storage) tools.

## WIT as the tool contract

The host/guest boundary is described once, as a
[WIT](https://component-model.bytecodealliance.org/design/wit.html) world in
`tools/tool.wit`. Both the harness and every tool generate bindings from it, so
the ABI is checked at compile time rather than hand-marshalled:

```wit
package olivia:tools;

world tool-world {
  record tool-info {
    name: string,
    description: string,
    schema: string          // JSON Schema for this tool's params
  }

  enum tool-output-state { done, error }

  record tool-output {
    state: tool-output-state,
    content: string
  }

  export info: func() -> tool-info;
  export run: func(input: string) -> tool-output;
}
```

- **`info()`** is called once at load time and returns the tool's `name`,
  human-readable `description`, and a JSON Schema for its parameters. The harness
  uses this to build its registry and to tell the model which tools exist and how
  to call them.
- **`run(input)`** is called when the model routes a request to this tool. It
  receives a JSON string of parameters (matching the schema) and returns a
  `tool-output` whose `state` reports success or failure.

## Creating a Tool

Tools live in the `tools/` Cargo workspace and compile to WebAssembly components
targeting `wasm32-wasip2`. The bundled `hello` tool (`tools/hello/`) is the
reference implementation.

**1. Create the crate** under `tools/` and register it in the workspace:

```toml
# tools/Cargo.toml
[workspace]
members = ["hello", "your-tool"]
resolver = "2"
```

**2. Declare it as a wasm component:**

```toml
# tools/your-tool/Cargo.toml
[package]
name = "your-tool"
version = "0.0.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = "0.60.0"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "1"        # optional: derive the params JSON Schema
```

**3. Implement the world.** Generate bindings from the shared WIT, implement the
`Guest` trait's `info` and `run`, and export your type:

```rust
// tools/your-tool/src/lib.rs
use wit_bindgen::generate;

generate!({
  world: "tool-world",
  path: "../tool.wit"
});

struct YourTool;

impl Guest for YourTool {
  fn info() -> ToolInfo {
    ToolInfo {
      name: "your_tool".to_string(),
      description: "What it does and when to use it".to_string(),
      schema: "{ \"type\": \"object\", \"properties\": { /* ... */ } }".to_string(),
    }
  }

  fn run(input: String) -> ToolOutput {
    // parse `input` (JSON matching your schema), do the work,
    // then report success or failure via the state.
    ToolOutput {
      state: ToolOutputState::Done,
      content: format!("handled: {}", input),
    }
  }
}

export!(YourTool);
```

**4. Build.** The `tools-builder` service compiles the workspace and drops the
component into `data/tools`, from which the harness loads it. A new `.wasm`
appearing there is picked up as a new tool — no harness rebuild required.

```sh
make develop        # builds + runs the whole stack; rebuilds tools on change
```

> [!NOTE]
> The `name` you return from `info()` is the identifier the model uses in the
> `"name"` field of a tool call; `description` and `schema` are what teach the
> model when and how to call it. Write them for a reader who only sees the
> catalogue, not your code.

### Creating tools: WASI & the sandbox

WebAssembly gives memory isolation and no ambient authority; host capabilities
are handed over explicitly through [WASI](https://wasi.dev/). The harness builds
each tool's context in `ToolState::new()`, and today it grants a **deliberately
small, fixed set** of capabilities to every tool:

- **stdout / stderr** — inherited, so a tool's logging surfaces in the harness logs.
- **Selected environment variables** — every host variable named
  `OLIVIA_TOOL_<NAME>` is forwarded to the tool as `<NAME>` (prefix stripped).
  Nothing else from the host environment is visible.
- **Outbound HTTP** — `wasmtime-wasi-http` is linked into every tool, so tools
  can make HTTP requests (this is what `web_search` and `web` use to reach
  SearXNG and Browserless, what `clickhouse_client` uses to reach ClickHouse's
  HTTP interface, and what `s3_client` uses to reach an S3-compatible store).
- **Outbound TCP + DNS** — `inherit_network()` grants raw outbound sockets and
  name resolution, so socket-based tools can reach networked services (this is
  what `postgres_client` uses to speak the wire protocol to a database).

Still **denied**: filesystem preopens, inbound sockets, process args, and any
host environment variable without the `OLIVIA_TOOL_` prefix. Every invocation
also gets a **fresh store**, so tools accumulate no state across calls.

> [!WARNING]
> Outbound HTTP **and** raw outbound TCP/DNS are currently granted to **all**
> tools, not opted in per tool — a deliberate widening of the trust boundary to
> support the network tools. If you need finer-grained control (a preopened
> directory, a clock, or restricting egress), gate it on the host side in
> `ToolState::new()` (e.g. swap `inherit_network()` for `allow_tcp` +
> `socket_addr_check`) / the linker. Treat every grant as widening the trust
> boundary.

### Bundled tools

| Tool         | Path            | Params            | What it does                                                                 |
| ------------ | --------------- | ----------------- | --------------------------------------------------------------------------- |
| `hello_tool` | `tools/hello/`  | `{ "suffix": … }` | Reference implementation — returns `Hello <suffix>`.                        |
| `python`     | `tools/python/` | `{ "script": … }` | Runs a Python 3 script in an embedded [RustPython](https://rustpython.github.io/) interpreter, inside the same wasm sandbox. |
| `web_search` | `tools/search/` | `{ "query": … }`  | Searches the web via a [SearXNG](https://docs.searxng.org/) instance (`SEARXNG_HOST`, default `http://searxng:8080`) and returns the JSON results. |
| `web`        | `tools/web/`    | `{ "code": … }`   | Drives a headless browser through the [Browserless](https://www.browserless.io/) `/function` API (`BROWSERLESS_HOST`/`BROWSERLESS_TOKEN`): runs a Puppeteer function you supply and returns its output. |
| `postgres_client` | `tools/postgres/` | `{ "connection_string": …, "query": … }` | Runs a single SQL statement against a PostgreSQL store (the `connection_string` comes from an `[agent.stores]` entry) and returns the result as CSV (a header row of column names followed by one row per record). Speaks the v3 wire protocol directly over `std::net` (cleartext-password or trust auth; no SCRAM, no TLS). |
| `clickhouse_client` | `tools/clickhouse/` | `{ "host": …, "username": …, "password": …, "query": … }` | Runs a single SQL statement against a ClickHouse store (the `host`/`username`/`password` come from an `[agent.stores]` entry) over the HTTP interface and returns a `SELECT`'s result as CSV (a header row of column names followed by one row per record). Requests `default_format=CSVWithNames`, so result-less statements (`CREATE`, `INSERT`, …) stay valid. `host` must carry an explicit `http://`/`https://` scheme. |
| `s3_client` | `tools/s3/` | `{ "bucket": …, "region": …, "endpoint": …, "access_key": …, "secret_key": …, "operation": …, "key": …, "content": … }` | Manages files in an S3-compatible object store (AWS S3, [RustFS](https://rustfs.com/), MinIO, …) — the connection fields come from an `[agent.stores]` entry. `operation` is one of `list` (list the bucket), `create` (upload `content` to `key`), `delete` (remove `key`) or `download` (return `key`'s contents). Signs each request with SigV4 ([`rusty-s3`](https://docs.rs/rusty-s3), Sans-IO) and sends it over HTTP; `endpoint` must carry an explicit `http://`/`https://` scheme. |

The `python` tool returns whatever the script assigns to the global
`__OLIVIA__FINAL__RESULT__` (coerced to a string); `print()` and `return` are not
used for the result.

> [!NOTE]
> `python` runs **builtins-only**: the full language is available (variables,
> arithmetic, `str`/`list`/`dict`, comprehensions, functions, classes,
> exceptions), but there is **no importable standard library** — `import math`,
> `import json`, etc. raise `ModuleNotFoundError`. The released
> `rustpython-stdlib 0.5.0` does not compile (it pulls two incompatible
> `malachite-bigint` versions), so the interpreter is built with
> `rustpython-vm` alone. Restoring the stdlib means pinning RustPython to a fixed
> git revision, or waiting for a corrected `rustpython-stdlib` release.

## Supported Guest Languages

A tool is just a component implementing `tool-world`, so any toolchain that
produces a `wasm32-wasip2` component can build one.

### Guest: Rust

The supported and reference path today.
[`wit-bindgen`](https://github.com/bytecodealliance/wit-bindgen) generates the
`Guest` trait from the WIT, and
[`cargo-component`](https://github.com/bytecodealliance/cargo-component) builds
the component — see [Creating a Tool](#creating-a-tool).

### Guest: Other Languages

Not yet exercised in this repo, but the contract is language-agnostic: C, Go,
TinyGo, JS, and others with component toolchains can target the same
`tool-world`. Contributions adding examples are welcome.

## The Host Harness

The harness (`harness/`) is an async Rust service. At startup it:

1. loads `/config.toml` (agent + services);
2. verifies the configured `model` is registered on the LiteLLM proxy, refusing
   to start otherwise; and
3. loads every `*.wasm` from `/tools`, calling each tool's `info()` to build the
   registry.

Tools are executed with [wasmtime](https://wasmtime.dev/): each file is loaded as
a `wasmtime::component::Component`, and for every call the harness instantiates
it into a fresh, sandboxed `Store` (via `wasmtime-wasi`) and invokes the
generated `call_info` / `call_run` bindings. The runtime is async, so tool calls
never block the request loop. Outbound HTTP is linked in via
[`wasmtime-wasi-http`](https://docs.rs/wasmtime-wasi-http), so tools can make
network requests through WASI — see [the sandbox](#creating-tools-wasi--the-sandbox)
for exactly which capabilities each tool receives.

## Security model

The threat model is simple: **an LLM cannot be trusted to be correct, and it
cannot be trusted with arbitrary execution.** A model can be jailbroken by a
crafted request, can hallucinate actions, or can just be wrong. Olivia contains
that with defense in depth.

### Layer 1 — the model has no execution surface of its own

The agent's developer prompt makes the model a *strict router*:

- **Zero internal logic** — it must not calculate, summarize, or answer from its
  own knowledge.
- **Always delegate** — any action, lookup, or computation *must* go through a
  declared tool; its default response state is "call a tool."
- **Strict JSON only** — it can only emit the response envelope, never free-form
  commands, code, or prose the harness would act on.

The only thing the model can cause is "call tool `X` with params `Y`," and the
harness validates the tool name against the registry, so the model cannot invent
a tool that isn't there.

### Layer 2 — tools are sandboxed WebAssembly, deny-by-default

Every tool runs as a memory-isolated WebAssembly component with **no ambient
authority**: it holds only the capabilities the harness explicitly hands to its
WASI context (see [the sandbox](#creating-tools-wasi--the-sandbox)). That set is
deliberately small and enumerated on the host — currently stdout/stderr, the
`OLIVIA_TOOL_*` environment variables, and outbound HTTP — with the filesystem
and everything else denied. Access is something you *hand over*, not something
tools *have*.

### Why this matters

Even in the worst case — the model is fully jailbroken *and* a tool has a bug —
the damage is bounded to what tools were explicitly granted. The model can't step
outside "pick a tool," and a tool can't touch anything outside its sandboxed
capability set: at most it can log, read its `OLIVIA_TOOL_*` config, and make
outbound HTTP requests. There is no path from "clever prompt" to "arbitrary code
on the host" or the host filesystem.

## Configuration

The harness loads its configuration from `/config.toml`:

```toml
[agent]
model = "ollama/gemma4"
prompt = "You must help us with some simple tasks"

# Data stores the agent can read from / write to via the matching tool
[agent.stores.default]
type = "postgres"
connection_string = "postgresql://user:password@postgres:5432/olivia"
prompt = "Default relational store — create tables and persist data here."

[agent.stores.analytics]
type = "clickhouse"
host = "http://clickhouse:8123"   # must include the http:// or https:// scheme
username = "default"               # optional, defaults to "default"
password = ""                      # optional, defaults to empty
prompt = "Column-oriented store for large-scale aggregations and reporting."

[agent.stores.files]
type = "s3"
bucket = "olivia-files"
endpoint = "http://rustfs:9000"    # must include the http:// or https:// scheme
region = "us-east-1"               # optional, defaults to "us-east-1"
access_key = "rustfsadmin"
secret_key = "rustfsadmin"
prompt = "S3-compatible object store for files and blobs."

# An HTTP service on the default address/port (0.0.0.0:80)
[services.test_http]
type = "http"

[services.test_http.endpoints.incoming_request]
path = "/testolo"
prompt = "You received the following message from the user. Please do something"

# A second HTTP service on a custom port
[services.second_http]
type = "http"
port = 9000

[services.second_http.endpoints.hello]
method = "put"
path = "/hello"
prompt = "Generate a classic Hello Something using the content of the request"

# A Telegram bot service
[services.my_bot]
type = "telegram"
token = "123456:ABC-DEF..."   # bot token from @BotFather
prompt = "You are a helpful assistant reachable from Telegram"

# A cron service that wakes the agent on a schedule
[services.hourly_report]
type = "cron"
schedule = "0 0 * * * *"   # 6-field cron (sec min hour day month weekday): top of every hour
prompt = "Summarise what happened in the last hour and store it."
```

**`[agent]`** — `model` (string, required): a model ID registered in the LiteLLM
`model_list`. `prompt` (string, required): the agent system prompt.

**`[agent.stores.<name>]`** — an optional data store the agent may use. `type`
(string, required) selects the backend and the tool that talks to it, and
`prompt` (string, required) describes the store to the model so it can pick the
right one. The remaining fields depend on `type`:

- **`postgres`** — `connection_string` (required), e.g.
  `postgresql://user:pass@host:5432/db`.
- **`clickhouse`** — `host` (required, must include an explicit `http://` or
  `https://` scheme, e.g. `http://clickhouse:8123`), `username` (optional,
  default `default`), `password` (optional, default empty).
- **`s3`** — `bucket` (required), `endpoint` (required, must include an explicit
  `http://` or `https://` scheme, e.g. `http://rustfs:9000`), `access_key`
  (required), `secret_key` (required), `region` (optional, default `us-east-1`).

Each store is listed to the model under **AVAILABLE DATA STORES** in the system
prompt; the model forwards its connection details to the matching tool
(`postgres_client` / `clickhouse_client` / `s3_client`) and is instructed never
to leak them back into a reply.

**`[services.<name>]`** — `type` (string, required): `http`, `telegram`, or
`cron`.

**`http` service** — `port` (u16, default `80`), `address` (string, default
`0.0.0.0`), `endpoints` (table, required).

**`http` endpoint** — under `[services.<name>.endpoints.<endpoint>]`: `path`
(default `/`), `method` (`GET`/`POST`/`PUT`/`DELETE`, default `GET`), `prompt`
(required). Each endpoint registers one route; the request body, if any, is
forwarded to the agent as context.

**`telegram` service** — `token` (string, required): the bot token from
[@BotFather](https://t.me/BotFather). `prompt` (string, required): the system
prompt. The bot answers two commands: `/help` (lists commands) and `/do <text>`,
which runs `<text>` through the agent and replies with the result in chat. Plain
(non-command) messages are received and logged, but not yet routed to the agent.

**`cron` service** — `schedule` (string, required): a cron expression in the
6-field, seconds-precision form used by
[`tokio-cron-scheduler`](https://docs.rs/tokio-cron-scheduler) —
`sec min hour day-of-month month day-of-week` (a 7th field for the year is also
accepted). `prompt` (string, required): the instruction handed to the agent on
each activation. On every tick the service invokes the agent with a system
message noting the job name and schedule, followed by `prompt`; there is no
inbound request and no caller, so the run's result is logged rather than
returned. Use it for recurring background work — periodic reports, polling,
cache warming, and the like.

**HTTP response** — every `http` endpoint responds with a JSON envelope (Telegram
replies in chat instead). `tool` is an internal loop state; callers only ever see
`done` or `error`:

```jsonc
// success
{ "error": null, "data": { "state": "done", "result": "..." } }
// agent-reported failure
{ "error": null, "data": { "state": "error", "message": "..." } }
// request/parsing error after all retries
{ "error": "<message>", "data": null }
```

## Building and Testing

The stack runs the harness alongside a LiteLLM proxy, a local Ollama instance,
the `tools-builder`, and the backing services the bundled tools reach over
WASI-HTTP: a [SearXNG](https://docs.searxng.org/) search engine (for `web_search`)
and a [Browserless](https://www.browserless.io/) headless-Chromium instance (for
`web`). Startup order is enforced via healthchecks: `ollama` healthy before
`litellm`, and `litellm` + `searxng` healthy before `harness`, which also waits
for `tools-builder` so `/tools` is populated.

```sh
make develop        # build + run the full stack for local development
```

This sources `./.env.develop` and runs `docker compose up --build`, mounting the
harness source, enabling debug logging, rebuilding on change via `cargo-watch`,
and injecting an inline test configuration.

Pull the model(s) referenced in the LiteLLM config into Ollama (once — persisted
under `data/ollama`):

```sh
docker compose exec ollama ollama pull gemma4:12b
```

| Command        | Description                                         |
| -------------- | --------------------------------------------------- |
| `make develop` | Build and run the full stack for local development. |
| `make build`   | Build the `olivia/harness` image.                   |

**Environment**

| Variable                        | Used by          | Description                                                                          |
| ------------------------------- | ---------------- | ----------------------------------------------------------------------------------- |
| `LITELLM_MASTER_KEY`            | harness, litellm | Master key for the LiteLLM proxy.                                                    |
| `LITELLM_HOST`                  | harness          | Base URL of the LiteLLM proxy. Defaults to `http://litellm:4000`.                   |
| `RUST_LOG`                      | harness          | Log filter (e.g. `info`, `debug`). Defaults to `info`.                              |
| `ANTHROPIC_KEY`                 | litellm          | API key for the `anthropic/*` entries in the LiteLLM `model_list`. Required when the agent's `model` is an Anthropic model. |
| `SEARXNG_TOKEN`                 | searxng          | `secret_key` for the SearXNG instance.                                              |
| `BROWSERLESS_TOKEN`             | browserless      | Auth token for Browserless. Also forwarded to the `web` tool (see below).           |
| `OLIVIA_TOOL_<NAME>`            | harness → tools  | Any var with this prefix is forwarded into every tool as `<NAME>` (prefix stripped). |

Tools read plainly-named variables; the harness exposes them by setting
`OLIVIA_TOOL_<NAME>` on its own environment. The bundled network tools use:

| Tool var            | Set on harness as               | Default                    | Description                          |
| ------------------- | ------------------------------- | -------------------------- | ------------------------------------ |
| `SEARXNG_HOST`      | `OLIVIA_TOOL_SEARXNG_HOST`      | `http://searxng:8080`      | Base URL of SearXNG (`web_search`).  |
| `BROWSERLESS_HOST`  | `OLIVIA_TOOL_BROWSERLESS_HOST`  | `http://browserless:3000`  | Base URL of Browserless (`web`).     |
| `BROWSERLESS_TOKEN` | `OLIVIA_TOOL_BROWSERLESS_TOKEN` | — (required by `web`)      | Browserless auth token (`web`).      |

> [!WARNING]
> Do not commit real secrets. Keep them in a local, git-ignored env file and
> share a placeholder template (e.g. `.env.develop.example`) instead.

## Versioning and Releases

Versions are derived from git tags. `make bump <patch|minor|major>` rewrites the
version in the tracked manifests, commits the change, and creates the matching
`vX.Y.Z` tag; the build derives the image `VERSION` from `git describe`.

## License

No license has been chosen for this project yet. Until a `LICENSE` file is added,
all rights are reserved by the authors.

### Contribution

Issues and pull requests are welcome. A good first contribution is a new tool
under `tools/` (see [Creating a Tool](#creating-a-tool)) or a guest example in a
language other than Rust.
