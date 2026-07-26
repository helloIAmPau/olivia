use std::collections::HashMap;
use std::sync::Arc;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FormatterResult;

use serde::Deserialize;
use tracing::info;
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
pub enum HttpMethod {
  #[serde(alias = "GET", alias = "get")]
  Get,
  #[serde(alias = "POST", alias = "post")]
  Post,
  #[serde(alias = "PUT", alias = "put")]
  Put,
  #[serde(alias = "DELETE", alias = "delete")]
  Delete
}

impl Display for HttpMethod {
  fn fmt(&self, formatter: &mut Formatter) -> FormatterResult {
    match self {
      HttpMethod::Get => write!(formatter, "GET"),
      HttpMethod::Post => write!(formatter, "POST"),
      HttpMethod::Put => write!(formatter, "PUT"),
      HttpMethod::Delete => write!(formatter, "DELETE")
    }
  }
}

#[derive(Deserialize)]
pub struct HttpTriggerConfig {
  #[serde(default = "default_path")]
  pub path: String,
  #[serde(default = "default_method")]
  pub method: HttpMethod,

  #[serde(flatten)]
  pub base: TriggerConfig
}

fn default_path() -> String {
  return "/".to_string();
}

fn default_method() -> HttpMethod {
  return HttpMethod::Get;
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
    info!("Registered new trigger {} as [{}] {}", &trigger_name, &trigger_config.method, &trigger_config.path);

    let handler = match trigger_config.method {
      HttpMethod::Get => get(http_handler),
      HttpMethod::Post => post(http_handler),
      HttpMethod::Put => put(http_handler),
      HttpMethod::Delete => delete(http_handler)
    };

    let path = trigger_config.path.clone();
    let state = Arc::new(trigger_config);
    app = app.route(&path, handler.with_state(state));
  }

  let address = format!("{}:{}", config.address, config.port);
  let listener = match TcpListener::bind(&address).await {
    Ok(listener) => listener,
    Err(error) => {
      return Err(ServiceError::Io(error));
    }
  };

  match serve(listener, app).await {
    Err(error) => {
      return Err(ServiceError::Io(error));
    },
    _ => {
      return Ok(());
    }
  };
}
