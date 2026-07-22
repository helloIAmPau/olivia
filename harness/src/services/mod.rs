pub mod http;

use std::collections::HashMap;

use tracing::error;
use serde::Deserialize;
use tokio::spawn;

use http::HttpConfig;
use http::init_http;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ServiceConfig {
  Http(HttpConfig)
}

pub struct ServiceError {
}

impl ServiceError {
  pub fn new() -> Self {
    return Self {};
  }
}


pub async fn init(config: HashMap<String, ServiceConfig>) -> Result<(), ServiceError> {
  let mut handles = Vec::new();

  for (name, service_config) in config {
    match service_config {
      ServiceConfig::Http(http_config) => {
        let handle = spawn(async {
          init_http(name, http_config).await
        });
        handles.push(handle);
      }
    }
  }

  for handle in handles {
    let result = match handle.await {
      Err(error) => {
        error!("{}", error);
    
        return Err(ServiceError::new());
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
