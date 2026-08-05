pub mod http;
pub mod telegram;

use std::collections::HashMap;
use std::sync::Arc;
use std::io::Error;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FormatterResult;

use serde::Deserialize;
use tokio::task::JoinSet;
use tokio::task::JoinError;

use http::HttpConfig;
use http::init_http;

use telegram::init_telegram;
use telegram::TelegramConfig;

use crate::agent::Agent;

struct ServiceState<T> {
  pub name: String,
  pub config: T,
  pub agent: Arc<Agent>
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ServiceConfig {
  Http(HttpConfig),
  Telegram(TelegramConfig)
}

#[derive(Debug)]
pub enum ServiceError {
  Io(Error),
  Join(JoinError)
}

impl Display for ServiceError {
  fn fmt(&self, formatter: &mut Formatter) -> FormatterResult {
    return match self {
      ServiceError::Io(error) => write!(formatter, "IO Error - {}", error),
      ServiceError::Join(error) => write!(formatter, "Join Error - {}", error)
    };
  }
}

pub async fn init(config: HashMap<String, ServiceConfig>, agent: Agent) -> Result<(), ServiceError> {
  let mut handles = JoinSet::new();
  let arc_agent = Arc::new(agent);

  for (name, service_config) in config {
    let cloned_agent = arc_agent.clone();

    match service_config {
      ServiceConfig::Http(http_config) => {
        handles.spawn(async move {
          init_http(name, http_config, cloned_agent).await
        });
      },
      ServiceConfig::Telegram(telegram_config) => {
        handles.spawn(async move {
          init_telegram(name, telegram_config, cloned_agent).await
        });
      }
    }
  }

  while let Some(joined) = handles.join_next().await {
    let result = match joined {
      Err(error) => {
        return Err(ServiceError::Join(error));
      },
      Ok(result) => result
    };

     match result {
      Err(error) => {
        return Err(error);
      },
      _ => {}
    };
  }

  return Ok(());
}
