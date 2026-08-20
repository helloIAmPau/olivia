use std::sync::Arc;

use tokio::sync::Mutex;

use tokio_cron_scheduler::Job;
use tokio_cron_scheduler::JobScheduler;

use tracing::info;
use tracing::warn;
use tracing::error;

use serde::Deserialize;

use crate::services::ServiceError;
use crate::agent::Agent;
use crate::agent::llm_client::ChatMessage;
use crate::agent::llm_client::ChatMessageRole;

#[derive(Deserialize)]
pub struct CronConfig {
  pub schedule: String,
  pub prompt: String
}

pub async fn init_cron(name: String, config: CronConfig, agent: Arc<Agent>) -> Result<(), ServiceError> {
  info!("Initializng {} service as cron service {}", &name, &config.schedule);

  let scheduler = match JobScheduler::new().await {
    Ok(scheduler) => scheduler,
    Err(error) => {
      return Err(ServiceError::Cron(error));
    }
  };

  let schedule = config.schedule;
  let prompt = config.prompt;
  let job_schedule = schedule.clone();
  let lock = Arc::new(Mutex::new(()));

  let job = match Job::new_async(job_schedule, move |_uuid, _scheduler| {
    let name = name.clone();
    let schedule = schedule.clone();
    let prompt = prompt.clone();
    let agent = agent.clone();
    let lock = lock.clone();

    return Box::pin(async move {
      info!("Cron job {} activated ({})", name, schedule);

      match lock.try_lock() {
        Err(_) => {
          warn!("Cron job {} still running... Skipping", name);
        },
        _ => {}
      }

      let system_prompt = format!(r#"
A cron job called {} with the following schedule {} has activated.
      "#, name, schedule);

      let request = vec![
        ChatMessage {
          role: ChatMessageRole::System,
          content: system_prompt
        },
        ChatMessage {
          role: ChatMessageRole::User,
          content: prompt
        }
      ];

      match agent.accept(request).await {
        Ok(_) => {
          info!("Cron job {} completed", name);
        },
        Err(error) => {
          error!("Cron job {} failed: {}", name, error);
        }
      };
    });
  }) {
    Ok(job) => job,
    Err(error) => {
      return Err(ServiceError::Cron(error));
    }
  };

  match scheduler.add(job).await {
    Err(error) => {
      return Err(ServiceError::Cron(error));
    },
    _ => {}
  };

  match scheduler.start().await {
    Err(error) => {
      return Err(ServiceError::Cron(error));
    },
    _ => {}
  };

  return Ok(());
}
