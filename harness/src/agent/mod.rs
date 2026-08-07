pub mod llm_client;
pub mod tool_registry;
pub mod tool;

use std::collections::HashMap;
use std::io::Error as IoError;
use std::fmt::Result as FormatterResult;
use std::fmt::Formatter;
use std::fmt::Display;
use std::env::VarError;

use serde::Deserialize;
use serde::Serialize;
use serde_json::Error as ParsingError;
use serde_json::from_str;

use reqwest::header::InvalidHeaderValue;
use reqwest::Error as ReqwestError;

use wasmtime::Error as WasmError;

use tracing::debug;
use tracing::error;

use schemars::JsonSchema;

use llm_client::LLMClient;
use tool_registry::ToolRegistry;

use crate::agent::llm_client::ChatMessage;
use crate::agent::llm_client::ChatMessageRole;
use crate::agent::llm_client::ChatRequest;
use crate::agent::llm_client::ChatResponse;

#[derive(Debug)]
pub enum AgentError {
  Schema,
  Var(&'static str, VarError),
  InvalidHeaderValue(InvalidHeaderValue),
  Request(ReqwestError),
  Model(&'static str, String),
  Parsing(ParsingError),
  Completions(ChatRequest, ChatResponse),
  MaxIterations,
  Agent(String),
  Io(IoError),
  InvalidToolInput(String, String, &'static str),
  Tool(String),
  Lock(String),
  Wasm(WasmError)
}

impl Display for AgentError {
  fn fmt(&self, formatter: &mut Formatter) -> FormatterResult {
    return match self {
      AgentError::Schema => write!(formatter, " Schema Error - Invalid schema for agent request"),
      AgentError::Io(error) => write!(formatter, "Io Error - {}", error),
      AgentError::Var(name, error) => write!(formatter, "Var Error - {} {}", name, error),
      AgentError::InvalidHeaderValue(error) => write!(formatter, "Invalid Header Value Error - {}", error),
      AgentError::Request(error) => write!(formatter, "Request Error - {}", error),
      AgentError::Parsing(error) => write!(formatter, "Parsing Error - {}", error),
      AgentError::Model(message, model) => write!(formatter, "[{}] Model Error - {}", model, message),
      AgentError::Completions(request, response) => write!(formatter, "Invalid response from model\nrequest:\n{:#?}\nresponse:\n{:#?}", request, response),
      AgentError::MaxIterations => write!(formatter, "Max agentic loop iterations reached. Aborted trigger"),
      AgentError::Agent(message) => write!(formatter, "Agent Error - {}", message),
      AgentError::Tool(error) => write!(formatter, "Tool Error - {}", error),
      AgentError::Lock(error) => write!(formatter, "Lock Error - {}", error),
      AgentError::Wasm(error) => write!(formatter, "Wasm Error - {}", error),
      AgentError::InvalidToolInput(name, params, message) => write!(formatter, "Invalid Tool Input Error - {} {}({})", message, name, params)
    }
  }
}

#[derive(Deserialize)]
pub struct PostgresConfig {
  pub connection_string: String,
  pub prompt: String
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AgentStoreConfig {
  Postgres(PostgresConfig)
}

fn default_stores() -> HashMap<String, AgentStoreConfig> {
  return HashMap::new();
}

#[derive(Deserialize)]
pub struct AgentConfig {
  pub prompt: String,
  pub model: String,
  #[serde(default = "default_stores")]
  pub stores: HashMap<String, AgentStoreConfig>
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AgentPayloadState {
  Done,
  Error,
  Tool
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct AgentPayload {
  /// the execution result
  pub state: AgentPayloadState,
  /// your output if the execution succeded. Required for state = Done, null otherwise
  pub result: Option<String>,
  /// your error message, if any, if the execution failed. Required for state = Error, null otherwise
  pub message: Option<String>,
  /// the name of the tool to use. Required for state = Tool, null otherwise
  pub name: Option<String>,
  /// a json string rapresenting the tool parameters. Look at the tool section to learn how to set the parameters for each tool. Required for state = Tool, null otherwise.
  pub params: Option<String>
}

const MAX_ITERATIONS: i32 = 200;

pub struct Agent {
  client: LLMClient,
  config: AgentConfig,
  registry: ToolRegistry,
  store_prompt: String
}

impl Agent {
  pub async fn new(config: AgentConfig) -> Result<Self, AgentError> {
    let client = match LLMClient::new() {
      Ok(client) => client,
      Err(error) => {
        return Err(error);
      }
    };

    debug!("Checking model {} exists on proxy", config.model);
    let models = match client.models().await {
      Ok(models) => models,
      Err(error) => {
        return Err(error);
      }
    };

    let has_model_id = models.iter().any(|model| model.id == config.model);
    if has_model_id == false {
      return Err(AgentError::Model("Model not found on proxy. Please check your configuration", config.model));
    }

    debug!("Model {} found", config.model);

    let registry = match ToolRegistry::new().await {
      Ok(registry) => registry,
      Err(error) => {
        return Err(error);
      }
    };

    let mut store_prompt = "".to_string();
    for (name, config) in &config.stores {
      match config {
        AgentStoreConfig::Postgres(postgres_config) => {
          store_prompt = format!("{}\n{} - {}\ntype: postgres\nconnection string: {}\n", store_prompt, name, postgres_config.prompt, postgres_config.connection_string);
        }
      }
    }

    let agent = Self {
      client,
      config,
      registry,
      store_prompt
    };

    return Ok(agent);
  }

  pub async fn accept(&self, request: Vec<ChatMessage>) -> Result<AgentPayload, AgentError> {
    debug!("Agent accepted a new request");

    let context = format!(r#"
You are OlivIA, a strict AI task coordinator. Your SOLE function is to analyze requests and delegate them to external tools. 

CRITICAL BEHAVIORAL RULES:
1. MULTI-STEP COORDINATION: You are capable of chaining multiple tools to complete complex tasks. If a task requires multiple steps (e.g., making an HTTP request to obtain a resource, then passing that data to a Python script), execute them sequentially. Call one tool, wait for the environment's response, and then evaluate your next step.
2. DELEGATE EXECUTION: Do NOT calculate, process heavy logic, or attempt to fulfill execution steps using your internal knowledge. You must use the provided tools to execute ANY action, retrieve ANY information, or process ANY logic. You are a router and coordinator.
3. STRICT JSON ONLY: You must respond ONLY with raw, deserializable JSON. Do NOT include markdown formatting, code blocks (e.g., ```json), or any conversational text before or after the JSON object.
4. CONVERSATIONAL JSON: You possess conversational capabilities, but all dialogue, explanations, updates, and final answers MUST be passed strictly as a string value within the "message" field of your JSON output.
5. DATA STORE UTILIZATION: You have access to specific data environments listed under AVAILABLE DATA STORES. You cannot connect to them directly. When a task requires retrieving or storing data, identify the appropriate environment based on its "description". You must pass the exact "connection_string" and "type" as parameters to the relevant tool to execute the operation.

EXAMPLES OF EXPECTED OUTPUT (RAW JSON ONLY):
* Tool usage
{{ "state": "tool", "name": "Web search tool", "params": "{{"query": "best website about cats"}}", "message": null, "result": null }}
* Error
{{ "state": "error", "message": "I cannot find any tool to execute the task", "name": null, "params": null, "result": null }}
* Success 
{{ "state": "done", "result": "look at this website https://www.cats.com", "message": null, "name": null, "params": null }}

AVAILABLE TOOLS:
{}

AVAILABLE DATA STORES:
{}
    "#, self.registry.prompt, self.store_prompt);

    let mut iteration = 0;
    let mut payload = vec![
      ChatMessage {
        role: ChatMessageRole::System,
        content: context.to_string()
      }
    ];
    payload.extend_from_slice(&request);

    loop {
      if iteration >= MAX_ITERATIONS {
        return Err(AgentError::MaxIterations);
      }

      iteration = iteration + 1;
      debug!("Agentic iteration {}/{}", iteration, MAX_ITERATIONS);

      let assistant_chat_message = match self.client.completions(self.config.model.to_string(), &payload).await {
        Ok(assistant_chat_message) => assistant_chat_message,
        Err(error) => {
          error!("Error in iteration {}/{}\n{}", iteration, MAX_ITERATIONS, error);

          continue;
        }
      };

      payload.push(assistant_chat_message.clone());

      let agent_payload: AgentPayload = match from_str(&assistant_chat_message.content) {
        Ok(agent_payload) => agent_payload,
        Err(error) => {
          let feedback = format!(r#"
SYSTEM ERROR: JSON DESERIALIZATION FAILED.

The system attempted to parse your last response but failed.
Decoder error: {}

<your_invalid_response>
{}
</your_invalid_response>

CRITICAL CORRECTION INSTRUCTIONS:
1. Identify the structural error pointed out by the Decoder error.
2. Strip ALL markdown formatting (do NOT use ```json fences).
3. Remove ALL conversational text before or after the JSON.
4. Ensure strictly valid JSON syntax (no trailing commas, proper quotes).

Rewrite your response immediately as a single, raw, valid JSON object.

EXAMPLES OF EXPECTED OUTPUT (RAW JSON ONLY):
* Tool usage
{{ "state": "tool", "name": "Web search tool", "params": "{{"query": "best website about cats"}}" }}
* Error
{{ "state": "error", "message": "I cannot find any tool to execute the task" }}
* Success 
{{ "state": "done", "result": "look at this website https://www.cats.com" }}
          "#, error, assistant_chat_message.content);

          payload.push(ChatMessage {
            role: ChatMessageRole::System,
            content: feedback
          });

          continue;
        }
      };

      match agent_payload.state {
        AgentPayloadState::Done => {
          return Ok(agent_payload);
        },
        AgentPayloadState::Error => {
          let message = match agent_payload.message {
            Some(message) => message,
            None => "No error message".to_string()
          };

          return Err(AgentError::Agent(message));
        },
        AgentPayloadState::Tool => {
          let params = match agent_payload.params {
            Some(params) => params,
            None => "".to_string()
          };

          let name = match agent_payload.name {
            Some(name) => name,
            None => "".to_string()
          };

          let tool_output = match self.registry.run(name, params).await {
            Ok(tool_output) => {
              format!(r#"
TOOL EXECUTED

<your_request>
{}
</your_request>

<result>
{}
</result>
              "#, assistant_chat_message.content, tool_output)
            },
            Err(AgentError::InvalidToolInput(bad_name, bad_params, message)) => {
              format!(r#"
SYSTEM ERROR: INVALID TOOL EXECUTION REQUEST.

Your JSON was structurally valid, but the tool request failed semantic validation. 
Error Details: You attempted to call a tool named '{}' with parameters '{}'. {}.

<your_invalid_request>
{}
</your_invalid_request>

CRITICAL CORRECTION INSTRUCTIONS:
1. Tool Name: You may ONLY use the exact tool names provided in the system registry. Do not invent tools.
2. Review the available tools below and correct your request.

AVAILABLE TOOLS:
{}

Rewrite your response immediately as a single, raw, valid JSON object calling a valid tool.
              "#, bad_name, bad_params, message, assistant_chat_message.content, self.registry.prompt)
            },
            Err(error) => {
              format!(r#"
TOOL EXECUTION FAILED

The system attempted to run the tool, but it encountered an internal error.

<your_request>
{}
</your_request>

<error_result>
{}
</error_result>

INSTRUCTIONS:
The tool failed. Do not blindly repeat the exact same request. 
You must decide the next best action:
1. Retry the tool with different parameters.
2. Use a different fallback tool.
3. If no other options exist, change your state to "error" and inform the user.

Respond immediately with your next JSON action.
              "#, assistant_chat_message.content, error)
            }
          };

          payload.push(ChatMessage {
            role: ChatMessageRole::User,
            content: tool_output
          });
        }
      }
    }
  }
}
