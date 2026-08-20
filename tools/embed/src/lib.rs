use std::env::var;
use std::fs::read;
use std::fs::read_dir;

use std::cmp::min;

use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;

use schemars::JsonSchema;
use serde::Deserialize;

use serde_json::json;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use common::define_tool;
use common::litellm::Litellm;
use common::litellm::ChatMessage;
use common::qdrant::Qdrant;
use common::qdrant::Point;

// Shared working directory preopened for every tool. The folder to ingest lives
// under here; its files never pass through the model's context except as the
// base64 payload the extraction model reads.
const SANDBOX_PATH: &str = "/sandbox";

// Character-based chunking with a small overlap so context isn't lost at the
// boundaries.
const CHUNK_SIZE: usize = 1000;
const CHUNK_OVERLAP: usize = 100;

const EXTRACT_PROMPT: &str = "Extract all readable text from the attached document verbatim. Return only the extracted text, with no commentary, summaries, headings or explanation. If the document has no readable text, return an empty response.";

#[derive(Deserialize, JsonSchema)]
struct EmbedParams {
  /// The exact `endpoint` of the Qdrant data store, copied verbatim from the AVAILABLE DATA STORES section (e.g. http://qdrant:6333). It must include an explicit http:// or https:// scheme.
  endpoint: String,
  /// The exact `collection` name of the Qdrant data store, copied verbatim from the AVAILABLE DATA STORES section. It is created automatically if it does not exist yet.
  collection: String,
  /// The folder inside the /sandbox directory whose files should be ingested, e.g. docs. Every regular file in it is read, its text extracted by the extraction model (PDF, image or plain text), split into chunks, embedded and stored in the collection. Must not contain "..".
  folder: String
}

fn valid_folder(name: &str) -> bool {
  return name.contains("..") == false;
}

// Builds the file content block the extraction model reads, choosing the block
// shape from the file extension. Binary formats are sent as base64 data URLs;
// anything else is decoded as UTF-8 text and passed through directly.
fn content_block(filename: &str, bytes: &[u8]) -> serde_json::Value {
  let lower = filename.to_lowercase();
  let encoded = STANDARD.encode(bytes);

  if lower.ends_with(".pdf") {
    return json!({
      "type": "file",
      "file": {
        "filename": filename,
        "file_data": format!("data:application/pdf;base64,{}", encoded)
      }
    });
  }

  let image = match () {
    _ if lower.ends_with(".png") => Some("image/png"),
    _ if lower.ends_with(".jpg") || lower.ends_with(".jpeg") => Some("image/jpeg"),
    _ if lower.ends_with(".gif") => Some("image/gif"),
    _ if lower.ends_with(".webp") => Some("image/webp"),
    _ => None
  };

  match image {
    Some(mime) => {
      return json!({
        "type": "image_url",
        "image_url": {
          "url": format!("data:{};base64,{}", mime, encoded)
        }
      });
    },
    None => {
      return json!({
        "type": "text",
        "text": String::from_utf8_lossy(bytes).into_owned()
      });
    }
  }
}

fn chunk_text(text: &str) -> Vec<String> {
  let characters: Vec<char> = text.chars().collect();
  let mut chunks: Vec<String> = Vec::new();

  if characters.is_empty() {
    return chunks;
  }

  let mut start = 0;
  loop {
    if start >= characters.len() {
      break;
    }

    let end = min(start + CHUNK_SIZE, characters.len());
    let chunk: String = characters[start..end].iter().collect();
    let trimmed = chunk.trim().to_string();

    if trimmed.is_empty() == false {
      chunks.push(trimmed);
    }

    if end >= characters.len() {
      break;
    }

    start = end - CHUNK_OVERLAP;
  }

  return chunks;
}

fn point_id(source: &str, index: usize, text: &str) -> u64 {
  let mut hasher = DefaultHasher::new();
  source.hash(&mut hasher);
  index.hash(&mut hasher);
  text.hash(&mut hasher);

  return hasher.finish();
}

fn error(message: String) -> ToolOutput {
  return ToolOutput {
    state: ToolOutputState::Error,
    content: message
  };
}

fn run(input: EmbedParams) -> ToolOutput {
  println!("[embed] Enabling tool");

  if valid_folder(&input.folder) == false {
    return error(format!("Invalid folder {}: it must not contain '..'", input.folder));
  }

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

  let extract_model = match var("EXTRACT_MODEL") {
    Ok(extract_model) => extract_model,
    Err(error_value) => {
      return error(format!("Missing EXTRACT_MODEL env variable: {}", error_value));
    }
  };

  let litellm = Litellm::new(host.as_str(), master_key.as_str());
  let qdrant = Qdrant::new(input.endpoint.as_str());

  let directory = format!("{}/{}", SANDBOX_PATH, input.folder);
  let entries = match read_dir(&directory) {
    Ok(entries) => entries,
    Err(error_value) => {
      return error(format!("Unable to read the folder {}: {}", directory, error_value));
    }
  };

  let mut created = false;
  let mut total_files = 0;
  let mut total_chunks = 0;
  let mut skipped: Vec<String> = Vec::new();

  for entry in entries {
    let entry = match entry {
      Ok(entry) => entry,
      Err(error_value) => {
        return error(format!("Unable to read a folder entry in {}: {}", directory, error_value));
      }
    };

    let path = entry.path();
    if path.is_file() == false {
      continue;
    }

    let filename = entry.file_name().to_string_lossy().into_owned();

    let bytes = match read(&path) {
      Ok(bytes) => bytes,
      Err(error_value) => {
        return error(format!("Unable to read the file {}: {}", filename, error_value));
      }
    };

    // Ask the extraction model to read the file and return its text.
    let content = json!([
      content_block(&filename, &bytes),
      { "type": "text", "text": EXTRACT_PROMPT }
    ]);

    let messages = vec![
      ChatMessage {
        role: "user".to_string(),
        content
      }
    ];

    let text = match litellm.completions(extract_model.as_str(), messages) {
      Ok(text) => text,
      Err(error_value) => {
        return error(format!("Failed to extract text from {}: {}", filename, error_value));
      }
    };

    let chunks = chunk_text(&text);
    if chunks.is_empty() {
      skipped.push(format!("{} (no extractable text)", filename));

      continue;
    }

    let vectors = match litellm.embed(embed_model.as_str(), chunks.clone()) {
      Ok(vectors) => vectors,
      Err(error_value) => {
        return error(format!("Failed to embed {}: {}", filename, error_value));
      }
    };

    if created == false {
      let size = match vectors.first() {
        Some(first) => first.len() as u64,
        None => {
          return error(format!("The embedding model returned no vector for {}", filename));
        }
      };

      match qdrant.create_collection(input.collection.as_str(), size) {
        Ok(_) => {},
        Err(error_value) => {
          return error(format!("Unable to create the collection {}: {}", input.collection, error_value));
        }
      };

      created = true;
    }

    let mut points: Vec<Point> = Vec::new();
    let mut index = 0;
    for (chunk, vector) in chunks.into_iter().zip(vectors.into_iter()) {
      let id = point_id(filename.as_str(), index, chunk.as_str());
      let payload = json!({
        "text": chunk,
        "source": filename
      });

      points.push(Point {
        id,
        vector,
        payload
      });

      index = index + 1;
    }

    let count = points.len();
    match qdrant.upsert(input.collection.as_str(), points) {
      Ok(_) => {},
      Err(error_value) => {
        return error(format!("Unable to store {} into the collection: {}", filename, error_value));
      }
    };

    total_files = total_files + 1;
    total_chunks = total_chunks + count;
  }

  let mut content = format!("Indexed {} chunk(s) from {} file(s) into collection {}", total_chunks, total_files, input.collection);
  if skipped.is_empty() == false {
    content = format!("{}\nSkipped {} file(s): {}", content, skipped.len(), skipped.join("; "));
  }

  return ToolOutput {
    state: ToolOutputState::Done,
    content
  };
}

define_tool!(
  Embed,
  EmbedParams,
  "Ingests a whole folder of documents into a Qdrant collection so they can be retrieved later (the indexing half of a RAG pipeline). Point it at a folder inside /sandbox and it reads every regular file, extracts its text with the configured extraction model (PDFs and images are sent as documents; other files are read as text), splits the text into overlapping chunks, embeds them with the configured embedding model and stores each chunk together with its source file name in the collection, creating the collection automatically on first use. Pass the exact endpoint and collection from the AVAILABLE DATA STORES section plus the folder name. Files whose text cannot be extracted are skipped and reported. Use it to index documents the user has placed or downloaded in the sandbox; use the retrieve tool afterwards to search them.",
  vec![Permission::Network, Permission::FileSystem],
  run
);
