use std::collections::HashMap;
use std::sync::Mutex;

use tokio::fs::read_dir;

use tracing::info;
use tracing::debug;
use tracing::warn;

use extism::Plugin;
use extism::Wasm;
use extism::Manifest;
use extism::convert::Json;

use serde_json::to_string;

use common::ToolInfo;
use common::ToolOutput;
use common::ToolOutputState;

use crate::agent::AgentError;

pub struct ToolRegistry {
  tools: HashMap<String, Mutex<Plugin>>,
  pub prompt: String
}

impl ToolRegistry {
  pub async fn new() -> Result<Self, AgentError> {
    let tools_folder = "/tools";
    info!("Loading tools from folder {}", tools_folder);
    let mut tools = HashMap::new();
    let mut prompt = "".to_string();

    let mut entries = match read_dir(tools_folder).await {
      Ok(entries) => entries,
      Err(error) => {
        return Err(AgentError::Io(error));
      }
    };

    loop {
      let entry = match entries.next_entry().await {
        Ok(Some(entry)) => entry,
        Ok(None) => break,
        Err(error) => {
          return Err(AgentError::Io(error));
        }
      };

      let tool_path = entry.path();
      let is_wasm = match tool_path.extension() {
        Some(extension) => extension == "wasm",
        None => false
      };

      if is_wasm == false {
        debug!("Skipping {}: not a valid tool", tool_path.display());

        continue;
      }

      debug!("Loading tool {}", tool_path.display());
      let wasm = Wasm::file(&tool_path);
      let manifest = Manifest::new([wasm]);
      let mut tool = match Plugin::new(&manifest, [], true) {
        Ok(tool) => tool,
        Err(error) => {
          warn!("Unable to load tool {}: {}", tool_path.display(), error);

          continue;
        }
      };

      let Json(info): Json<ToolInfo> = match tool.call("info", "") {
        Ok(info) => info,
        Err(error) => {
          warn!("Unable to load tool {}: {}", tool_path.display(), error);

          continue;
        }
      };

      let info_json = match to_string(&info) {
        Ok(info_json) => info_json,
        Err(error) => {
          warn!("Unable to load tool {}: {}", tool_path.display(), error);

          continue;
        }
      };

      prompt = format!("{}\n{}", prompt, info_json);
      tools.insert(info.name, Mutex::new(tool));
    }

    return Ok(ToolRegistry {
      tools,
      prompt
    });
  }

  pub fn run(&self, name: String, params: String) -> Result<String, AgentError> {
    let tool_mutex = match self.tools.get(&name) {
      Some(tool_mutex) => tool_mutex,
      None => {
        return Err(AgentError::InvalidToolInput(name, params, "This tool does not exist in the registry"));
      }
    };

    let mut tool = match tool_mutex.lock() {
      Ok(tool) => tool,
      Err(error) => {
        return Err(AgentError::Lock(error.to_string()));
      }
    };

    let Json(tool_output): Json<ToolOutput> = match tool.call("execute", params) {
      Ok(tool_output) => tool_output,
      Err(error) => {
        return Err(AgentError::Tool(error.to_string()));
      }
    };

    match tool_output.state {
      ToolOutputState::Done => {
        return Ok(tool_output.output);
      },
      ToolOutputState::Error => {
        return Err(AgentError::Tool(tool_output.output));
      }
    };
  }
}
