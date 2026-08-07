use std::env::var;

use schemars::JsonSchema;
use schemars::schema_for;

use serde::Deserialize;

use serde_json::from_str;
use serde_json::to_string;

use waki::Client;

use wit_bindgen::generate;

generate!({
  world: "tool-world",
  path: "../tool.wit"
});

#[derive(Deserialize, JsonSchema)]
struct SearchParams {
  /// the query to search for on the internet
  query: String
}

struct SearchTool;

impl Guest for SearchTool {
  fn info() -> ToolInfo {
    let schema = schema_for!(SearchParams);
    let schema_json = match to_string(&schema) {
      Ok(schema_json) => schema_json,
      Err(error) => {
        return ToolInfo {
          name: "Invalid tool! Do not use it".to_string(),
          description: error.to_string(),
          schema: "Invalid tool! Do not use it".to_string()
        };
      }
    };

    return ToolInfo {
      name: "web_search".to_string(),
      description: "Searches the internet for a query and returns the top results as title, url and description. Use it whenever you need fresh or external information you do not already know.".to_string(),
      schema: schema_json
    };
  }

  fn run(params: String) -> ToolOutput {
    let input: SearchParams = match from_str(&params) {
      Ok(input) => input,
      Err(error) => {
        return ToolOutput {
          state: ToolOutputState::Error,
          content: format!("Invalid input received: {} {}", params, error)
        };
      }
    };

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
}

export!(SearchTool);
