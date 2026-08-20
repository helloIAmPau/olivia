use std::env::var;

use schemars::JsonSchema;
use serde::Deserialize;

use serde_json::Value;

use common::define_tool;
use common::litellm::Litellm;
use common::qdrant::Qdrant;

#[derive(Deserialize, JsonSchema)]
struct RetrieveParams {
  /// The exact `endpoint` of the Qdrant data store, copied verbatim from the AVAILABLE DATA STORES section (e.g. http://qdrant:6333). It must include an explicit http:// or https:// scheme.
  endpoint: String,
  /// The exact `collection` name of the Qdrant data store, copied verbatim from the AVAILABLE DATA STORES section. It must be the same collection the documents were indexed into with the embed tool.
  collection: String,
  /// The natural-language question or search phrase to find relevant document chunks for.
  query: String,
  /// How many of the most relevant chunks to return. Defaults to 5.
  limit: Option<u64>
}

fn error(message: String) -> ToolOutput {
  return ToolOutput {
    state: ToolOutputState::Error,
    content: message
  };
}

fn run(input: RetrieveParams) -> ToolOutput {
  println!("[retrieve] Enabling tool");

  let host = match var("LITELLM_HOST") {
    Ok(host) => host,
    Err(_) => "http://litellm:4000".to_string()
  };

  let master_key = match var("LITELLM_MASTER_KEY") {
    Ok(master_key) => master_key,
    Err(error_value) => {
      return error(format!("Missing LITELLM_MASTER_KEY env variable: {}", error_value));
    }
  };

  let embed_model = match var("EMBED_MODEL") {
    Ok(embed_model) => embed_model,
    Err(error_value) => {
      return error(format!("Missing EMBED_MODEL env variable: {}", error_value));
    }
  };

  let litellm = Litellm::new(host.as_str(), master_key.as_str());

  // Embed the question with the same model the documents were embedded with.
  let vectors = match litellm.embed(embed_model.as_str(), vec![input.query]) {
    Ok(vectors) => vectors,
    Err(error_value) => {
      return error(format!("Failed to embed the query: {}", error_value));
    }
  };

  let vector = match vectors.into_iter().next() {
    Some(vector) => vector,
    None => {
      return error("The embedding model returned no vector for the query".to_string());
    }
  };

  let limit = match input.limit {
    Some(limit) => limit,
    None => 5
  };

  let qdrant = Qdrant::new(input.endpoint.as_str());
  let matches = match qdrant.search(input.collection.as_str(), vector, limit) {
    Ok(matches) => matches,
    Err(error_value) => {
      return error(format!("Failed to search the collection: {}", error_value));
    }
  };

  if matches.is_empty() {
    return ToolOutput {
      state: ToolOutputState::Done,
      content: "No relevant documents were found".to_string()
    };
  }

  let mut blocks: Vec<String> = Vec::new();
  for hit in matches {
    let text = match hit.payload.get("text") {
      Some(Value::String(text)) => text.clone(),
      _ => "".to_string()
    };

    let source = match hit.payload.get("source") {
      Some(Value::String(source)) => source.clone(),
      _ => "unknown".to_string()
    };

    blocks.push(format!("[source: {} | score: {:.4}]\n{}", source, hit.score, text));
  }

  return ToolOutput {
    state: ToolOutputState::Done,
    content: format!("RETRIEVED CONTEXT:\n\n{}", blocks.join("\n---\n"))
  };
}

define_tool!(
  Retrieve,
  RetrieveParams,
  "Retrieves the document chunks most relevant to a question from a Qdrant collection (the retrieval half of a RAG pipeline). It embeds the query with the configured embedding model, runs a nearest-neighbour search over the collection and returns the top matching chunks with their source file name and similarity score. Pass the exact endpoint and collection from the AVAILABLE DATA STORES section (the same collection the embed tool indexed into) plus the user's question. Use the returned context to compose your final answer, grounding it in the retrieved chunks; if nothing relevant is found, say so rather than inventing an answer.",
  vec![Permission::Network],
  run
);
