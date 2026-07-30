pub mod llm_client;
pub mod tool_registry;

use std::io::Error as IoError;
use std::fmt::Result as FormatterResult;
use std::fmt::Formatter;
use std::fmt::Display;
use std::env::VarError;

use serde::Deserialize;
use serde::Serialize;
use serde_json::Error as ParsingError;
use serde_json::from_str;
use serde_json::to_string;
use reqwest::header::InvalidHeaderValue;
use reqwest::Error as ReqwestError;

use tracing::debug;
use tracing::error;

use schemars::JsonSchema;
use schemars::schema_for;

use llm_client::LLMClient;
use tool_registry::ToolRegistry;

use crate::agent::llm_client::ChatMessage;
use crate::agent::llm_client::ChatMessageRole;
use crate::agent::llm_client::ChatRequest;
use crate::agent::llm_client::ChatResponse;

#[derive(Debug)]
pub enum AgentError {
  Var(&'static str, VarError),
  InvalidHeaderValue(InvalidHeaderValue),
  Request(ReqwestError),
  Model(&'static str, String),
  Parsing(ParsingError),
  Completions(ChatRequest, ChatResponse),
  MaxIterations,
  Agent(String),
  Io(IoError)
}

impl Display for AgentError {
  fn fmt(&self, formatter: &mut Formatter) -> FormatterResult {
    return match self {
      AgentError::Io(error) => write!(formatter, "Io Error - {}", error),
      AgentError::Var(name, error) => write!(formatter, "Var Error - {} {}", name, error),
      AgentError::InvalidHeaderValue(error) => write!(formatter, "Invalid Header Value Error - {}", error),
      AgentError::Request(error) => write!(formatter, "Request Error - {}", error),
      AgentError::Parsing(error) => write!(formatter, "Parsing Error - {}", error),
      AgentError::Model(message, model) => write!(formatter, "[{}] Model Error - {}", model, message),
      AgentError::Completions(request, response) => write!(formatter, "Invalid response from model\nrequest:\n{:#?}\nresponse:\n{:#?}", request, response),
      AgentError::MaxIterations => write!(formatter, "Max agentic loop iterations reached. Aborted trigger"),
      AgentError::Agent(message) => write!(formatter, "Agent Error - {}", message)
    }
  }
}

#[derive(Deserialize)]
pub struct AgentConfig {
  pub prompt: String,
  pub model: String
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
  /// your output, if any, if the execution succeded 
  pub result: Option<String>,
  /// your error message, if any, if the execution failed
  pub message: Option<String>,
  /// the name of the tool to use. Only set for state = tool
  pub name: Option<String>,
  /// a json string rapresenting the tool parameters. Only set for state = tool
  pub params: Option<String>
}

const MAX_ITERATIONS: i32 = 3;

pub struct Agent {
  client: LLMClient,
  config: AgentConfig,
  registry: ToolRegistry
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

    let agent = Self {
      client,
      config,
      registry
    };

    return Ok(agent);
  }

  pub async fn accept(&self, request: Vec<ChatMessage>) -> Result<AgentPayload, AgentError> {
    debug!("Agent accepted a new request");

    let output_schema = schema_for!(AgentPayload);
    let output_schema_json = match to_string(&output_schema) {
      Ok(output_schema_json) => output_schema_json,
      Err(error) => {
        return Err(AgentError::Parsing(error));
      }
    };

    let context = format!(r#"
You are OlivIA, a strict AI task coordinator. Your SOLE function is to analyze requests and delegate them to external tools. 

CRITICAL BEHAVIORAL RULES:
1. ZERO INTERNAL LOGIC: Do NOT calculate, summarize, or attempt to fulfill the task using your internal knowledge. 
2. ALWAYS DELEGATE: You must use the provided tools to execute ANY action, retrieve ANY information, or process ANY logic. You are a router, not a solver.
3. STRICT JSON ONLY: You must respond ONLY with raw, deserializable JSON. Do NOT include markdown formatting, code blocks (e.g., ```json), or any conversational text before or after the JSON object.

OUTPUT SCHEMA:
Your response must strictly adhere to the following JSON schema:
{}

TOOL USAGE:
Because you do not execute logic yourself, your default response state should be calling a tool.
To execute a tool, your JSON output must reflect the following:
- "state": "tool"
- "name": "<exact_tool_name>"
- "params": <JSON_object_of_parameters>

AVAILABLE TOOLS:
{}
    "#, output_schema_json, self.registry.prompt);

    let mut iteration = 0;
    let mut payload = vec![
      ChatMessage {
        role: ChatMessageRole::Developer,
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

      let llm_result = match self.client.completions(self.config.model.to_string(), &payload).await {
        Ok(llm_result) => llm_result,
        Err(error) => {
          error!("Error in iteration {}/{}\n{}", iteration, MAX_ITERATIONS, error);

          continue;
        }
      };

      let result: AgentPayload = match from_str(&llm_result.content) {
        Ok(result) => result,
        Err(error) => {
          let feedback = format!(r#"
Error: Your response was not valid JSON or did not conform to the schema.
Decoder error: {}
Your output was:\n{}
Please respond strictly with valid JSON conforming to the requested schema.
          "#, error, llm_result.content);

          payload.push(ChatMessage {
            role: ChatMessageRole::Developer,
            content: feedback
          });

          continue;
        }
      };

      match result.state {
        AgentPayloadState::Done => {
          return Ok(result);
        },
        AgentPayloadState::Error => {
          let message = match result.message {
            Some(message) => message,
            None => "No error message".to_string()
          };

          return Err(AgentError::Agent(message));
        },
        AgentPayloadState::Tool => {
          debug!("TODO");
        }
      }
    }
  }
}
