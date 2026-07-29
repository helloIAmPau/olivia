pub mod llm_client;

use std::collections::HashMap;
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

use tracing::debug;
use tracing::error;

use llm_client::LLMClient;

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
  Agent(String)
}

impl Display for AgentError {
  fn fmt(&self, formatter: &mut Formatter) -> FormatterResult {
    return match self {
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

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentPayloadState {
  Done,
  Error,
  Tool
}

#[derive(Deserialize, Serialize)]
pub struct ToolParams {
  pub name: String,
  pub argument: HashMap<String, String>
}

#[derive(Deserialize, Serialize)]
pub struct AgentPayload {
  pub state: AgentPayloadState,
  pub result: Option<String>,
  pub message: Option<String>,
  pub params: Option<ToolParams>
}

const MAX_ITERATIONS: i32 = 3;

pub struct Agent {
  client: LLMClient,
  config: AgentConfig
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

    let agent = Self {
      client,
      config
    };

    return Ok(agent);
  }

  pub async fn accept(&self, request: Vec<ChatMessage>) -> Result<AgentPayload, AgentError> {
    debug!("Agent accepted a new request");

    let context = r#"
Your name is OlivIA, an artificial intelligence built to help human in different tasks.
You are the brain of a more complex system composed by an harness, some triggers and some tools.
Eventually you will be prompted to execute some task via trigger. You must do all you can do to achieve the result.
I ask you to respond using a structured message, in JSON, shapepd like the following:
* if you finished the task succesfully return { "state": "done", "result": "your content" }
* if an error occurs return { "state": "error", "message": "the error message" }
Please be as more coherent as possible to the format, do not add any markdown, but reply only with the deserializable json.
    "#;

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
