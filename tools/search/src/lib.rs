use std::env::var;

use schemars::JsonSchema;

use serde::Deserialize;

use common::define_tool;
use common::http::HttpClient;

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

  let client = HttpClient::new(base.as_str());
  let query: Vec<(&str, &str)> = vec![
    ("q", input.query.as_str()),
    ("format", "json"),
    ("limit", "5")
  ];

  let response = match client.get("/search", vec![], query) {
    Ok(response) => response,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Request to searxng failed: {}", error)
      };
    }
  };

  if response.is_success() == false {
    return ToolOutput {
      state: ToolOutputState::Error,
      content: format!("searxng returned status {}: {}", response.status, response.text())
    };
  }

  return ToolOutput {
    state: ToolOutputState::Done,
    content: response.text()
  };
}

define_tool!(
  WebSearch,
  SearchParams,
  "Searches the internet for a query and returns the top results as title, url and description. Use it whenever you need fresh or external information you do not already know.",
  vec![Permission::Network],
  run
);
