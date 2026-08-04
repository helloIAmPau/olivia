use schemars::JsonSchema;
use schemars::schema_for;

use serde::Deserialize;

use serde_json::from_str;
use serde_json::to_string;

use wit_bindgen::generate;

generate!({
  world: "tool-world",
  path: "../tool.wit"
});

#[derive(Deserialize, JsonSchema)]
struct HelloParams {
  suffix: String
}

struct HelloTool;

impl Guest for HelloTool {
  fn info() -> ToolInfo {
    let schema = schema_for!(HelloParams);
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
      name: "Hello World tool".to_string(),
      description: "A simple hello-world tool returning hello + a string received as argument".to_string(),
      schema: schema_json 
    };
  }

  fn run(params: String) -> ToolOutput {
    let input: HelloParams = match from_str(&params) {
      Ok(input) => input,
      Err(error) => {
        return ToolOutput {
          state: ToolOutputState::Error,
          content: format!("Invalid input received: {} {}", params, error)
        };
      }
    };

    return ToolOutput {
      state: ToolOutputState::Done,
      content: format!("Hello {}", input.suffix)
    };
  }
}

export!(HelloTool);
