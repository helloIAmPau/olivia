use std::env::var;

use schemars::JsonSchema;

use serde::Deserialize;
use serde::Serialize;

use common::define_tool;
use common::http::HttpClient;

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

fn run(input: WebParams) -> ToolOutput {
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

  let payload = FunctionRequest {
    code: input.code
  };

  let client = HttpClient::new(host.as_str());
  let query: Vec<(&str, &str)> = vec![
    ("token", token.as_str()),
    ("launch", "{\"stealth\":true}")
  ];

  let response = match client.post("/function", vec![], query, Some(&payload)) {
    Ok(response) => response,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Request to browserless failed: {}", error)
      };
    }
  };

  if response.is_success() == false {
    return ToolOutput {
      state: ToolOutputState::Error,
      content: format!("browserless /function returned status {}: {}", response.status, response.text())
    };
  }

  return ToolOutput {
    state: ToolOutputState::Done,
    content: response.text()
  };
}

define_tool!(
  Web,
  WebParams,
  "Drives a headless browser through the browserless /function API. Provide the body of a JavaScript module that default-exports an async function receiving { page, context } (page is a Puppeteer Page) and returning { data, type }. Use it to browse, scrape or screenshot pages whenever you need fresh or external information you do not already have.",
  vec![Permission::Network],
  run
);
