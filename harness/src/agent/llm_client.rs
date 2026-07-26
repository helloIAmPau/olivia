use std::env::var;

use reqwest::Client;
use reqwest::header::HeaderMap;
use reqwest::header::AUTHORIZATION;
use reqwest::header::HeaderValue;

use crate::agent::AgentError;

pub struct LLMClient {
  client: Client,
  host: String
}

impl LLMClient {
  pub fn new() -> Result<Self, AgentError> {
    let MASTER_KEY = match var("LITELLM_MASTER_KEY") {
      Ok(master_key) => {
        format!("Bearer {}", master_key)
      },
      Err(error) => {
        return Err(AgentError::Var("LITELLM_MASTER_KEY", error));
      }
    };

    let host = match var("LITELLM_HOST") {
      Ok(host) => host,
      Err(_) => "http://litellm:4000".to_string()
    };

    let mut headers = HeaderMap::new();
    let authorization_value = match HeaderValue::from_str(MASTER_KEY.as_str()) {
      Ok(authorization_value) => authorization_value,
      Err(error) => {
        return Err(AgentError::InvalidHeaderValue(error));
      }
    };
    headers.insert(AUTHORIZATION, authorization_value);

    let client = match Client::builder().default_headers(headers).build() {
      Ok(client) => client,
      Err(error) => {
        return Err(AgentError::Request(error));
      }
    };

    let llm_client = Self {
      client,
      host
    };

    return Ok(llm_client);
  }
}
