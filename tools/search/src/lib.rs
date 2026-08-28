use std::env::var;

use schemars::JsonSchema;

use serde::Deserialize;

use waki::Client;

use common::define_tool;

#[derive(Deserialize, JsonSchema)]
struct SearchParams {
  /// the query to search for on the internet
  query: String
}

#[derive(Deserialize)]
struct SearchResult {
  #[serde(default)]
  title: String,
  #[serde(default)]
  url: String,
  #[serde(default)]
  content: String
}

#[derive(Deserialize)]
struct SearchResponse {
  #[serde(default)]
  results: Vec<SearchResult>
}

fn run(input: SearchParams) -> ToolOutput {
  let base = match var("SEARXNG_HOST") {
    Ok(base) => base,
    Err(_) => "http://searxng:8080".to_string()
  };

  let endpoint = format!("{}/search", base);
  let response = match Client::new().get(endpoint.as_str()).query([
    ("q", input.query),
    ("format", "json".to_string()),
    ("limit", "5".to_string())
  ]).send() {
    Ok(response) => response,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Request to browserless failed: {}", error)
      };
    }
  };

  let bytes = match response.body() {
    Ok(bytes) => bytes,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Could not read the search response: {}", error)
      };
    }
  };

  let parsed: SearchResponse = match serde_json::from_slice(&bytes) {
    Ok(parsed) => parsed,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Could not parse the search response: {}", error)
      };
    }
  };

  let mut content = String::new();
  for result in parsed.results.iter().take(5) {
    content.push_str(&format!("{}\n{}\n{}\n\n", result.title, result.url, result.content));
  }

  return ToolOutput {
    state: ToolOutputState::Done,
    content: content
  };
}

define_tool!(
  WebSearch,
  SearchParams,
  "Searches the internet for a query and returns the top results as title, url and description. Use it whenever you need fresh or external information you do not already know.",
  vec![Permission::Network],
  run
);
