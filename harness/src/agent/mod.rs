pub mod llm_client;

use std::fmt::Result as FormatterResult;
use std::fmt::Formatter;
use std::fmt::Display;
use std::env::VarError;

use serde::Deserialize;
use reqwest::header::InvalidHeaderValue;
use reqwest::Error as ReqwestError;

use llm_client::LLMClient;

#[derive(Debug)]
pub enum AgentError {
  Var(&'static str, VarError),
  InvalidHeaderValue(InvalidHeaderValue),
  Request(ReqwestError)
}

impl Display for AgentError {
  fn fmt(&self, formatter: &mut Formatter) -> FormatterResult {
    return match self {
      AgentError::Var(name, error) => write!(formatter, "Var Error - {} {}", name, error),
      AgentError::InvalidHeaderValue(error) => write!(formatter, "Invalid Header Value Error - {}", error),
      AgentError::Request(error) => write!(formatter, "Request Error - {}", error)
    }
  }
}

#[derive(Deserialize)]
pub struct AgentConfig {
  pub prompt: String
}

pub struct Agent {
  client: LLMClient
}

impl Agent {
  pub fn new(config: AgentConfig) -> Result<Self, AgentError> {
    let client = match LLMClient::new() {
      Ok(client) => client,
      Err(error) => {
        return Err(error);
      }
    };

    let agent = Self {
      client
    };

    return Ok(agent);
  }
}
