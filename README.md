<div align="center">
  <img src=".github/cat_spotted_grey_moves.gif" alt="Olivia Harness">

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

## What is Olivia?

Olivia is a small, auditable runtime for putting an LLM **in front of real
actions without handing it a shell**. You describe an agent and the services
that invoke it in a single TOML file; the harness exposes the matching
endpoints and routes every request through the agent. The agent never *does* the
work itself — it only decides which **tool** to call, and every tool runs inside
an isolated WebAssembly sandbox.

> The "ia" in Olivia stands for *intelligenza artificiale* — Italian for
> artificial intelligence.

### The aim

Two ideas drive the whole design:

1. **The model is a router, not a solver.** It is instructed to perform *zero*
   internal logic and to accomplish everything by delegating to declared tools.
   Its only possible output is a small JSON envelope that either calls a tool or
   ends the run.
2. **Tools are untrusted code, so they run sandboxed.** Each tool is a
   WebAssembly component executed under a capability-based WASI sandbox that
   grants *nothing* by default. A tool can only touch what the host explicitly
   hands it.

Together these bound the blast radius of both a misbehaving model and a
misbehaving tool: the worst a jailbroken model can do is "call tool `X` with
params `Y`", and the worst a buggy tool can do is limited to the handful of
capabilities it was granted. See [Security model](#security-model).

## Architecture

```
                              tool registry ─► pick + run in a wasm + WASI sandbox ─► SearXNG · Browserless · Postgres · ClickHouse · S3 · /sandbox
                                 ▲   │
  HTTP request ─┐                │   ▼
  Telegram msg ─┼─► service ─►  agent (router) ─► done / error
  cron tick    ─┘                │   ▲
                                 ▼   │
                              LiteLLM ─► Ollama / Anthropic / OpenAI
```

A request enters through a **service**, is handled by the **agent**, and the
agent runs an *agentic loop*:

1. **A service receives a trigger** — an HTTP request, a Telegram message, or a
   cron tick — and calls the agent with the incoming text.
2. **The agent asks the model what to do.** It wraps the request in a strict
   developer prompt (the tool catalogue, the available data stores, the rules)
   and sends the running conversation to the model through the **LiteLLM** proxy,
   which fronts Ollama, Anthropic and OpenAI.
3. **The model replies with a JSON envelope** — nothing else. It is one of:
   - `{"state":"tool", "name":…, "params":…}` — call a tool,
   - `{"state":"done", "result":…}` — finished, here is the answer,
   - `{"state":"error", "message":…}` — give up with a reason.
4. **On `tool`**, the harness looks the tool up in the **registry**, runs it in a
   fresh **wasm + WASI sandbox**, appends the tool's output to the conversation,
   and loops back to step 2.
5. **On `done`/`error`**, the agent returns to the service, which replies to the
   caller. The loop is bounded by `MAX_ITERATIONS` (200).

Supporting pieces:

- **Data stores** — Postgres, ClickHouse and S3-compatible object storage are
  declared in config and surfaced to the model; it reaches them only by passing
  their connection details to the matching tool.
- **The sandbox directory** — a shared `/sandbox` folder is mounted into the
  harness and preopened for every tool, so tools can persist files and hand data
  to one another across steps.
- **Sessions** — every run is tagged with a UUID. When a run completes
  (`done`), the agent saves its full message chain under that id, so a caller can
  resume the same conversation on a later request. See [Sessions](#sessions).

### WIT as the tool contract

The host/guest boundary is described once, as a
[WIT](https://component-model.bytecodealliance.org/design/wit.html) world in
`tools/tool.wit`. Both the harness and every tool generate bindings from it, so
the ABI is checked at compile time rather than hand-marshalled:

```wit
package olivia:tools;

world tool-world {
  enum permission { network, file-system }

  record tool-info {
    name: string,
    description: string,
    schema: string,               // JSON Schema for this tool's params
    permissions: list<permission> // capabilities the sandbox must grant
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
  human-readable `description`, a JSON Schema for its parameters, and the list of
  `permissions` (capabilities) it needs. The harness uses this to build its
  registry, to tell the model which tools exist and how to call them, and to
  decide which capabilities to grant the tool's sandbox — a tool receives nothing
  it did not declare here.
- **`run(input)`** is called when the model routes a request to this tool. It
  receives a JSON string of parameters (matching the schema) and returns a
  `tool-output` whose `state` reports success or failure and whose `content`
  carries the result (or the error message).

Any language that can produce a `wasm32-wasip2` **component** implementing this
world can be a tool — see [Implementing a tool](#implementing-a-tool).

## The host harness

The harness (`harness/`) is an async Rust service built on
[wasmtime](https://wasmtime.dev/). At startup it:

1. loads the config file (`/config.toml` by default, override with
   `--config=<path>`) — agent + services;
2. verifies the configured `model` is registered on the LiteLLM proxy, refusing
   to start otherwise;
3. loads every `*.wasm` from the tools folder (`/tools` by default, override with
   `--tools=<folder>`), calling each tool's `info()` to build the registry and
   the tool catalogue injected into the system prompt.

Each tool file is loaded as a `wasmtime::component::Component`; for every call
the harness instantiates it into a **fresh, sandboxed `Store`**, so tools
accumulate no state across invocations. The runtime is async, so tool calls
never block the request loop.

### Command-line options

Both startup paths default to the container mount points and can be pointed
elsewhere. Options are accepted only in `--key=value` form:

| Option | Default | Description |
| ------ | ------- | ----------- |
| `--config=<path>` | `/config.toml` | The TOML config file to load. |
| `--tools=<folder>` | `/tools` | The folder scanned for `*.wasm` tools. |
| `--sandbox=<folder>` | `/sandbox` | Host base directory for per-session sandboxes; each session gets `<folder>/<session_id>`, mounted into file-system tools at `/sandbox`. |

```sh
harness --config=/examples/hello.toml --tools=/tools
```

Bundled example configs live in `examples/` (mounted read-only into the harness
at `/examples`); `examples/hello.toml` exposes a single HTTP endpoint that
greets a posted name.

## Sessions

Tools are stateless, but a *conversation* is not: a caller often needs to send a
follow-up that builds on what the agent already did. Sessions carry that
context.

Every run is identified by a **session id** (a UUID). Two entry points on the
agent decide whether a run starts fresh or continues an existing one:

- **`accept(request, None)`** mints a new UUID, prepends the router system
  prompt, and runs the agentic loop from scratch.
- **`accept(request, Some(id))`** looks up a stored message chain by id, appends
  the new messages to it, and continues the loop with the full history (an
  unknown id returns an `Invalid session id` error).

When a run finishes with `done`, the agent stores its complete message chain
(system prompt, every model turn, and every tool result) under the session id
before returning. A later `accept` with that id replays the whole exchange, so the
model sees everything that happened. Runs that end in `error` are **not** saved,
and the store is an in-memory map — sessions do not survive a harness restart.

Every reply carries the id back to the caller:

```json
{
  "session_id": "b1f7c0de-1234-4a5b-9c8d-0f1e2d3c4b5a",
  "payload": { "state": "done", "result": "…" }
}
```

Each service exposes this differently:

- **HTTP** — the response is `{ "error": …, "data": { "session_id", "payload" } }`.
  To continue a conversation, send the id back on the next request in the
  `OLIVIA-SESSION-ID` header; the harness parses it and continues that session.
  A request with no (or an unparseable) header starts a fresh session; a
  well-formed id that isn't in the store returns an `Invalid session id` error.
- **Telegram** — `/do <text>` starts a session and the bot remembers it per
  chat, so any **plain follow-up message** continues that conversation
  automatically. Messaging before you have run `/do` replies with `Unable to
  restore session`.
- **cron** — each tick is a standalone `accept`; there is no caller to hand the
  id to, so cron runs are effectively single-shot.

## Configuration

The harness loads its configuration from `/config.toml` (override with
`--config=<path>`):

```toml
[agent]
model = "anthropic/claude-haiku-4-5"   # must be registered in the LiteLLM model_list
prompt = "You must help us with some simple tasks"

# ── Data stores the agent can reach through the matching tool ──

[agent.stores.default]
type = "postgres"
connection_string = "postgresql://user:password@postgres:5432/olivia"
prompt = "Default relational store — create tables and persist data here."

[agent.stores.analytics]
type = "clickhouse"
host = "http://clickhouse:8123"        # must include the http:// or https:// scheme
username = "default"                    # optional, defaults to "default"
password = ""                           # optional, defaults to empty
prompt = "Column-oriented store for large-scale aggregations and reporting."

[agent.stores.files]
type = "s3"
bucket = "olivia-files"
endpoint = "http://rustfs:9000"         # must include the http:// or https:// scheme
region = "us-east-1"                    # optional, defaults to "us-east-1"
access_key = "rustfsadmin"
secret_key = "rustfsadmin"
prompt = "S3-compatible object store for files and blobs."

# ── Services that invoke the agent ──

[services.public_api]                    # an HTTP service (default 0.0.0.0:80)
type = "http"

[services.public_api.endpoints.ask]
method = "post"
path = "/ask"
prompt = "You received a request from the user. Please help."

[services.bot]                           # a Telegram bot
type = "telegram"
token = "123456:ABC-DEF..."              # bot token from @BotFather
prompt = "You are a helpful assistant reachable from Telegram"

[services.nightly]                       # a scheduled trigger
type = "cron"
schedule = "0 0 3 * * *"                 # 6-field cron: sec min hour day month weekday
prompt = "Summarise yesterday's activity and store it."
```

**`[agent]`** — `model` (required): a model id registered in the LiteLLM
`model_list`. `prompt` (required): the agent system prompt.

**`[agent.stores.<name>]`** — an optional data store the agent may use. `type`
selects the backend (and the tool that talks to it) and `prompt` describes the
store to the model. Remaining fields depend on `type`:

| `type`       | Fields |
| ------------ | ------ |
| `postgres`   | `connection_string` |
| `clickhouse` | `host` (with scheme), `username` (default `default`), `password` (default empty) |
| `s3`         | `bucket`, `endpoint` (with scheme), `access_key`, `secret_key`, `region` (default `us-east-1`) |

Each store is listed to the model under **AVAILABLE DATA STORES**; the model
forwards its connection details to the matching tool (`postgres_client` /
`clickhouse_client` / `s3_client`) and is instructed never to leak them.

**`[services.<name>]`** — `type` is `http`, `telegram`, or `cron`:

- **`http`** — `port` (default `80`), `address` (default `0.0.0.0`), and one or
  more `[services.<name>.endpoints.<ep>]` with `path` (default `/`), `method`
  (`GET`/`POST`/`PUT`/`DELETE`, default `GET`) and `prompt`. The request body is
  forwarded to the agent; each endpoint replies with a JSON envelope carrying the
  `session_id` and the agent's `payload`. Pass a prior id back in the
  `OLIVIA-SESSION-ID` header to continue that conversation — see
  [Sessions](#sessions).
- **`telegram`** — `token` (from [@BotFather](https://t.me/BotFather)) and
  `prompt`. Answers `/help` and `/do <text>` (starts a run through the agent);
  plain follow-up messages continue the chat's session — see
  [Sessions](#sessions).
- **`cron`** — `schedule` (a 6-field, seconds-precision cron expression via
  [`tokio-cron-scheduler`](https://docs.rs/tokio-cron-scheduler)) and `prompt`.
  On every tick it runs the agent with the prompt; there is no caller, so the
  result is logged.

## Bundled tools

| Tool | Path | Params | What it does |
| ---- | ---- | ------ | ------------ |
| `hello_tool` | `tools/hello/` | `{ "suffix": … }` | Reference implementation — returns `Hello <suffix>`. |
| `python` | `tools/python/` | `{ "script": … }` | Runs a Python 3 script in an embedded [RustPython](https://rustpython.github.io/) interpreter inside the wasm sandbox. |
| `web_search` | `tools/search/` | `{ "query": … }` | Searches the web via a [SearXNG](https://docs.searxng.org/) instance and returns the JSON results. |
| `web` | `tools/web/` | `{ "code": … }` | Drives a headless browser through the [Browserless](https://www.browserless.io/) `/function` API (stealth mode on): runs a Puppeteer function you supply and returns its output. |
| `http` | `tools/http/` | `{ "method": …, "url": …, "headers": …, "body": … }` | Makes a direct HTTP request to any URL and returns the status line plus the response body. `method` ∈ `GET`/`POST`/`PUT`/`PATCH`/`DELETE`/`HEAD`; optional `headers` (name/value pairs) and `body`. Any status (incl. `4xx`/`5xx`) comes back as a result; only network-level failures error. For pages needing a real browser, use `web`. |
| `download` | `tools/download/` | `{ "code": …, "path": … }` | Triggers a browser download through Browserless `/download` and writes the file's bytes to the absolute sandbox `path` (e.g. `/sandbox/report.pdf`). The `code` must fire a real download (click a link / click an `<a download>`) — `page.goto(fileUrl)` alone does not download. |
| `fs` | `tools/fs/` | `{ "operation": …, "path": …, "kind": …, "content": … }` | Manages files and directories inside the sandbox. `operation` ∈ `list` (entries of `path`, each prefixed `dir `/`file`) / `create` (a `file` or `directory` per `kind`, writing optional `content` and any missing parents) / `delete` (recursive for directories). `path` is an absolute sandbox path, e.g. `/sandbox/reports/2026.txt`. |
| `postgres_client` | `tools/postgres/` | `{ "connection_string": …, "query": … }` | Runs one SQL statement against a PostgreSQL store and returns the result as CSV. Speaks the v3 wire protocol directly over `std::net`. |
| `clickhouse_client` | `tools/clickhouse/` | `{ "host": …, "username": …, "password": …, "query": … }` | Runs one SQL statement against a ClickHouse store over its HTTP interface and returns a `SELECT` as CSV (`default_format=CSVWithNames`). |
| `s3_client` | `tools/s3/` | `{ "bucket": …, "region": …, "endpoint": …, "access_key": …, "secret_key": …, "operation": …, "key": …, "path": … }` | Manages files in an S3-compatible store (SigV4 via [`rusty-s3`](https://docs.rs/rusty-s3)). `operation` ∈ `list` / `create` (upload the sandbox file at absolute `path` to `key`) / `delete` / `download` (save `key` to absolute `path`). |

> The `python` tool runs **builtins-only**: the full language is available, but
> there is no importable standard library (`import json`, `import math`, … raise
> `ModuleNotFoundError`). It returns whatever the script assigns to the global
> `__OLIVIA__FINAL__RESULT__`.

## Implementing a tool

A tool is **any `wasm32-wasip2` component that implements the `tool-world`
world** in `tools/tool.wit`. That makes tools language-agnostic: whatever
toolchain can emit a component works. The recipe is always the same:

1. Generate bindings from `tools/tool.wit` for the world `tool-world`.
2. Implement the two exports:
   - `info()` → `{ name, description, schema, permissions }`, where `schema` is a
     **JSON Schema string** describing the params (the model reads `description` +
     `schema` to learn when and how to call the tool) and `permissions` lists the
     capabilities (`network`, `file-system`) the sandbox must grant,
   - `run(input)` → `{ state, content }`, where `input` is a JSON string of the
     params and `state` is `done` or `error`.
3. Build a component targeting `wasm32-wasip2`.
4. Drop the resulting `*.wasm` into `data/tools/` (mounted read-only into the
   harness at `/tools`). New files are picked up as new tools — no harness
   rebuild required.

> [!NOTE]
> **Rust is the reference, integrated path** (it builds automatically with the
> stack and derives the JSON Schema for you). The other languages below produce
> a standalone component you place in `data/tools/`; they are supported by the
> contract but exercised less, so treat the commands as starting points.

### Rust

Rust tools live in the `tools/` Cargo workspace and compile to components with
[`cargo-component`](https://github.com/bytecodealliance/cargo-component). The
`common` crate provides a `define_tool!` macro that generates the `Guest`
implementation, derives the params' JSON Schema from
[`schemars`](https://docs.rs/schemars), and exports the component.

**1. Register the crate** in `tools/Cargo.toml`:

```toml
[workspace]
members = ["common", "hello", "your-tool"]
resolver = "2"
```

**2. Declare it as a wasm component** (`tools/your-tool/Cargo.toml`):

```toml
[package]
name = "your-tool"
version = "0.0.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
common = { path = "../common" }
wit-bindgen = "0.60.0"
schemars = "1.2.2"
serde_json = "1.0.151"
serde = { version = "1.0.229", features = ["derive"] }
```

**3. Implement it** (`tools/your-tool/src/lib.rs`). Define your params (with a
doc comment per field — it becomes the schema description the model sees), a
`run` function, and hand them to `define_tool!` along with the capabilities the
tool needs (`vec![]` for none):

```rust
use schemars::JsonSchema;
use serde::Deserialize;

use common::define_tool;

#[derive(Deserialize, JsonSchema)]
struct HelloParams {
  /// The value to greet.
  suffix: String
}

fn run(input: HelloParams) -> ToolOutput {
  ToolOutput {
    state: ToolOutputState::Done,
    content: format!("Hello {}", input.suffix)
  }
}

define_tool!(
  HelloTool,                                     // name is snake_cased -> "hello_tool"
  HelloParams,
  "A simple hello-world tool returning hello + a string received as argument",
  vec![],                                        // capabilities, e.g. vec![Permission::Network, Permission::FileSystem]
  run
);
```

`ToolOutput`, `ToolOutputState`, `Permission` and the `Guest`/`export!` glue all
come from the macro. The tool's registered `name` is the struct name in
snake_case, and the `vec![…]` of `Permission`s becomes the `permissions` the
harness grants its sandbox. It builds automatically with `make develop`.

### JavaScript / TypeScript

Use [`jco`](https://github.com/bytecodealliance/jco) with ComponentizeJS.

```sh
npm install -g @bytecodealliance/jco @bytecodealliance/componentize-js
jco componentize tool.js --wit tool.wit --world-name tool-world --out tool.wasm
```

`tool.js` implements the world's exports as named exports (the record becomes a
plain object; the enum becomes the string `"done"`/`"error"`):

```js
export function info() {
  return {
    name: "my_tool",
    description: "What it does and when to use it",
    schema: JSON.stringify({
      type: "object",
      properties: { suffix: { type: "string", description: "The value to greet." } },
      required: ["suffix"],
    }),
    permissions: [],   // e.g. ["network", "file-system"]
  };
}

export function run(input) {
  const { suffix } = JSON.parse(input);
  return { state: "done", content: `Hello ${suffix}` };
}
```

### Python

Use [`componentize-py`](https://github.com/bytecodealliance/componentize-py).

```sh
pip install componentize-py
# inspect the generated stubs for the exact function signatures:
componentize-py -d tool.wit -w tool-world bindings .
# build the component from your module (app.py):
componentize-py -d tool.wit -w tool-world componentize app -o tool.wasm
```

`app.py` implements the world's exported functions, returning the generated
`ToolInfo` / `ToolOutput` types:

```python
import json
from tool_world import exports   # names come from the generated bindings

class ToolWorld(exports.ToolWorld):
    def info(self):
        return exports.ToolInfo(
            name="my_tool",
            description="What it does and when to use it",
            schema=json.dumps({
                "type": "object",
                "properties": {"suffix": {"type": "string"}},
                "required": ["suffix"],
            }),
            permissions=[],   # e.g. [exports.Permission.NETWORK, exports.Permission.FILE_SYSTEM]
        )

    def run(self, input: str):
        params = json.loads(input)
        return exports.ToolOutput(
            state=exports.ToolOutputState.DONE,
            content=f"Hello {params['suffix']}",
        )
```

### Go (TinyGo)

Use [TinyGo](https://tinygo.org/) (≥ 0.33, which targets `wasip2`) with
[`wit-bindgen-go`](https://github.com/bytecodealliance/go-modules).

```sh
go install go.bytecodealliance.org/cmd/wit-bindgen-go@latest
wit-bindgen-go generate --world tool-world --out ./gen ./tool.wit
tinygo build -target=wasip2 --wit-package ./tool.wit --wit-world tool-world -o tool.wasm main.go
```

In `main.go` you assign the generated export hooks in `init()` and return the
generated record types:

```go
package main

import (
    "encoding/json"
    "fmt"
    tool "example.com/tool/gen/olivia/tools/tool-world"
)

func init() {
    tool.Exports.Info = func() tool.ToolInfo {
        return tool.ToolInfo{
            Name:        "my_tool",
            Description: "What it does and when to use it",
            Schema:      `{"type":"object","properties":{"suffix":{"type":"string"}},"required":["suffix"]}`,
            Permissions: nil, // e.g. []tool.Permission{tool.PermissionNetwork, tool.PermissionFileSystem}
        }
    }
    tool.Exports.Run = func(input string) tool.ToolOutput {
        var p struct{ Suffix string `json:"suffix"` }
        _ = json.Unmarshal([]byte(input), &p)
        return tool.ToolOutput{State: tool.ToolOutputStateDone, Content: fmt.Sprintf("Hello %s", p.Suffix)}
    }
}

func main() {}
```

### C / C++

Use [`wit-bindgen`](https://github.com/bytecodealliance/wit-bindgen) with the
[wasi-sdk](https://github.com/WebAssembly/wasi-sdk) and
[`wasm-tools`](https://github.com/bytecodealliance/wasm-tools).

```sh
wit-bindgen c --world tool-world tool.wit          # emits tool-world.c / .h
# implement the exported functions in tool.c, then:
clang --target=wasm32-wasip2 -mexec-model=reactor tool.c tool-world.c -o core.wasm
wasm-tools component new core.wasm -o tool.wasm
```

You implement the generated `exports_*_info` / `exports_*_run` functions,
filling the out-params with the `tool_info` / `tool_output` structs the header
declares.

### Other languages

The component model keeps growing: **C#/.NET** via
[`componentize-dotnet`](https://github.com/bytecodealliance/componentize-dotnet),
and others as their toolchains mature. Anything that emits a `wasm32-wasip2`
component implementing `tool-world` works — generate bindings from `tool.wit`,
implement `info`/`run`, build, and drop the `.wasm` in `data/tools/`.

### Capabilities & the sandbox

Tools run with **no ambient authority**. Each grant is opted into by the tool's
declared `permissions` (see [WIT as the tool contract](#wit-as-the-tool-contract)):
`ToolState::new()` reads that list and hands the WASI context only what it names.

Granted to **every** tool:

- **stdout / stderr** — inherited, so a tool's logging surfaces in the harness logs.
- **Selected env vars** — every host variable named `OLIVIA_TOOL_<NAME>` is
  forwarded to the tool as `<NAME>` (prefix stripped). Nothing else from the host
  environment is visible.

Granted **only when declared**:

- **`network`** → `inherit_network()` + name resolution: raw outbound TCP and
  DNS, plus the linked-in `wasmtime-wasi-http` for HTTP. Declared by the
  networked tools (`web_search`, `web`, `http`, `download`, `postgres_client`,
  `clickhouse_client`, `s3_client`).
- **`file-system`** → a per-session `/sandbox` directory, preopened read/write;
  the only writable path. Declared by `fs`, `python`, `download` and `s3_client`,
  which use it to persist files and exchange data across steps (e.g. `download`
  writes a file, `python` reads it, `s3_client` uploads it). On the host each
  session lives under a private subdirectory (`<sandbox>/<session_id>`) but every
  tool sees it as `/sandbox`.

Always **denied**: any capability a tool did not declare, any other filesystem
path, inbound sockets, process args, and host env vars without the
`OLIVIA_TOOL_` prefix. Each invocation gets a fresh store, so no state leaks
between calls.

## Security model

The threat model is simple: **an LLM cannot be trusted to be correct, and it
cannot be trusted with arbitrary execution.** Olivia contains that with defense
in depth.

**Layer 1 — the model has no execution surface of its own.** The developer
prompt makes it a strict router: zero internal logic, always delegate, and emit
*only* the JSON envelope. The one thing it can cause is "call tool `X` with
params `Y`", and the harness validates the tool name against the registry, so it
cannot invent a tool that isn't there.

**Layer 2 — tools are sandboxed WebAssembly, deny-by-default.** Every tool is a
memory-isolated component holding only the capabilities listed above. There is
no path from "clever prompt" to arbitrary code on the host or an un-granted
resource.

Even in the worst case — the model is fully jailbroken *and* a tool has a bug —
the damage is bounded to the small, enumerated capability set.

## Building and running

The stack runs the harness alongside a LiteLLM proxy, a local Ollama instance,
the tools builder, and the backing services the tools reach: SearXNG,
Browserless, Postgres, ClickHouse and a [RustFS](https://rustfs.com/)
S3-compatible store. Startup order is enforced with healthchecks.

```sh
make develop        # sources .env.develop, then docker compose up --build
```

This mounts the source, enables debug logging, rebuilds on change via
`cargo-watch`, and injects an inline test configuration. Pull the Ollama
model(s) referenced by the LiteLLM config once:

```sh
docker compose exec ollama ollama pull gemma4:12b
```

| Command | Description |
| ------- | ----------- |
| `make develop` | Build and run the full stack for local development. |
| `make build`   | Build the `olivia/harness` image. |
| `make bump <patch\|minor\|major>` | Rewrite the version in tracked manifests, commit, and tag. |

### How tools are built

The `tools` Docker stage runs `cargo component build --release
--target=wasm32-wasip2` over the Rust workspace and copies every `*.wasm` into
`/tools`; in dev, `cargo-watch` rebuilds on change. Components authored in other
languages are dropped into `data/tools/` directly.

### Environment

| Variable | Used by | Description |
| -------- | ------- | ----------- |
| `LITELLM_MASTER_KEY` | harness, litellm | Master key for the LiteLLM proxy. |
| `LITELLM_HOST` | harness | Base URL of the proxy (default `http://litellm:4000`). |
| `RUST_LOG` | harness | Log filter (e.g. `info`, `debug`). |
| `OLIVIA_ANTHROPIC_KEY` | litellm | API key for the `anthropic/*` models. |
| `OPENAI_API_KEY` | litellm | API key for the `openai/*` models. |
| `SEARXNG_TOKEN` | searxng | `secret_key` for the SearXNG instance. |
| `BROWSERLESS_TOKEN` | browserless | Auth token for Browserless (also forwarded to `web`/`download`). |
| `POSTGRES_*` / `CLICKHOUSE_*` / `RUSTFS_*` / `S3_BUCKET` | stores | Credentials for the bundled data stores. |
| `OLIVIA_TOOL_<NAME>` | harness → tools | Any var with this prefix is forwarded into every tool as `<NAME>`. |

Tools read plainly-named variables (`BROWSERLESS_HOST`, `BROWSERLESS_TOKEN`,
`SEARXNG_HOST`, …); the harness exposes them by setting `OLIVIA_TOOL_<NAME>` on
its own environment.

> [!WARNING]
> Do not commit real secrets. Keep them in a local, git-ignored env file and
> share a placeholder template (e.g. `.env.develop.example`) instead.

## License

No license has been chosen for this project yet. Until a `LICENSE` file is added,
all rights are reserved by the authors.

### Contribution

Issues and pull requests are welcome. A good first contribution is a new tool
under `tools/` (see [Implementing a tool](#implementing-a-tool)) or a guest
example in a language other than Rust.
