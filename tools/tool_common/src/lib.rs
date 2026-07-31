use serde::Deserialize;
use serde::Serialize;

use schemars::Schema;

#[derive(Serialize, Deserialize)]
pub struct ToolInfo {
  pub name: String,
  pub description: String,
  pub params: Schema
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolOutputState {
  Done,
  Error
}

#[derive(Serialize, Deserialize)]
pub struct ToolOutput {
  pub state: ToolOutputState,
  pub output: String
}
