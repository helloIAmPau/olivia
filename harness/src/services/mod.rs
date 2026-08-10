pub mod http;
pub mod telegram;
pub mod cron;

use std::collections::HashMap;
use std::sync::Arc;
use std::io::Error;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FormatterResult;

use serde::Deserialize;
use tokio::task::JoinSet;
use tokio::task::JoinError;

use tokio_cron_scheduler::JobSchedulerError;

use http::HttpConfig;
use http::init_http;

use telegram::init_telegram;
use telegram::TelegramConfig;

use cron::CronConfig;
use cron::init_cron;

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
  Telegram(TelegramConfig),
  Cron(CronConfig)
}

#[derive(Debug)]
pub enum ServiceError {
  Io(Error),
  Join(JoinError),
  Cron(JobSchedulerError)
}

impl Display for ServiceError {
  fn fmt(&self, formatter: &mut Formatter) -> FormatterResult {
    return match self {
      ServiceError::Io(error) => write!(formatter, "IO Error - {}", error),
      ServiceError::Join(error) => write!(formatter, "Join Error - {}", error),
      ServiceError::Cron(error) => write!(formatter, "Cron Error - {}", error)
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
      },
      ServiceConfig::Cron(cron_config) => {
        handles.spawn(async move {
          init_cron(name, cron_config, cloned_agent).await
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
