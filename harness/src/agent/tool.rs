use std::path::PathBuf;
use std::sync::Arc;
use std::env::vars;

use tracing::info;

use wasmtime::Config;
use wasmtime::Engine;
use wasmtime::Store;

use wasmtime::component::bindgen;
use wasmtime::component::Component;
use wasmtime::component::Linker;
use wasmtime::component::ResourceTable;

use wasmtime_wasi::WasiCtx;
use wasmtime_wasi::WasiCtxView;
use wasmtime_wasi::WasiView;

use wasmtime_wasi::p2::add_to_linker_async;

use wasmtime_wasi_http::WasiHttpCtx;
use wasmtime_wasi_http::p2::add_only_http_to_linker_async;
use wasmtime_wasi_http::p2::WasiHttpView;
use wasmtime_wasi_http::p2::WasiHttpCtxView;
use wasmtime_wasi_http::p2::WasiHttpHooks;

use crate::agent::AgentError;

bindgen!({
  world: "tool-world",
  path: "../tools/tool.wit",
  exports: { default: async }
});

struct ToolHooks;

impl WasiHttpHooks for ToolHooks {}

struct ToolState {
  context: WasiCtx,
  table: ResourceTable,
  http: WasiHttpCtx,
  hooks: ToolHooks
}

impl ToolState {
  fn new() -> Self {
    let mut builder = WasiCtx::builder();
    builder.inherit_stdout();
    builder.inherit_stderr();

    for (key, value) in vars() {
      match key.strip_prefix("OLIVIA_TOOL_") {
        Some(name) => {
          info!("Loaded env variable {}={}", name, value);

          builder.env(name, value);
        }
        _ => {}
      }
    }

    let context = builder.build();

    return Self {
      context,
      http: WasiHttpCtx::new(),
      hooks: ToolHooks,
      table: ResourceTable::new()
    };
  }
}

impl WasiView for ToolState {
  fn ctx(&mut self) -> WasiCtxView<'_> {
    return WasiCtxView {
      ctx: &mut self.context,
      table: &mut self.table
    };
  }
}

impl WasiHttpView for ToolState {
  fn http(&mut self) -> WasiHttpCtxView<'_> {
    return WasiHttpCtxView {
      ctx: &mut self.http,
      table: &mut self.table,
      hooks: &mut self.hooks
    };
  }
}

pub struct ToolEngine {
  wasm: Engine,
  linker: Linker<ToolState>
}

impl ToolEngine {
  pub async fn new() -> Result<Self, AgentError> {
    let mut config = Config::new();
    config.wasm_component_model(true);

    let wasm = match Engine::new(&config) {
      Ok(wasm) => wasm,
      Err(error) => {
        return Err(AgentError::Wasm(error));
      }
    };

    let mut linker = Linker::new(&wasm);
    match add_to_linker_async(&mut linker) {
      Err(error) => {
        return Err(AgentError::Wasm(error));
      },
      _ => {}
    };
    match add_only_http_to_linker_async(&mut linker) {
      Err(error) => {
        return Err(AgentError::Wasm(error));
      },
      _ => {}
    };

    return Ok(Self {
      wasm,
      linker
    });
  }
}

pub struct Tool {
  component: Component,
  engine: Arc<ToolEngine>
}

impl Tool {
  pub fn new(path: PathBuf, engine: Arc<ToolEngine>) -> Result<Self, AgentError> {
    let component = match Component::from_file(&engine.wasm, path) {
      Ok(component) => component,
      Err(error) => {
        return Err(AgentError::Wasm(error));
      }
    };

    return Ok(Self {
      component,
      engine
    });
  }

  pub async fn info(&self) -> Result<ToolInfo, AgentError> {
    let state = ToolState::new();
    let mut store = Store::new(&self.engine.wasm, state);

    let bindings = match ToolWorld::instantiate_async(&mut store, &self.component, &self.engine.linker).await {
      Ok(bindings) => bindings,
      Err(error) => {
        return Err(AgentError::Wasm(error));
      }
    };

    let info = match bindings.call_info(&mut store).await {
      Ok(info) => info,
      Err(error) => {
        return Err(AgentError::Wasm(error));
      }
    };

    return Ok(info);
  }

  pub async fn run(&self, params: String) -> Result<ToolOutput, AgentError> {
    let state = ToolState::new();
    let mut store = Store::new(&self.engine.wasm, state);

    let bindings = match ToolWorld::instantiate_async(&mut store, &self.component, &self.engine.linker).await {
      Ok(bindings) => bindings,
      Err(error) => {
        return Err(AgentError::Wasm(error));
      }
    };

    let result = match bindings.call_run(&mut store, &params).await {
      Ok(result) => result,
      Err(error) => {
        return Err(AgentError::Wasm(error));
      }
    };

    return Ok(result);
  }
}
