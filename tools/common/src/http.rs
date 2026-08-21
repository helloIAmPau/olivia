use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FormatterResult;

use serde::Serialize;
use serde_json::to_vec;

use waki::Client;

#[derive(Debug)]
pub enum HttpError {
  Serialize(String),
  Request(String),
  Body(String)
}

impl Display for HttpError {
  fn fmt(&self, formatter: &mut Formatter) -> FormatterResult {
    return match self {
      HttpError::Serialize(error) => write!(formatter, "Serialize Error - could not encode the request body: {}", error),
      HttpError::Request(error) => write!(formatter, "Request Error - {}", error),
      HttpError::Body(error) => write!(formatter, "Body Error - could not read the response body: {}", error)
    };
  }
}

#[derive(Clone, Copy)]
enum Method {
  Get,
  Post,
  Put,
  Delete
}

fn to_pairs<K: Into<String>, V: Into<String>>(query: impl IntoIterator<Item = (K, V)>) -> Vec<(String, String)> {
  let mut pairs: Vec<(String, String)> = Vec::new();

  for (key, value) in query {
    pairs.push((key.into(), value.into()));
  }

  return pairs;
}

pub struct Response {
  pub status: u16,
  body: Vec<u8>
}

impl Response {
  pub fn is_success(&self) -> bool {
    return self.status >= 200 && self.status < 300;
  }

  pub fn bytes(&self) -> &[u8] {
    return &self.body;
  }

  pub fn text(&self) -> String {
    return String::from_utf8_lossy(&self.body).into_owned();
  }
}

pub struct HttpClient {
  base_url: String
}

impl HttpClient {
  pub fn new(base_url: String) -> Self {
    return Self {
      base_url: base_url.trim_end_matches('/').to_string()
    };
  }

  pub fn get<K: Into<String>, V: Into<String>>(&self, path: &str, headers: Vec<(&'static str, &str)>, query: impl IntoIterator<Item = (K, V)>) -> Result<Response, HttpError> {
    return self.send(Method::Get, path, headers, to_pairs(query), None);
  }

  pub fn post<T: Serialize, K: Into<String>, V: Into<String>>(&self, path: &str, headers: Vec<(&'static str, &str)>, query: impl IntoIterator<Item = (K, V)>, body: Option<&T>) -> Result<Response, HttpError> {
    return self.json(Method::Post, path, headers, to_pairs(query), body);
  }

  pub fn put<T: Serialize, K: Into<String>, V: Into<String>>(&self, path: &str, headers: Vec<(&'static str, &str)>, query: impl IntoIterator<Item = (K, V)>, body: Option<&T>) -> Result<Response, HttpError> {
    return self.json(Method::Put, path, headers, to_pairs(query), body);
  }

  pub fn delete<T: Serialize, K: Into<String>, V: Into<String>>(&self, path: &str, headers: Vec<(&'static str, &str)>, query: impl IntoIterator<Item = (K, V)>, body: Option<&T>) -> Result<Response, HttpError> {
    return self.json(Method::Delete, path, headers, to_pairs(query), body);
  }

  pub fn post_raw<K: Into<String>, V: Into<String>>(&self, path: &str, headers: Vec<(&'static str, &str)>, query: impl IntoIterator<Item = (K, V)>, body: Vec<u8>) -> Result<Response, HttpError> {
    return self.send(Method::Post, path, headers, to_pairs(query), Some(body));
  }

  pub fn put_raw<K: Into<String>, V: Into<String>>(&self, path: &str, headers: Vec<(&'static str, &str)>, query: impl IntoIterator<Item = (K, V)>, body: Vec<u8>) -> Result<Response, HttpError> {
    return self.send(Method::Put, path, headers, to_pairs(query), Some(body));
  }

  fn json<T: Serialize>(&self, method: Method, path: &str, mut headers: Vec<(&'static str, &str)>, query: Vec<(String, String)>, body: Option<&T>) -> Result<Response, HttpError> {
    let bytes = match body {
      Some(value) => {
        let encoded = match to_vec(value) {
          Ok(encoded) => encoded,
          Err(error) => {
            return Err(HttpError::Serialize(error.to_string()));
          }
        };

        headers.push(("Content-Type", "application/json"));

        Some(encoded)
      },
      None => None
    };

    return self.send(method, path, headers, query, bytes);
  }

  fn send(&self, method: Method, path: &str, headers: Vec<(&'static str, &str)>, query: Vec<(String, String)>, body: Option<Vec<u8>>) -> Result<Response, HttpError> {
    let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
    let client = Client::new();

    let mut request = match method {
      Method::Get => client.get(url.as_str()),
      Method::Post => client.post(url.as_str()),
      Method::Put => client.put(url.as_str()),
      Method::Delete => client.delete(url.as_str())
    };

    if headers.is_empty() == false {
      request = request.headers(headers);
    }

    if query.is_empty() == false {
      request = request.query(query);
    }

    match body {
      Some(bytes) => {
        request = request.body(bytes);
      },
      None => {}
    };

    let response = match request.send() {
      Ok(response) => response,
      Err(error) => {
        return Err(HttpError::Request(error.to_string()));
      }
    };

    let status = response.status_code();

    let body = match response.body() {
      Ok(body) => body,
      Err(error) => {
        return Err(HttpError::Body(error.to_string()));
      }
    };

    return Ok(Response {
      status,
      body
    });
  }
}
