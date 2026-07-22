use serde::Deserialize;

#[derive(Deserialize)]
pub struct TriggerConfig {
  pub prompt: String
}
