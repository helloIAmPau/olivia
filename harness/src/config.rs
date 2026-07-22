use std::collections::HashMap;

use tokio::fs::read_to_string;
use serde::Deserialize;
use tracing::info;
use tracing::debug;
use tracing::error;

use crate::services::ServiceConfig;

pub struct ConfigError {
}

impl ConfigError {
  pub fn new() -> Self {
    return Self {};
  }
}

#[derive(Deserialize)]
pub struct Config {
  pub services: HashMap<String, ServiceConfig>
}

impl Config {
  pub async fn load() -> Result<Self, ConfigError> {
    let config_path = "/config.yml";
    info!("Loading config file in {}", config_path);

    let plain_config = match read_to_string(config_path).await {
      Ok(plain_config) => plain_config,
      Err(error) => {
        error!("{}", error);

        return Err(ConfigError::new());
      }
    };
    debug!("Config content\n{}", plain_config);

    let config = match serde_yaml::from_str(&plain_config) {
      Ok(config) => config,
      Err(error) => {
        error!("{}", error);

        return Err(ConfigError::new());
      }
    };

    return Ok(config);
  }
}
