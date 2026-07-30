use extism_pdk::plugin_fn;
use extism_pdk::FnResult;
use extism_pdk::Json;

use serde::Deserialize;
use serde::Serialize;

use schemars::JsonSchema;
use schemars::schema_for;
use schemars::Schema;

#[derive(Deserialize, JsonSchema)]
pub struct ExecParams {
  /// The script to execute. It must be a bash script. Take in account the script will run in a Linux environment.
  pub script: String
}

#[derive(Serialize)]
pub struct ExecInfo {
  pub name: &'static str,
  pub description: &'static str,
  pub params: Schema
}

#[plugin_fn]
pub fn info() -> FnResult<Json<ExecInfo>> {
  let info = ExecInfo {
    name: "exec",
    description: "This tool executes a command on host machine. You can use it to execute bash scripts.",
    params: schema_for!(ExecParams)
  };

  return Ok(Json(info));
}

#[plugin_fn]
pub fn execute() -> FnResult<()> {
  return Ok(());
}
