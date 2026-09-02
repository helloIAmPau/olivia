pub mod llm_client;
pub mod tool_registry;
pub mod tool;

use std::collections::HashMap;
use std::sync::Mutex;
use std::io::Error as IoError;
use std::fmt::Result as FormatterResult;
use std::fmt::Formatter;
use std::fmt::Display;
use std::env::VarError;

use serde::Deserialize;
use serde::Serialize;
use serde_json::Error as ParsingError;
use serde_json::from_str;

use reqwest::header::InvalidHeaderValue;
use reqwest::Error as ReqwestError;

use wasmtime::Error as WasmError;

use tracing::debug;
use tracing::error;
use tracing::info;

use schemars::JsonSchema;

use uuid::Uuid;

use llm_client::LLMClient;
use tool_registry::ToolRegistry;

use crate::agent::tool::GUEST_SANDBOX_PATH;

use crate::agent::llm_client::ChatMessage;
use crate::agent::llm_client::ChatMessageRole;
use crate::agent::llm_client::ChatRequest;
use crate::agent::llm_client::ChatResponse;

#[derive(Debug)]
pub enum AgentError {
  Schema,
  Var(&'static str, VarError),
  InvalidHeaderValue(InvalidHeaderValue),
  Request(ReqwestError),
  Model(&'static str, String),
  Parsing(ParsingError),
  Completions(ChatRequest, ChatResponse),
  LLMRequest(u16, String),
  MaxIterations,
  TooManyFailures,
  Agent(String),
  Io(IoError),
  InvalidToolInput(String, String, &'static str),
  Tool(String),
  SessionMutex(String),
  Session(Uuid),
  Wasm(WasmError)
}

impl Display for AgentError {
  fn fmt(&self, formatter: &mut Formatter) -> FormatterResult {
    return match self {
      AgentError::Schema => write!(formatter, " Schema Error - Invalid schema for agent request"),
      AgentError::Io(error) => write!(formatter, "Io Error - {}", error),
      AgentError::Var(name, error) => write!(formatter, "Var Error - {} {}", name, error),
      AgentError::InvalidHeaderValue(error) => write!(formatter, "Invalid Header Value Error - {}", error),
      AgentError::Request(error) => write!(formatter, "Request Error - {}", error),
      AgentError::Parsing(error) => write!(formatter, "Parsing Error - {}", error),
      AgentError::Model(message, model) => write!(formatter, "[{}] Model Error - {}", model, message),
      AgentError::Completions(request, response) => write!(formatter, "Invalid response from model\nrequest:\n{:#?}\nresponse:\n{:#?}", request, response),
      AgentError::LLMRequest(status, body) => write!(formatter, "LLM Request Error - upstream returned HTTP {}: {}", status, body),
      AgentError::MaxIterations => write!(formatter, "Max agentic loop iterations reached. Aborted trigger"),
      AgentError::TooManyFailures => write!(formatter, "Too many consecutive failures (unparseable replies or tool errors). Aborted trigger"),
      AgentError::Agent(message) => write!(formatter, "Agent Error - {}", message),
      AgentError::Tool(error) => write!(formatter, "Tool Error - {}", error),
      AgentError::Wasm(error) => write!(formatter, "Wasm Error - {}", error),
      AgentError::SessionMutex(error) => write!(formatter, "Session Mutex Error - Unable to lock the sessions store: {}", error),
      AgentError::Session(session_id) => write!(formatter, "Session Error - Invalid session id {}", session_id),
      AgentError::InvalidToolInput(name, params, message) => write!(formatter, "Invalid Tool Input Error - {} {}({})", message, name, params)
    }
  }
}

#[derive(Deserialize)]
pub struct PostgresConfig {
  pub connection_string: String,
  pub prompt: String
}

fn default_clickhouse_username() -> String {
  return "default".to_string();
}

fn default_clickhouse_password() -> String {
  return "".to_string();
}

#[derive(Deserialize)]
pub struct ClickhouseConfig {
  pub host: String,
  #[serde(default = "default_clickhouse_username")]
  pub username: String,
  #[serde(default = "default_clickhouse_password")]
  pub password: String,
  pub prompt: String
}

fn default_s3_region() -> String {
  return "us-east-1".to_string();
}

#[derive(Deserialize)]
pub struct S3Config {
  pub bucket: String,
  #[serde(default = "default_s3_region")]
  pub region: String,
  pub endpoint: String,
  pub access_key: String,
  pub secret_key: String,
  pub prompt: String
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AgentStoreConfig {
  Postgres(PostgresConfig),
  Clickhouse(ClickhouseConfig),
  S3(S3Config)
}

fn default_stores() -> HashMap<String, AgentStoreConfig> {
  return HashMap::new();
}

#[derive(Deserialize)]
pub struct AgentConfig {
  pub prompt: String,
  pub model: String,
  #[serde(default = "default_stores")]
  pub stores: HashMap<String, AgentStoreConfig>
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AgentPayloadState {
  /// Task execution is complete
  Done,
  /// An unrecoverable error occurred
  Error,
  /// Execute a tool
  Tool
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Thought {
  /// User objective, <=20 words. Written once, copied forward verbatim. Never edit.
  pub goal: String,
  /// Steps as "<n>. <imperative, <=10 words>". Max 8. Indices never reused; gaps expected after replan. Entries with a `done` record are frozen.
  pub plan: Vec<String>,
  /// Append-only log: "<n>:<ok|fail> <outcome, <=8 words>". Outcomes, not narrative. Never restate tool output. Retries repeat an index.
  pub done: Vec<String>,
  /// Values later steps need: paths, IDs, counts. Terse keys. Drop when no remaining step needs them. No credentials, no file contents.
  pub facts: HashMap<String, String>,
  /// Step running now: "<n> <action>". Justify only if non-obvious. "<n> complete" when done, "<n> abort" on error.
  pub cur: String
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct AgentPayload {
  /// Your memory. Used for internal reasoning, plan for the current step, keep track of the execution and more. Required for all states.
  pub thought: String,
  /// the execution result
  pub state: AgentPayloadState,
  /// your output if the execution succeeded. Required for state = Done, null otherwise
  pub result: Option<String>,
  /// your error message, if any, if the execution failed. Required for state = Error, null otherwise
  pub error_message: Option<String>,
  /// the name of the tool to use. Required for state = Tool, null otherwise
  pub name: Option<String>,
  /// a json string rapresenting the tool parameters. Look at the tool section to learn how to set the parameters for each tool. Required for state = Tool, null otherwise.
  pub params: Option<String>
}

#[derive(Serialize)]
pub struct ToolOutput {
  pub output: String
}

#[derive(Serialize)]
pub struct AgentResult {
  pub session_id: Uuid,
  pub payload: AgentPayload
}

const MAX_ITERATIONS: i32 = 200;
const MAX_FAILURES: i32 = 5;

pub struct Agent {
  client: LLMClient,
  config: AgentConfig,
  registry: ToolRegistry,
  store_prompt: String,
  sessions: Mutex<HashMap<Uuid, String>>
}

impl Agent {
  pub async fn new(config: AgentConfig, tools_folder: &str, sandbox_folder: &str) -> Result<Self, AgentError> {
    let client = match LLMClient::new() {
      Ok(client) => client,
      Err(error) => {
        return Err(error);
      }
    };

    debug!("Checking model {} exists on proxy", config.model);
    let models = match client.models().await {
      Ok(models) => models,
      Err(error) => {
        return Err(error);
      }
    };

    let has_model_id = models.iter().any(|model| model.id == config.model);
    if has_model_id == false {
      return Err(AgentError::Model("Model not found on proxy. Please check your configuration", config.model));
    }

    debug!("Model {} found", config.model);

    let registry = match ToolRegistry::new(tools_folder, sandbox_folder).await {
      Ok(registry) => registry,
      Err(error) => {
        return Err(error);
      }
    };

    let mut store_prompt = "".to_string();
    for (name, config) in &config.stores {
      match config {
        AgentStoreConfig::Postgres(postgres_config) => {
          store_prompt = format!("{}\n{} - {}\ntype: postgres\nconnection string: {}\n", store_prompt, name, postgres_config.prompt, postgres_config.connection_string);
        },
        AgentStoreConfig::Clickhouse(clickhouse_config) => {
          store_prompt = format!("{}\n{} - {}\ntype: clickhouse\nhost: {}\nusername: {}\npassword: {}\n", store_prompt, name, clickhouse_config.prompt, clickhouse_config.host, clickhouse_config.username, clickhouse_config.password);
        },
        AgentStoreConfig::S3(s3_config) => {
          store_prompt = format!("{}\n{} - {}\ntype: s3\nbucket: {}\nregion: {}\nendpoint: {}\naccess_key: {}\nsecret_key: {}\n", store_prompt, name, s3_config.prompt, s3_config.bucket, s3_config.region, s3_config.endpoint, s3_config.access_key, s3_config.secret_key);
        }
      }
    }

    let agent = Self {
      client,
      config,
      registry,
      store_prompt,
      sessions: Mutex::new(HashMap::new())
    };

    return Ok(agent);
  }

  pub async fn accept(&self, request: Vec<ChatMessage>, maybe_session_id: Option<Uuid>) -> Result<AgentResult, AgentError> {
    let context = format!(r#"
You are OlivIA, a strict AI task coordinator. Your SOLE function is to analyze requests, plan a sequence of actions, and delegate execution to external tools in an iterative loop.

CRITICAL BEHAVIORAL RULES:

1. ITERATIVE EXECUTION (THE LOOP): You operate in a continuous Plan -> Act -> Observe loop. You are capable of chaining multiple tools to complete complex tasks. Execute ONE step at a time. Call a tool, wait for the environment to return the result, and then evaluate your next step. Continue this loop until the overarching goal is achieved, then return a "done" state.

2. DELEGATE EXECUTION: Do NOT calculate, process heavy logic, or attempt to fulfill execution steps using your internal knowledge. You must use the provided tools to execute ANY action, retrieve ANY information, or process ANY logic. You are a router and coordinator.

3. HANDLING LARGE & BINARY FILES: NEVER attempt to read, output, or pass binary data (images, PDFs, raw file bytes, large text dumps) directly into your conversational context. You must save all downloads and generated files directly to `{sandbox}` using the `download`, `fs`, or `s3_client` tools. If you need to extract information from a downloaded file, write a script using the `python` tool to process the file in the sandbox and return only the requested text/metadata.

4. STRICT JSON ONLY: You must respond ONLY with raw, deserializable JSON. Do NOT include markdown formatting, code blocks (e.g., ```json), or any conversational text before or after the JSON object.

   "thought" and "params" are both JSON-ENCODED STRINGS. Each holds a complete
   JSON object, serialized: opening quote, escaped inner quotes, closing quote.
   The SAME rule applies to both — there is no exception and no asymmetry.

     RIGHT: "thought": "{{\"goal\":\"fetch report\",\"plan\":[\"1. download\"]}}"
     WRONG: "thought": {{"goal": "fetch report", "plan": ["1. download"]}}

     RIGHT: "params": "{{\"query\":\"weather Milan\"}}"
     WRONG: "params": {{"query": "weather Milan"}}

   Both strings must deserialize cleanly. Escape every inner quote. Escape
   newlines inside values as \n. Emit no raw line breaks inside either string.

5. STATEFUL CHAIN OF THOUGHT: The conversation history is NOT preserved between
   iterations. The "thought" object is your ONLY memory. It must be self-sufficient:
   a fresh model instance receiving only your last "thought" plus the newest tool
   result must be able to continue the task correctly.

   Once decoded, the "thought" string must yield an object with these keys, all
   five present on every iteration:
     "goal"  : string. The user's objective, restated once, <=20 words. Copy it
               forward VERBATIM every iteration. Never re-derive or reword it.
     "plan"  : array of strings. The full step list, one short imperative clause
               each (<=10 words), prefixed by index: "1. fetch csv", "2. parse".
               Written on the FIRST iteration and copied forward UNCHANGED unless
               replanning is required (see below).
     "done"  : array of strings. One entry per completed step, format
               "<index>:<ok|fail> <outcome in <=8 words>". Append only.
     "facts" : object mapping terse keys to string values. Durable values later
               steps need: file paths, IDs, counts, URLs, schema names. Delete an
               entry once no step in "plan" still needs it. NEVER put credentials
               or file contents here.
     "cur"   : string. The step being executed NOW, format "<index> <action>",
               plus a brief justification only when the choice is non-obvious.

   Nothing validates the contents of the "thought" string for you. A missing key
   or a broken escape means the response is discarded and the step is lost.

   TOKEN DISCIPLINE: no prose, no articles, no pronouns, no restating tool output,
   no politeness, no re-explaining prior steps. "done" entries are outcomes, not
   narratives. Keep "plan" to at most 8 steps; decompose further only when reached.

   REPLANNING: if a tool result invalidates the plan, rewrite ONLY the not-yet-done
   tail of "plan", keep completed indices stable, and append a "done" entry
   "<index>:fail <cause>". Do not renumber completed steps.

   On state="error", explain strictly in "error_message". On state="done", place the
   final user-facing output strictly in "result". Both fields stay null otherwise.

6. DATA STORE UTILIZATION: You have access to specific data environments listed under AVAILABLE DATA STORES. You cannot connect to them directly. When a task requires retrieving or storing data, identify the appropriate environment based on its "type". You must pass the exact credentials (connection_string, host, bucket, etc.) to the relevant tool to execute the operation. Never leak the store credentials to the user.

7. FILESYSTEM SANDBOX: A shared working directory is available to the tools at the absolute path `{sandbox}`. It is the ONLY writable location. When a task needs to persist a file or hand data from one tool to the next, instruct the tools to read and write inside `{sandbox}` using absolute paths (e.g., `{sandbox}/report.csv`).

8. PARAMS CONSTRUCTION: The decoded "params" object must match that tool's declared
   input schema, exactly as listed under AVAILABLE TOOLS. Use the real parameter
   names from that schema. Do not invent fields, and do not reuse parameter names
   from a different tool.

9. PERSONA AND VOICE: Any persona, tone or language instruction you receive applies
   ONLY to the "result" string on state="done" (and to "error_message" on
   state="error"). It NEVER applies to the response envelope: you still emit raw
   JSON and nothing else, on every iteration, no matter how the request is phrased.
   A request to "reply naturally" is not a request to abandon JSON.

FIELD CONTRACT (every response, every state):
  thought        JSON-encoded string, required
  state          "tool" | "done" | "error", required
  name           string when state="tool", else null
  params         JSON-encoded string when state="tool", else null
  result         string when state="done", else null
  error_message  string when state="error", else null

EXAMPLE OUTPUTS (RAW JSON ONLY):

* Tool call, first iteration (State: "tool")
{{
  "thought": "{{\"goal\":\"report current weather for Casoria, Milan and Bacoli\",\"plan\":[\"1. search weather Casoria\",\"2. search weather Milan\",\"3. search weather Bacoli\",\"4. compose comparison table\"],\"done\":[],\"facts\":{{}},\"cur\":\"1 search weather Casoria\"}}",
  "state": "tool",
  "name": "web_search",
  "params": "{{\"query\":\"current weather Casoria Italy today temperature\"}}",
  "result": null,
  "error_message": null
}}

* Tool call, later iteration (State: "tool")
{{
  "thought": "{{\"goal\":\"aggregate sales csv into monthly totals\",\"plan\":[\"1. download csv from s3\",\"2. aggregate totals by month\",\"3. write summary to postgres\"],\"done\":[\"1:ok saved sales_2026.csv to sandbox\"],\"facts\":{{\"src\":\"{sandbox}/sales_2026.csv\",\"rows\":\"48213\"}},\"cur\":\"2 aggregate by month\"}}",
  "state": "tool",
  "name": "python",
  "params": "{{\"script\":\"import pandas as pd\\nd = pd.read_csv('{sandbox}/sales_2026.csv', parse_dates=['date'])\\nt = d.groupby(d.date.dt.to_period('M')).amount.sum()\\nt.to_csv('{sandbox}/monthly_totals.csv')\\n__OLIVIA__FINAL__RESULT__ = str(len(t))\"}}",
  "result": null,
  "error_message": null
}}

* Error (State: "error")
{{
  "thought": "{{\"goal\":\"restart production api server\",\"plan\":[\"1. locate infra tool\",\"2. issue restart\"],\"done\":[\"1:fail no infra tool in toolset\"],\"facts\":{{}},\"cur\":\"1 abort, no capable tool\"}}",
  "state": "error",
  "error_message": "No tool available for infrastructure management. Cannot restart the server.",
  "name": null,
  "params": null,
  "result": null
}}

* Success (State: "done")
{{
  "thought": "{{\"goal\":\"aggregate sales csv into monthly totals\",\"plan\":[\"1. download csv from s3\",\"2. aggregate totals by month\",\"3. write summary to postgres\"],\"done\":[\"1:ok saved sales_2026.csv to sandbox\",\"2:ok 12 monthly buckets\",\"3:ok inserted into monthly_sales\"],\"facts\":{{\"table\":\"monthly_sales\"}},\"cur\":\"3 complete\"}}",
  "state": "done",
  "result": "Aggregated 48,213 sales rows into 12 monthly buckets and stored them in the monthly_sales table.",
  "error_message": null,
  "name": null,
  "params": null
}}

AVAILABLE TOOLS:
{tools}

AVAILABLE DATA STORES:
{stores}
    "#, tools = self.registry.prompt, stores = self.store_prompt, sandbox = GUEST_SANDBOX_PATH);

    let system_prompts = vec![
      ChatMessage {
        role: ChatMessageRole::System,
        content: context.to_string()
      },
      ChatMessage {
        role: ChatMessageRole::System,
        content: self.config.prompt.clone()
      }
    ];

    let mut payload = vec![];
    payload.extend_from_slice(&system_prompts);
    payload.extend_from_slice(&request);

    let session_id = match maybe_session_id {
      Some(session_id) => {
        let sessions = match self.sessions.lock() {
          Ok(sessions) => sessions,
          Err(error) => {
            return Err(AgentError::SessionMutex(error.to_string()));
          }
        };

        let previous_state = match sessions.get(&session_id) {
          Some(previous_state) => {
            format!(r#"
PREVIOUS_STATE:
{}
          "#, previous_state)
          },
          None => {
            return Err(AgentError::Session(session_id));
          }
        };
        payload.push(ChatMessage {
          role: ChatMessageRole::User,
          content: previous_state
        });

        info!("Restored session {}", session_id);

        session_id
      },
      None => {
        let session_id = Uuid::new_v4();
        info!("New session {}", session_id);

        session_id
      }
    };

    let mut iterations = 0;
    let mut failures = 0;

    loop {
      if iterations >= MAX_ITERATIONS {
        return Err(AgentError::MaxIterations);
      }

      if failures >= MAX_FAILURES {
        return Err(AgentError::TooManyFailures);
      }

      iterations = iterations + 1;
      debug!("Agentic iteration {}/{}", iterations, MAX_ITERATIONS);

      info!("Session {}:\n{:#?}", session_id, payload);

      let assistant_chat_message = match self.client.completions(self.config.model.to_string(), &payload).await {
        Ok(assistant_chat_message) => assistant_chat_message,
        Err(error) => {
          error!("Error in iteration {}/{}\n{}", iterations, MAX_ITERATIONS, error);

          return Err(error);
        }
      };

      info!("Session {}:\n{:#?}", session_id, assistant_chat_message);

      let agent_payload: AgentPayload = match from_str(&assistant_chat_message.content) {
        Ok(agent_payload) => agent_payload,
        Err(error) => {
          let feedback = format!(r#"
SYSTEM ERROR: JSON DESERIALIZATION FAILED.

Your last response was DISCARDED. No tool was executed.

Decoder error: {}
<your_invalid_response>
{}
</your_invalid_response>

This is a formatting error, not a task error. Recover "goal", "plan", "done" and
"facts" from your invalid response above and carry them forward UNCHANGED — do
not replan, do not append a "fail" entry. Re-emit the same intended action as a
single raw JSON object.
          "#, error, assistant_chat_message.content);

          let next_message = ChatMessage {
            role: ChatMessageRole::User,
            content: feedback
          };

          if payload.len() - system_prompts.len() == request.len() {
            payload.push(next_message);
          } else {
            let last_index = payload.len() - 1;
            payload[last_index] = next_message;
          }

          failures = failures + 1;

          continue;
        }
      };

      match agent_payload.state {
        AgentPayloadState::Done => {
          match self.sessions.lock() {
            Ok(mut sessions) => { 
              sessions.insert(session_id, assistant_chat_message.content);
            },
            Err(error) => {
              error!("Unable to lock the sessions store to save {}: {}", session_id, error);
            }
          };

          return Ok(AgentResult {
            payload: agent_payload,
            session_id
          });
        },
        AgentPayloadState::Error => {
          match self.sessions.lock() {
            Ok(mut sessions) => { 
              sessions.insert(session_id, assistant_chat_message.content);
            },
            Err(error) => {
              error!("Unable to lock the sessions store to save {}: {}", session_id, error);
            }
          };

          let message = match agent_payload.error_message {
            Some(message) => message,
            None => "No error message".to_string()
          };

          return Err(AgentError::Agent(message));
        },
        AgentPayloadState::Tool => {
          let params = match agent_payload.params {
            Some(params) => params,
            None => "".to_string()
          };

          let name = match agent_payload.name {
            Some(name) => name,
            None => "".to_string()
          };

          let tool_output = match self.registry.run(name, params, &session_id).await {
            Ok(tool_output) => {
              failures = 0;

              format!(r#"
PREVIOUS_STATE:
{}

TOOL EXECUTED

<result>
{}
</result>

Bind this result to the step in "cur", append its outcome to "done".
              "#, assistant_chat_message.content, tool_output)
            },
            Err(AgentError::InvalidToolInput(_, _, message)) => {
              failures = failures + 1;

              format!(r#"
PREVIOUS_STATE:
{}

SYSTEM ERROR: INVALID TOOL REQUEST.

Your JSON was valid but the request failed validation. Nothing was executed.

Reason: {}

CORRECTION:
1. This is a request error, not a task failure. Keep "goal", "plan", "done" and
   "facts" unchanged. Do NOT append a "fail" entry — no step ran. Do not replan.
2. Pass only parameters listed in the signature. Do not invent parameter names.

Re-emit the same intended step, corrected, as a single raw JSON object.
              "#, assistant_chat_message.content, message)
            },
            Err(error) => {
              failures = failures + 1;

              format!(r#"
PREVIOUS_STATE:
{}

TOOL EXECUTION FAILED

<error>
{}
</error>

Your JSON was valid and the tool ran; the tool itself failed. This IS a task
event: unlike a rejected request, it may be recorded in "done".

Choose ONE:
  RETRY - transient failure (timeout, rate limit, 5xx, lock). Reissue the step UNCHANGED. Do not edit "plan" or "done".
  ADJUST - the params were wrong but the step is still right. Reissue with corrected params. Do not edit "plan" or "done".
  REPLAN - the failure proves the remaining approach cannot work. Append "fail <cause in <=8 words>" to "done", discard the unexecuted tail of "plan".
  ABORT  - no alternative exists. Append the fail entry, then state="error".

Emit a single raw JSON object.
              "#, assistant_chat_message.content, error)
            }
          };

          let next_message = ChatMessage {
            role: ChatMessageRole::User,
            content: tool_output
          };

          if payload.len() - system_prompts.len() == request.len() {
            payload.push(next_message);
          } else {
            let last_index = payload.len() - 1;
            payload[last_index] = next_message;
          }
        }
      }
    }
  }
}
