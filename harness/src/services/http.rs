use std::collections::HashMap;
use std::sync::Arc;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FormatterResult;

use serde::Deserialize;
use serde::Serialize;

use tracing::info;

use tokio::net::TcpListener;

use axum::Router;
use axum::routing::delete;
use axum::routing::get;
use axum::routing::post;
use axum::routing::put;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::serve;
use axum::Json;

use uuid::Uuid;

use crate::services::ServiceError;
use crate::services::ServiceState;
use crate::agent::Agent;
use crate::agent::AgentResult;
use crate::agent::llm_client::ChatMessage;
use crate::agent::llm_client::ChatMessageRole;

const SESSION_HEADER: &str = "OLIVIA_SESSION_ID";

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
pub struct HttpEndpointConfig {
  #[serde(default = "default_path")]
  pub path: String,
  #[serde(default = "default_method")]
  pub method: HttpMethod,
  pub prompt: String
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
  pub endpoints: HashMap<String, HttpEndpointConfig>
}

fn default_port() -> u16 {
  return 80;
}

fn default_address() -> String {
  return "0.0.0.0".to_string();
}

#[derive(Serialize)]
struct HttpAgentResult {
  error: Option<String>,
  data: Option<AgentResult>
}

#[axum::debug_handler]
async fn http_handler(State(state): State<Arc<ServiceState<HttpEndpointConfig>>>, headers: HeaderMap, payload: String) -> Json<HttpAgentResult> {
  info!("Activating {} endpoint via {} HTTP request on {}", state.name, state.config.method, state.config.path);

  let mut system_prompt = format!(r#"
The user activated the HTTP endpopint named {}.
The endpoint is defined to respond to {} requests on {}.
  "#, state.name, state.config.method, state.config.path);

  if payload != "" {
    info!("Received payload\n{}", payload);
    system_prompt = format!(r#"
{}
The request contains a payload as well:
{}
    "#, system_prompt, payload);
  }

  let request = vec![
    ChatMessage {
      role: ChatMessageRole::System,
      content: system_prompt
    },
    ChatMessage {
      role: ChatMessageRole::User,
      content: state.config.prompt.clone()
    }
  ]; 

  let result = match headers.get(SESSION_HEADER) {
    Some(value) => {
      let raw = match value.to_str() {
        Ok(raw) => {
          info!("Found raw session header: {}", raw);

          raw
        },
        Err(error) => {
          return Json(HttpAgentResult {
            error: Some(format!("Invalid {} header: {}", SESSION_HEADER, error)),
            data: None
          });
        }
      };

      let session_id = match Uuid::parse_str(raw) {
        Ok(session_id) => {
          info!("Session {} is a valid session id", session_id);

          session_id
        },
        Err(error) => {
          return Json(HttpAgentResult {
            error: Some(format!("Invalid {} header: {}", SESSION_HEADER, error)),
            data: None
          });
        }
      };

      state.agent.ask(session_id, request).await
    },
    None => {
      info!("Creating new session for request");

      state.agent.accept(request).await
    }
  };

  match result {
    Ok(data) => {
      return Json(HttpAgentResult {
        error: None,
        data: Some(data)
      });
    },
    Err(error) => {
      return Json(HttpAgentResult {
        error: Some(error.to_string()),
        data: None
      });
    }
  };
}

pub async fn init_http(name: String, config: HttpConfig, agent: Arc<Agent>) -> Result<(), ServiceError> {
  info!("Initializng {} service as http service @ http://{}:{}", name, config.address, config.port);
  let mut app = Router::new();
  for (endpoint_name, endpoint_config) in config.endpoints {
    info!("Registered new endpoint {} as [{}] {}", &endpoint_name, &endpoint_config.method, &endpoint_config.path);

    let handler = match endpoint_config.method {
      HttpMethod::Get => get(http_handler),
      HttpMethod::Post => post(http_handler),
      HttpMethod::Put => put(http_handler),
      HttpMethod::Delete => delete(http_handler)
    };

    let path = endpoint_config.path.clone();
    let state = Arc::new(ServiceState::<HttpEndpointConfig> {
      name: endpoint_name,
      config: endpoint_config,
      agent: agent.clone()
    });
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
