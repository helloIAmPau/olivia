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
        content: format!("Could not read the browserless response: {}", error)
      };
    }
  };

  return ToolOutput {
    state: ToolOutputState::Done,
    content: String::from_utf8_lossy(&bytes).into_owned()
  };
}

define_tool!(
  WebSearch,
  SearchParams,
  "Searches the internet for a query and returns the top results as title, url and description. Use it whenever you need fresh or external information you do not already know.",
  vec![Permission::Network],
  run
);
