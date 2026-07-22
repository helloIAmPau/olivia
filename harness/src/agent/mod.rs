pub mod client;

use serde::Deserialize;

use client::Client;

#[derive(Deserialize)]
pub struct AgentConfig {
  pub prompt: String
}

pub struct Agent {
  client: Client
}

impl Agent {
  pub fn new(config: AgentConfig) -> Self {
    let client = Client::new();

    return Self {
      client: client
    };
  }
}
