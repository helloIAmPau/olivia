use std::sync::Arc;

use tracing::info;
use tracing::error;

use serde::Deserialize;

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
use crate::agent::AgentPayloadState;
use crate::agent::llm_client::ChatMessage;
use crate::agent::llm_client::ChatMessageRole;

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

async fn answer(bot: Bot, message: Message, command: Commands, state: Arc<ServiceState<TelegramConfig>>) -> ResponseResult<()> {
  info!("Received new message on bot {}", state.name);

  match command {
    Commands::Help => {
      match bot.send_message(message.chat.id, Commands::descriptions().to_string()).await {
        Ok(_) => {
          return Ok(());
        },
        Err(error) => {
          error!("{}", error);

          return Err(error);
        }
      }
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
    
      let reply = match state.agent.accept(request).await {
        Ok(data) => {
          let mut reply = match data.state {
            AgentPayloadState::Done => "Success:".to_string(),
            AgentPayloadState::Error => "Error:".to_string(),
            _ => "".to_string()
          };

          reply = match data.result {
            Some(result) => format!("{} {}", reply, result),
            None => reply
          };

          reply = match data.message {
            Some(error_message) => format!("{} {}", reply, error_message),
            None => reply
          };

          reply
        },
        Err(error) => {
          format!("Error: {}", error.to_string())
        }
      };

      match bot.send_message(message.chat.id, reply).await {
        Ok(_) => {
          return Ok(());
        },
        Err(error) => {
          error!("{}", error);

          return Err(error);
        }
      };
    }
  };
}

pub async fn init_telegram(name: String, config: TelegramConfig, agent: Arc<Agent>) -> Result<(), ServiceError> {
  info!("Initializng {} service as bot telegram", name);

  let bot = Bot::new(&config.token);
  let handler = Update::filter_message().filter_command::<Commands>().endpoint(answer);
  let state = Arc::new(ServiceState::<TelegramConfig> {
    name,
    config,
    agent
  });
  let deps = dptree::deps![state];
  let mut dispatcher = Dispatcher::builder(bot, handler).dependencies(deps).build();

  dispatcher.dispatch().await;

  return Ok(());
}
