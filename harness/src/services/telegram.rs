use std::sync::Arc;
use std::collections::HashMap;

use tokio::sync::Mutex;

use uuid::Uuid;

use tracing::info;

use serde::Deserialize;

use teloxide::types::ChatId;
use teloxide::prelude::Requester;
use teloxide::Bot;
use teloxide::requests::ResponseResult;
use teloxide::types::Update;
use teloxide::utils::command::BotCommands;
use teloxide::dispatching::UpdateFilterExt;
use teloxide::dispatching::HandlerExt;
use teloxide::types::Message;
use teloxide::prelude::Dispatcher;
use teloxide::dptree;

use crate::services::ServiceError;
use crate::services::ServiceState;
use crate::agent::Agent;
use crate::agent::llm_client::ChatMessage;
use crate::agent::llm_client::ChatMessageRole;

use crate::agent::AgentPayloadState;
use crate::agent::AgentResult;

#[derive(Deserialize)]
pub struct TelegramConfig {
  pub token: String,
  pub prompt: String
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "These commands are supported:")]
enum Commands {
  #[command(description = "display this text.")]
  Help,
  #[command(description = "let olivia do something.")]
  Do(String)
}

async fn handle_command(bot: Bot, message: Message, command: Commands, state: Arc<ServiceState<TelegramConfig>>, chatid_sessions: Arc<Mutex<HashMap<ChatId, Uuid>>>) -> ResponseResult<()> {
  info!("Received new message on bot {}", state.name);

  match command {
    Commands::Help => {
      match bot.send_message(message.chat.id, Commands::descriptions().to_string()).await {
        Ok(_) => {
          return Ok(());
        },
        Err(error) => {
          return Err(error);
        }
      };
    },
    Commands::Do(prompt) => {
      let request = vec![
        ChatMessage {
          role: ChatMessageRole::User,
          content: state.config.prompt.clone()
        },
        ChatMessage {
          role: ChatMessageRole::User,
          content: prompt
        }
      ];

      match state.agent.accept(request).await {
        Ok(data) => {
          let session_id = data.session_id;

          let mut sessions = chatid_sessions.lock().await;
          sessions.insert(message.chat.id, session_id);
          drop(sessions);

          return send_payload(bot, message, data).await;
        },
        Err(error) => {
          match bot.send_message(message.chat.id, format!("Error: {}", error.to_string())).await {
            Ok(_) => {
              return Ok(());
            },
            Err(error) => {
              return Err(error);
            }
          };
        }
      };
    }
  };
}

async fn handle_message(bot: Bot, message: Message, state: Arc<ServiceState<TelegramConfig>>, chatid_sessions: Arc<Mutex<HashMap<ChatId, Uuid>>>) -> ResponseResult<()> {
  let sessions = chatid_sessions.lock().await;
  let session_id = match sessions.get(&message.chat.id) {
    Some(session_id) => session_id.clone(),
    None => {
      match bot.send_message(message.chat.id, "Unable to restore session").await {
        Ok(_) => {
          return Ok(());
        },
        Err(error) => {
          return Err(error);
        }
      };
    }
  };
  drop(sessions);

  let content = match message.text() {
    Some(content) => content.to_string(),
    None => "Received message with no text".to_string()
  };
  let request = vec![
    ChatMessage {
      role: ChatMessageRole::User,
      content 
    }
  ];

  match state.agent.ask(session_id, request).await {
    Ok(data) => {
      return send_payload(bot, message, data).await;
    },
    Err(error) => {
      match bot.send_message(message.chat.id, format!("Error: {}", error.to_string())).await {
        Ok(_) => {
          return Ok(());
        },
        Err(error) => {
          return Err(error);
        }
      };
    }
  };
}

async fn send_payload(bot: Bot, message: Message, data: AgentResult) -> ResponseResult<()> {
  let reply = match data.payload.state {
    AgentPayloadState::Done => match data.payload.result {
      Some(result) => result,
      None => "The agent finished without producing a result".to_string()
    },
    AgentPayloadState::Error => match data.payload.error_message {
      Some(error_message) => error_message,
      None => "The agent reported an error without a message".to_string()
    },
    AgentPayloadState::Tool => "The agent unexpectedly stopped on a tool step".to_string()
  };

  match bot.send_message(message.chat.id, reply).await {
    Ok(_) => {
      return Ok(());
    },
    Err(error) => {
      return Err(error);
    }
  };
}


pub async fn init_telegram(name: String, config: TelegramConfig, agent: Arc<Agent>) -> Result<(), ServiceError> {
  info!("Initializng {} service as bot telegram", name);

  let chatid_sessions = Arc::new(Mutex::new(HashMap::<ChatId, Uuid>::new()));

  let bot = Bot::new(&config.token);
  let handler = Update::filter_message()
    .branch(dptree::entry().filter_command::<Commands>().endpoint(handle_command))
    .branch(dptree::endpoint(handle_message));
  let state = Arc::new(ServiceState::<TelegramConfig> {
    name,
    config,
    agent
  });
  let deps = dptree::deps![state, chatid_sessions];
  let mut dispatcher = Dispatcher::builder(bot, handler).dependencies(deps).build();
  dispatcher.dispatch().await;

  return Ok(());
}
