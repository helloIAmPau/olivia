use std::env::var;

use schemars::JsonSchema;
use schemars::schema_for;

use serde::Deserialize;
use serde::Serialize;

use serde_json::from_str;
use serde_json::to_string;

use waki::Client;

use wit_bindgen::generate;

generate!({
  world: "tool-world",
  path: "../tool.wit"
});

#[derive(Deserialize, JsonSchema)]
struct WebParams {
  /// The body of a JavaScript module that default-exports an async function.
  /// It receives a single context argument `{ page, context }`, where `page`
  /// is a Puppeteer Page, and must return `{ data, type }`, where `data` is the
  /// payload (JSON, plain text or a Buffer) and `type` is its Content-Type.
  /// Example: export default async function ({ page }) { await page.goto('https://example.com'); return { data: await page.title(), type: 'text/plain' }; }
  code: String
}

#[derive(Serialize)]
struct FunctionRequest {
  code: String
}

struct WebTool;

impl Guest for WebTool {
  fn info() -> ToolInfo {
    let schema = schema_for!(WebParams);
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
      name: "web".to_string(),
      description: "Drives a headless browser through the browserless /function API. Provide the body of a JavaScript module that default-exports an async function receiving { page, context } (page is a Puppeteer Page) and returning { data, type }. Use it to browse, scrape or screenshot pages whenever you need fresh or external information you do not already have.".to_string(),
      schema: schema_json
    };
  }

  fn run(params: String) -> ToolOutput {
    let input: WebParams = match from_str(&params) {
      Ok(input) => input,
      Err(error) => {
        return ToolOutput {
          state: ToolOutputState::Error,
          content: format!("Invalid input received: {} {}", params, error)
        };
      }
    };

    let token = match var("BROWSERLESS_TOKEN") {
      Ok(token) => token,
      Err(_) => {
        return ToolOutput {
          state: ToolOutputState::Error,
          content: "BROWSERLESS_TOKEN not set".to_string()
        }
      }
    };

    let host = match var("BROWSERLESS_HOST") {
      Ok(host) => host,
      Err(_) => "http://browserless:3000".to_string()
    };

    let endpoint = format!("{}/function", host);
    let payload = FunctionRequest {
      code: input.code
    };

    let response = match Client::new().post(endpoint.as_str()).query([("token", token)]).json(&payload).send() {
      Ok(response) => response,
      Err(error) => {
        return ToolOutput {
          state: ToolOutputState::Error,
          content: format!("Request to browserless failed: {}", error)
        };
      }
    };

    let status = response.status_code();

    let bytes = match response.body() {
      Ok(bytes) => bytes,
      Err(error) => {
        return ToolOutput {
          state: ToolOutputState::Error,
          content: format!("Could not read the browserless response: {}", error)
        };
      }
    };

    let content = String::from_utf8_lossy(&bytes).into_owned();

    if status < 200 || status >= 300 {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("browserless /function returned status {}: {}", status, content)
      };
    }

    return ToolOutput {
      state: ToolOutputState::Done,
      content
    };
  }
}

export!(WebTool);
