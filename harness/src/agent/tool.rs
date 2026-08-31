use std::path::PathBuf;
use std::sync::Arc;
use std::env::var;
use std::fs::create_dir_all;

use uuid::Uuid;

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
use wasmtime_wasi::DirPerms;
use wasmtime_wasi::FilePerms;

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

pub const GUEST_SANDBOX_PATH: &str = "/sandbox";

struct ToolHooks;

impl WasiHttpHooks for ToolHooks {}

struct ToolState {
  context: WasiCtx,
  table: ResourceTable,
  http: WasiHttpCtx,
  hooks: ToolHooks
}

impl ToolState {
  fn new(permissions: Vec<Permission>, env: Vec<String>, sandbox_path: String) -> Result<Self, AgentError> {
    let mut builder = WasiCtx::builder();
    builder.inherit_stdout();
    builder.inherit_stderr();

    for permission in &permissions {
      match permission {
        Permission::Network => {
          builder.inherit_network();
          builder.allow_ip_name_lookup(true);
        }
        Permission::FileSystem => {
          match create_dir_all(&sandbox_path) {
            Ok(_) => {},
            Err(error) => {
              let message = format!("Unable to create the session sandbox directory {}: {}", sandbox_path, error);
              return Err(AgentError::Tool(message));
            }
          };

          match builder.preopened_dir(&sandbox_path, GUEST_SANDBOX_PATH, DirPerms::all(), FilePerms::all()) {
            Ok(_) => {},
            Err(error) => {
              let message = format!("Unable to preopen the sandbox directory {}: {}", sandbox_path, error);
              return Err(AgentError::Tool(message));
            }
          };
        }
      }
    }

    for key in env {
      match var(&key) {
        Ok(value) => {
          info!("Passing env variable {}={}", key, value);

          builder.env(key, value);
        },
        _ => {}
      }
    }

    let context = builder.build();

    return Ok(Self {
      context,
      http: WasiHttpCtx::new(),
      hooks: ToolHooks,
      table: ResourceTable::new()
    });
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
  linker: Linker<ToolState>,
  sandbox: String
}

impl ToolEngine {
  pub async fn new(sandbox: &str) -> Result<Self, AgentError> {
    match rustls::crypto::ring::default_provider().install_default() {
      Ok(_) => {},
      Err(_) => {}
    };

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
      linker,
      sandbox: sandbox.to_string()
    });
  }
}

pub struct Tool {
  component: Component,
  engine: Arc<ToolEngine>,
  pub info: ToolInfo
}

impl Tool {
  pub async fn new(path: PathBuf, engine: Arc<ToolEngine>) -> Result<Self, AgentError> {
    let component = match Component::from_file(&engine.wasm, path) {
      Ok(component) => component,
      Err(error) => {
        return Err(AgentError::Wasm(error));
      }
    };

    let info = match Tool::bindings(vec![], vec![], &Uuid::nil(), engine.clone(), &component, async |bindings, store| {
      return bindings.call_info(store).await;
    }).await {
      Ok(info) => info,
      Err(error) => {
        return Err(error);
      }
    };

    return Ok(Self {
      component,
      engine,
      info
    });
  }

  pub async fn run(&self, params: String, session_id: &Uuid) -> Result<ToolOutput, AgentError> {
    let result = match Tool::bindings(self.info.permissions.clone(), self.info.env.clone(), session_id, self.engine.clone(), &self.component, async |bindings, store| {
      return bindings.call_run(store, &params).await;
    }).await {
      Ok(result) => result,
      Err(error) => {
        return Err(error);
      }
    };

    return Ok(result);
  }

  async fn bindings<T>(permissions: Vec<Permission>, env: Vec<String>, session_id: &Uuid, engine: Arc<ToolEngine>, component: &Component, call: impl AsyncFnOnce(ToolWorld, &mut Store<ToolState>) -> wasmtime::Result<T>) -> Result<T, AgentError> {
    let sandbox_path = format!("{}/{}", engine.sandbox, session_id);
    
    let state = match ToolState::new(permissions, env, sandbox_path) {
      Ok(state) => state,
      Err(error) => {
        return Err(error);
      }
    };
    let mut store = Store::new(&engine.wasm, state);

    let bindings = match ToolWorld::instantiate_async(&mut store, component, &engine.linker).await {
      Ok(bindings) => bindings,
      Err(error) => {
        return Err(AgentError::Wasm(error));
      }
    };

    match call(bindings, &mut store).await {
      Ok(value) => Ok(value),
      Err(error) => Err(AgentError::Wasm(error))
    }
  }
}
