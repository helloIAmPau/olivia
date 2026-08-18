use std::collections::HashMap;
use std::io::Error;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FormatterResult;

use toml::de::Error as TomlError;
use toml::from_str;

use tokio::fs::read_to_string;
use serde::Deserialize;
use tracing::info;
use tracing::debug;

use crate::services::ServiceConfig;
use crate::agent::AgentConfig;

#[derive(Debug)]
pub enum ConfigError {
  Io(Error),
  Parsing(TomlError)
}

impl Display for ConfigError {
  fn fmt(&self, formatter: &mut Formatter) -> FormatterResult {
    return match self {
      ConfigError::Io(error) => write!(formatter, "IO Error - {}", error),
      ConfigError::Parsing(error) => write!(formatter, "Yaml Parsing Error - {}", error)
    };
  }
}

#[derive(Deserialize)]
pub struct Config {
  pub services: HashMap<String, ServiceConfig>,
  pub agent: AgentConfig
}

impl Config {
  pub async fn load(config_path: &str) -> Result<Self, ConfigError> {
    info!("Loading config file in {}", config_path);

    let plain_config = match read_to_string(config_path).await {
      Ok(plain_config) => plain_config,
      Err(error) => {
        return Err(ConfigError::Io(error));
      }
    };
    debug!("Config content\n{}", plain_config);

    let config = match from_str(&plain_config) {
      Ok(config) => config,
      Err(error) => {
        return Err(ConfigError::Parsing(error));
      }
    };

    return Ok(config);
  }
}
