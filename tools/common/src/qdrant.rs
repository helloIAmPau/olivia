use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FormatterResult;

use serde::Serialize;
use serde::Deserialize;

use serde_json::Value;
use serde_json::from_slice;

use crate::http::HttpClient;
use crate::http::HttpError;

// A reusable client for the Qdrant vector database, built on the shared HTTP
// client and speaking its REST API. It creates collections, upserts points
// (an id, a vector and a JSON payload) and runs nearest-neighbour searches.

#[derive(Debug)]
pub enum QdrantError {
  Http(HttpError),
  Server(u16, String),
  Parse(String)
}

impl Display for QdrantError {
  fn fmt(&self, formatter: &mut Formatter) -> FormatterResult {
    return match self {
      QdrantError::Http(error) => write!(formatter, "{}", error),
      QdrantError::Server(status, body) => write!(formatter, "Qdrant returned status {}: {}", status, body),
      QdrantError::Parse(error) => write!(formatter, "Could not parse the Qdrant response: {}", error)
    };
  }
}

/// A point to store: a numeric id, its embedding vector and an arbitrary JSON
/// payload (e.g. the source text).
#[derive(Serialize)]
pub struct Point {
  pub id: u64,
  pub vector: Vec<f64>,
  pub payload: Value
}

/// A single search hit: its similarity score and the stored payload.
pub struct Match {
  pub score: f64,
  pub payload: Value
}

#[derive(Serialize)]
struct VectorsConfig {
  size: u64,
  distance: String
}

#[derive(Serialize)]
struct CreateBody {
  vectors: VectorsConfig
}

#[derive(Serialize)]
struct UpsertBody {
  points: Vec<Point>
}

#[derive(Serialize)]
struct SearchBody {
  vector: Vec<f64>,
  limit: u64,
  with_payload: bool
}

#[derive(Deserialize)]
struct SearchHit {
  score: f64,
  payload: Option<Value>
}

#[derive(Deserialize)]
struct SearchResponse {
  result: Vec<SearchHit>
}

pub struct Qdrant {
  client: HttpClient
}

impl Qdrant {
  pub fn new(endpoint: &str) -> Self {
    return Self {
      client: HttpClient::new(endpoint)
    };
  }

  /// Creates a collection with the given vector size and cosine distance. An
  /// already-existing collection is treated as success so ingestion is
  /// idempotent.
  pub fn create_collection(&self, collection: &str, size: u64) -> Result<(), QdrantError> {
    let path = format!("/collections/{}", collection);
    let body = CreateBody {
      vectors: VectorsConfig {
        size,
        distance: "Cosine".to_string()
      }
    };

    let response = match self.client.put(path.as_str(), vec![], vec![], Some(&body)) {
      Ok(response) => response,
      Err(error) => {
        return Err(QdrantError::Http(error));
      }
    };

    if response.is_success() {
      return Ok(());
    }

    let text = response.text();
    if text.contains("already exists") {
      return Ok(());
    }

    return Err(QdrantError::Server(response.status, text));
  }

  /// Inserts or updates the given points, waiting for the write to be applied.
  /// Returns how many points were sent.
  pub fn upsert(&self, collection: &str, points: Vec<Point>) -> Result<usize, QdrantError> {
    let count = points.len();
    let path = format!("/collections/{}/points", collection);
    let body = UpsertBody {
      points
    };

    let response = match self.client.put(path.as_str(), vec![], vec![("wait", "true")], Some(&body)) {
      Ok(response) => response,
      Err(error) => {
        return Err(QdrantError::Http(error));
      }
    };

    if response.is_success() == false {
      return Err(QdrantError::Server(response.status, response.text()));
    }

    return Ok(count);
  }

  /// Returns the `limit` nearest points to `vector`, with their payloads.
  pub fn search(&self, collection: &str, vector: Vec<f64>, limit: u64) -> Result<Vec<Match>, QdrantError> {
    let path = format!("/collections/{}/points/search", collection);
    let body = SearchBody {
      vector,
      limit,
      with_payload: true
    };

    let response = match self.client.post(path.as_str(), vec![], vec![], Some(&body)) {
      Ok(response) => response,
      Err(error) => {
        return Err(QdrantError::Http(error));
      }
    };

    if response.is_success() == false {
      return Err(QdrantError::Server(response.status, response.text()));
    }

    let parsed: SearchResponse = match from_slice(response.bytes()) {
      Ok(parsed) => parsed,
      Err(error) => {
        return Err(QdrantError::Parse(error.to_string()));
      }
    };

    let mut matches: Vec<Match> = Vec::new();
    for hit in parsed.result {
      let payload = match hit.payload {
        Some(payload) => payload,
        None => Value::Null
      };

      matches.push(Match {
        score: hit.score,
        payload
      });
    }

    return Ok(matches);
  }
}
