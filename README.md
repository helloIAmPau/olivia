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
   HTTP request        Harness
  ─────────────►  service ─► trigger ─► agent (router) ──┐  "tool"
                                            ▲            │
                                            │            ▼
                              tool registry ◄──── pick + run in a
                             (loads /tools)        wasm + WASI sandbox
                                            │
                                            ▼
                                     LiteLLM ─► Ollama
```

The agent (self-identified to the model as "OlivIA") wraps each request in a
developer prompt that forces the model to answer **only** with a small JSON
envelope: `{"state":"tool", …}` to call a tool, `{"state":"done", …}` on
success, or `{"state":"error", …}` to give up. When the model asks for a tool,
the harness runs it in its sandbox and feeds the result back, looping until the
model returns `done`/`error` or `MAX_ITERATIONS` (3) is reached.

> [!NOTE]
> **Status:** early / work-in-progress. Config loading, HTTP routing,
> trigger-to-agent wiring, and the WASM tool runtime (wasmtime component model +
> WASI) are in place. Harness-side dispatch of a chosen tool
> (`ToolRegistry::run`) and the auto-generated tool catalogue in the system
> prompt are being finalized.

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
      name: "Your tool".to_string(),
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

A tool starts with **no** host access. WebAssembly gives memory isolation and no
ambient authority; host capabilities are handed over explicitly through
[WASI](https://wasi.dev/). The harness builds each tool's context with
`WasiCtxBuilder::new().build()` — i.e. **nothing is granted**: no stdio, no
environment, no filesystem preopens, no network. Every invocation also gets a
**fresh store**, so tools accumulate no state across calls.

> [!WARNING]
> If a tool needs a capability (a preopened directory, an outbound socket, a
> clock), that must be granted deliberately on the host side when building the
> tool's `WasiCtx`. It is never implicit — treat every grant as widening the
> trust boundary.

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
never block the request loop.

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
authority** and a WASI context that grants nothing unless the harness opts a
capability in (see [the sandbox](#creating-tools-wasi--the-sandbox)). Access is
something you *hand over*, not something tools *have*.

### Why this matters

Even in the worst case — the model is fully jailbroken *and* a tool has a bug —
the damage is bounded to what that specific tool was explicitly granted. The
model can't step outside "pick a tool," and the tool can't step outside its
sandbox. There is no path from "clever prompt" to "arbitrary code on the host."

## Configuration

The harness loads its configuration from `/config.toml`:

```toml
[agent]
model = "ollama/gemma4"
prompt = "You must help us with some simple tasks"

# An HTTP service on the default address/port (0.0.0.0:80)
[services.test_http]
type = "http"

[services.test_http.triggers.incoming_request]
path = "/testolo"
prompt = "You received the following message from the user. Please do something"

# A second HTTP service on a custom port
[services.second_http]
type = "http"
port = 9000

[services.second_http.triggers.hello]
method = "put"
path = "/hello"
prompt = "Generate a classic Hello Something using the content of the request"
```

**`[agent]`** — `model` (string, required): a model ID registered in the LiteLLM
`model_list`. `prompt` (string, required): the agent system prompt.

**`[services.<name>]`** — `type` (string, required): only `http` is supported.

**`http` service** — `port` (u16, default `80`), `address` (string, default
`0.0.0.0`), `triggers` (table, required).

**`http` trigger** — under `[services.<name>.triggers.<trigger>]`: `path`
(default `/`), `method` (`GET`/`POST`/`PUT`/`DELETE`, default `GET`), `prompt`
(required).

**HTTP response** — every trigger responds with a JSON envelope. `tool` is an
internal loop state; callers only ever see `done` or `error`:

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
and the `tools-builder`. Startup order is enforced via healthchecks: `ollama`
healthy before `litellm`, `litellm` healthy before `harness`, and `harness`
waits for `tools-builder` so `/tools` is populated.

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

| Variable             | Used by          | Description                                                        |
| -------------------- | ---------------- | ----------------------------------------------------------------- |
| `LITELLM_MASTER_KEY` | harness, litellm | Master key for the LiteLLM proxy.                                 |
| `LITELLM_HOST`       | harness          | Base URL of the LiteLLM proxy. Defaults to `http://litellm:4000`. |
| `RUST_LOG`           | harness          | Log filter (e.g. `info`, `debug`). Defaults to `info`.            |

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
