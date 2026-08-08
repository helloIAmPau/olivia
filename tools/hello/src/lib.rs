use schemars::JsonSchema;

use serde::Deserialize;

use common::define_tool;

#[derive(Deserialize, JsonSchema)]
struct HelloParams {
  suffix: String
}

fn execute_hello(input: HelloParams) -> ToolOutput {
  ToolOutput {
    state: ToolOutputState::Done,
    content: format!("Hello {}", input.suffix)
  }
}

define_tool!(
  HelloTool,
  HelloParams,
  "A simple hello-world tool returning hello + a string received as argument",
  execute_hello
);
