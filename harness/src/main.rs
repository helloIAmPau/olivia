pub mod config;
pub mod services;
pub mod trigger;
pub mod agent;

use tracing::info;
use tracing_subscriber::EnvFilter;

use config::Config;
use agent::Agent;
use services::init;

#[tokio::main]
async fn main() {
  let filter = match EnvFilter::try_from_default_env() {
    Ok(value) => value,
    Err(_) => EnvFilter::new("info")
  };
  tracing_subscriber::fmt().with_env_filter(filter).init();

  info!("Welcome to Olivia Harness");

  let config = match Config::load().await {
    Ok(config) => config,
    Err(error) => {
      panic!("{}", error);
    }
  };

  let agent = match Agent::new(config.agent) {
    Ok(agent) => agent,
    Err(error) => {
      panic!("{}", error);
    }
  };

  match init(config.services).await {
    Err(error) => {
      panic!("{}", error);
    },
    _ => {}
  };
}
