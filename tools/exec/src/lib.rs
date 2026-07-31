use extism_pdk::plugin_fn;
use extism_pdk::FnResult;
use extism_pdk::Json;

use serde::Deserialize;

use schemars::JsonSchema;
use schemars::schema_for;

use common::ToolInfo;
use common::ToolOutput;
use common::ToolOutputState;

#[derive(Deserialize, JsonSchema)]
pub struct ExecParams {
  /// The script to execute. It must be a bash script. Take in account the script will run in a Linux environment.
  pub script: String
}

#[plugin_fn]
pub fn info() -> FnResult<Json<ToolInfo>> {
  let info = ToolInfo {
    name: "exec".to_string(),
    description: "This tool executes a command on host machine. You can use it to execute bash scripts.".to_string(),
    params: schema_for!(ExecParams)
  };

  return Ok(Json(info));
}

#[plugin_fn]
pub fn execute(Json(params): Json<ExecParams>) -> FnResult<Json<ToolOutput>> {
  println!("{}", params.script);

  return Ok(Json(ToolOutput {
    state: ToolOutputState::Error,
    output: "Not Implemented!".to_string()
  }));
}
