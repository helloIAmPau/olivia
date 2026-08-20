use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FormatterResult;

use serde::Serialize;
use serde::Deserialize;

use serde_json::from_slice;
use serde_json::Value;

use crate::http::HttpClient;
use crate::http::HttpError;

// A reusable client for a LiteLLM proxy (which fronts Ollama, Anthropic and
// OpenAI), built on the shared HTTP client. It exposes the two endpoints a RAG
// pipeline needs: embeddings and chat completions.

#[derive(Debug)]
pub enum LitellmError {
  Http(HttpError),
  Server(u16, String),
  Parse(String),
  Empty
}

impl Display for LitellmError {
  fn fmt(&self, formatter: &mut Formatter) -> FormatterResult {
    return match self {
      LitellmError::Http(error) => write!(formatter, "{}", error),
      LitellmError::Server(status, body) => write!(formatter, "LiteLLM returned status {}: {}", status, body),
      LitellmError::Parse(error) => write!(formatter, "Could not parse the LiteLLM response: {}", error),
      LitellmError::Empty => write!(formatter, "LiteLLM returned no result")
    };
  }
}

/// A single chat message. `content` is a JSON value so it can be either a plain
/// string (`json!("...")`) or an array of content blocks (text, image, file)
/// for multimodal requests such as document extraction.
#[derive(Serialize)]
pub struct ChatMessage {
  pub role: String,
  pub content: Value
}

#[derive(Serialize)]
struct EmbeddingRequest {
  model: String,
  input: Vec<String>
}

#[derive(Deserialize)]
struct EmbeddingDatum {
  index: usize,
  embedding: Vec<f64>
}

#[derive(Deserialize)]
struct EmbeddingResponse {
  data: Vec<EmbeddingDatum>
}

#[derive(Serialize)]
struct ChatRequest {
  model: String,
  messages: Vec<ChatMessage>
}

#[derive(Deserialize)]
struct ChatResponseMessage {
  content: String
}

#[derive(Deserialize)]
struct ChatChoice {
  message: ChatResponseMessage
}

#[derive(Deserialize)]
struct ChatResponse {
  choices: Vec<ChatChoice>
}

pub struct Litellm {
  host: String,
  api_key: String
}

impl Litellm {
  pub fn new(host: &str, api_key: &str) -> Self {
    return Self {
      host: host.to_string(),
      api_key: api_key.to_string()
    };
  }

  /// Embeds one or more texts and returns a vector per input, in input order.
  pub fn embed(&self, model: &str, inputs: Vec<String>) -> Result<Vec<Vec<f64>>, LitellmError> {
    let count = inputs.len();
    let client = HttpClient::new(self.host.as_str());
    let authorization = format!("Bearer {}", self.api_key);
    let headers: Vec<(&str, &str)> = vec![("Authorization", authorization.as_str())];

    let request = EmbeddingRequest {
      model: model.to_string(),
      input: inputs
    };

    let response = match client.post("/v1/embeddings", headers, vec![], Some(&request)) {
      Ok(response) => response,
      Err(error) => {
        return Err(LitellmError::Http(error));
      }
    };

    if response.is_success() == false {
      return Err(LitellmError::Server(response.status, response.text()));
    }

    let mut parsed: EmbeddingResponse = match from_slice(response.bytes()) {
      Ok(parsed) => parsed,
      Err(error) => {
        return Err(LitellmError::Parse(error.to_string()));
      }
    };

    if parsed.data.len() != count {
      return Err(LitellmError::Parse(format!("expected {} embeddings but received {}", count, parsed.data.len())));
    }

    // The API does not guarantee ordering, so realign on the returned index.
    parsed.data.sort_by(|left, right| left.index.cmp(&right.index));

    let vectors: Vec<Vec<f64>> = parsed.data.into_iter().map(|datum| datum.embedding).collect();

    return Ok(vectors);
  }

  /// Runs a chat completion and returns the assistant's message content.
  pub fn completions(&self, model: &str, messages: Vec<ChatMessage>) -> Result<String, LitellmError> {
    let client = HttpClient::new(self.host.as_str());
    let authorization = format!("Bearer {}", self.api_key);
    let headers: Vec<(&str, &str)> = vec![("Authorization", authorization.as_str())];

    let request = ChatRequest {
      model: model.to_string(),
      messages
    };

    let response = match client.post("/v1/chat/completions", headers, vec![], Some(&request)) {
      Ok(response) => response,
      Err(error) => {
        return Err(LitellmError::Http(error));
      }
    };

    if response.is_success() == false {
      return Err(LitellmError::Server(response.status, response.text()));
    }

    let mut parsed: ChatResponse = match from_slice(response.bytes()) {
      Ok(parsed) => parsed,
      Err(error) => {
        return Err(LitellmError::Parse(error.to_string()));
      }
    };

    if parsed.choices.is_empty() {
      return Err(LitellmError::Empty);
    }

    let choice = parsed.choices.remove(0);

    return Ok(choice.message.content);
  }
}
