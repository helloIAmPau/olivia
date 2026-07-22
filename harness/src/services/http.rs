use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;
use tracing::info;
use tracing::error;
use tokio::net::TcpListener;
use axum::Router;
use axum::routing::delete;
use axum::routing::get;
use axum::routing::post;
use axum::routing::put;
use axum::extract::Request;
use axum::extract::State;
use axum::serve;

use crate::trigger::TriggerConfig;
use crate::services::ServiceError;

#[derive(Deserialize)]
pub struct HttpTriggerConfig {
  #[serde(default = "default_path")]
  pub path: String,
  #[serde(default = "default_method")]
  pub method: String,

  #[serde(flatten)]
  pub base: TriggerConfig
}

fn default_path() -> String {
  return "/".to_string();
}

fn default_method() -> String {
  return "GET".to_string();
}

#[derive(Deserialize)]
pub struct HttpConfig {
  #[serde(default = "default_port")]
  pub port: u16,
  #[serde(default = "default_address")]
  pub address: String,
  pub triggers: HashMap<String, HttpTriggerConfig>
}

fn default_port() -> u16 {
  return 80;
}

fn default_address() -> String {
  return "0.0.0.0".to_string();
}

async fn http_handler(State(state): State<Arc<HttpTriggerConfig>>, request: Request) {
  
}

pub async fn init_http(name: String, config: HttpConfig) -> Result<(), ServiceError> {
  info!("Initializng {} service as http service @ http://{}:{}", name, config.address, config.port);
  let mut app = Router::new();
  for (trigger_name, trigger_config) in config.triggers {
    let method = trigger_config.method.to_uppercase();

    let handler = match method.as_str() {
      "GET" => get(http_handler),
      "POST" => post(http_handler),
      "PUT" => put(http_handler),
      "DELETE" => delete(http_handler),
      _ => {
        error!("Invalid method selected {}", method);

        return Err(ServiceError::new());
      }
    };

    let path = trigger_config.path.clone();
    let state = Arc::new(trigger_config);
    app = app.route(&path, handler.with_state(state));
    info!("Registered new trigger {} as [{}] {}", trigger_name, method, path);
  }

  let address = format!("{}:{}", config.address, config.port);
  let listener = match TcpListener::bind(&address).await {
    Ok(listener) => listener,
    Err(error) => {
      error!("{}", error);

      return Err(ServiceError::new());
    }
  };

  match serve(listener, app).await {
    Err(error) => {
      error!("{}", error);

      return Err(ServiceError::new());
    },
    _ => {
      return Ok(());
    }
  };
}
