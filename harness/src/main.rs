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

  info!("Welcome to Oliva Harness");

  let config = match Config::load().await {
    Ok(config) => config,
    Err(_) => {
      panic!();
    }
  };

  let agent = Agent::new(config.agent);

  match init(config.services).await {
    Err(_) => {
      panic!();
    },
    _ => {}
  };
}
