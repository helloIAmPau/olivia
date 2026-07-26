pub mod http;

use std::collections::HashMap;
use std::io::Error;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FormatterResult;

use serde::Deserialize;
use tokio::task::JoinSet;
use tokio::task::JoinError;

use http::HttpConfig;
use http::init_http;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ServiceConfig {
  Http(HttpConfig)
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

pub async fn init(config: HashMap<String, ServiceConfig>) -> Result<(), ServiceError> {
  let mut handles = JoinSet::new();

  for (name, service_config) in config {
    match service_config {
      ServiceConfig::Http(http_config) => {
        handles.spawn(async move {
          init_http(name, http_config).await
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
