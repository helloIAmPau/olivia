use std::collections::HashMap;
use std::sync::Arc;

use tokio::fs::read_dir;

use tracing::info;
use tracing::debug;

use crate::agent::tool::ToolEngine;
use crate::agent::tool::Tool;
 use crate::agent::tool::ToolOutputState;
use crate::agent::AgentError;

pub struct ToolRegistry {
  tools: HashMap<String, Tool>,
  pub prompt: String
}

impl ToolRegistry {
  pub async fn new(tools_folder: &str) -> Result<Self, AgentError> {
    info!("Loading tools from folder {}", tools_folder);
    let mut tools = HashMap::new();
    let mut prompt = "".to_string();
    let engine = match ToolEngine::new().await {
      Ok(engine) => engine,
      Err(error) => {
        return Err(error);
      }
    };

    let mut entries = match read_dir(tools_folder).await {
      Ok(entries) => entries,
      Err(error) => {
        return Err(AgentError::Io(error));
      }
    };

    let engine_arc = Arc::new(engine);
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

      info!("Loading tool {}", tool_path.display());
      let tool = match Tool::new(tool_path, engine_arc.clone()) {
        Ok(tool) => tool,
        Err(error) => {
          return Err(error);
        }
      };

      let info = match tool.info().await {
        Ok(info) => info,
        Err(error) => {
          return Err(error);
        }
      };

      info!("{:#?}", info);
      prompt = format!("{}\nname: {}\ndescription: {}\ninput schema: {}\n", prompt, info.name, info.description, info.schema);
      tools.insert(info.name, tool);
    }

    return Ok(ToolRegistry {
      tools,
      prompt
    });
  }

  pub async fn run(&self, name: String, params: String) -> Result<String, AgentError> {
    info!("Received request to run tool {} with params: {}", name, params);

    let tool = match self.tools.get(&name) {
      Some(tool) => tool,
      None => {
        return Err(AgentError::InvalidToolInput(name, params, "This tool does not exist in the registry"));
      }
    };

    let output = match tool.run(params).await {
      Ok(output) => output,
      Err(error) => {
        return Err(AgentError::Tool(error.to_string()));
      }
    };

    match output.state {
      ToolOutputState::Done => {
        return Ok(output.content);
      },
      ToolOutputState::Error => {
        return Err(AgentError::Tool(output.content));
      }
    };
  }
}
