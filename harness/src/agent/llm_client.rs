use std::env::var;

use serde_json::from_str;

use reqwest::Client;
use reqwest::header::HeaderMap;
use reqwest::header::AUTHORIZATION;
use reqwest::header::HeaderValue;

use tracing::debug;

use serde::Serialize;
use serde::Deserialize;

use crate::agent::AgentError;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ChatMessageRole {
  User,
  Developer,
  System,
  Assistant
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatMessage {
  pub role: ChatMessageRole,
  pub content: String
}

#[derive(Serialize, Debug)]
pub struct ChatRequest {
  pub model: String,
  pub messages: Vec<ChatMessage>
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ChatChoice {
  pub message: ChatMessage
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ChatResponse {
  pub choices: Vec<ChatChoice>
}

#[derive(Deserialize)]
pub struct Model {
  pub id: String
}

#[derive(Deserialize)]
pub struct ModelListResponse {
  pub data: Vec<Model>
}

pub struct LLMClient {
  client: Client,
  host: String
}

impl LLMClient {
  pub fn new() -> Result<Self, AgentError> {
    let master_key = match var("LITELLM_MASTER_KEY") {
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
    let authorization_value = match HeaderValue::from_str(master_key.as_str()) {
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

  pub async fn models(&self) -> Result<Vec<Model>, AgentError> {
    let url = format!("{}/v1/models", self.host);

    let response = match self.client.get(&url).send().await {
      Ok(response) => response,
      Err(error) => {
        return Err(AgentError::Request(error));
      }
    };

    let result: ModelListResponse = match response.json().await {
      Ok(result) => result,
      Err(error) => {
        return Err(AgentError::Request(error));
      }
    };

    return Ok(result.data);
  }

  pub async fn completions(&self, model: String, messages: &Vec<ChatMessage>) -> Result<ChatMessage, AgentError> {
    let url = format!("{}/v1/chat/completions", self.host);

    let body = ChatRequest {
      model,
      messages: messages.to_vec()
    };

    debug!("Sending request\n{:#?}", body);

    let response = match self.client.post(&url).json(&body).send().await {
      Ok(response) => response,
      Err(error) => {
        return Err(AgentError::Request(error));
      }
    };

    debug!("Received response\n{:#?}", response);

    let content = match response.text().await {
      Ok(content) => content,
      Err(error) => {
        return Err(AgentError::Request(error));
      }
    };

    debug!("Response body\n{}", content);

    let mut result: ChatResponse = match from_str(&content) {
      Ok(result) => result,
      Err(error) => {
        return Err(AgentError::Parsing(error));
      }
    };

    if result.choices.is_empty() {
      return Err(AgentError::Completions(body, result));
    }
    let choice = result.choices.remove(0);

    return Ok(choice.message);
  }
}
