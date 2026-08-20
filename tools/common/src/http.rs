use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FormatterResult;

use serde::Serialize;
use serde_json::to_vec;

use waki::Client;

// A small, uniform HTTP client shared by every networked tool. It wraps `waki`
// so tools stop repeating the same "send, check the status, read the body"
// dance and so their error handling stays consistent. A client is built once
// with a base URL and then issues requests against paths under it. The body of
// post/put/delete is an optional `Serialize` value, encoded as JSON.

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

/// The outcome of a request: the HTTP status plus the raw response body.
pub struct Response {
  pub status: u16,
  body: Vec<u8>
}

impl Response {
  pub fn is_success(&self) -> bool {
    return self.status >= 200 && self.status < 300;
  }

  /// The raw response body bytes.
  pub fn bytes(&self) -> &[u8] {
    return &self.body;
  }

  /// The response body decoded as UTF-8 (lossily).
  pub fn text(&self) -> String {
    return String::from_utf8_lossy(&self.body).into_owned();
  }
}

pub struct HttpClient {
  base: String
}

impl HttpClient {
  /// Builds a client rooted at `base` (e.g. `http://litellm:4000`). Every
  /// request is issued against a path joined onto this base.
  pub fn new(base: &str) -> Self {
    return Self {
      base: base.trim_end_matches('/').to_string()
    };
  }

  pub fn get(&self, path: &str, headers: Vec<(&str, &str)>, query: Vec<(&str, &str)>) -> Result<Response, HttpError> {
    return self.send(Method::Get, path, headers, query, None);
  }

  pub fn post<T: Serialize>(&self, path: &str, headers: Vec<(&str, &str)>, query: Vec<(&str, &str)>, body: Option<&T>) -> Result<Response, HttpError> {
    return self.json(Method::Post, path, headers, query, body);
  }

  pub fn put<T: Serialize>(&self, path: &str, headers: Vec<(&str, &str)>, query: Vec<(&str, &str)>, body: Option<&T>) -> Result<Response, HttpError> {
    return self.json(Method::Put, path, headers, query, body);
  }

  pub fn delete<T: Serialize>(&self, path: &str, headers: Vec<(&str, &str)>, query: Vec<(&str, &str)>, body: Option<&T>) -> Result<Response, HttpError> {
    return self.json(Method::Delete, path, headers, query, body);
  }

  /// Like `post`, but sends the raw bytes verbatim: no JSON encoding and no
  /// Content-Type set. Add any Content-Type header yourself through `headers`.
  pub fn post_raw(&self, path: &str, headers: Vec<(&str, &str)>, query: Vec<(&str, &str)>, body: Vec<u8>) -> Result<Response, HttpError> {
    return self.send(Method::Post, path, headers, query, Some(body));
  }

  /// Like `put`, but sends the raw bytes verbatim: no JSON encoding and no
  /// Content-Type set. Add any Content-Type header yourself through `headers`.
  pub fn put_raw(&self, path: &str, headers: Vec<(&str, &str)>, query: Vec<(&str, &str)>, body: Vec<u8>) -> Result<Response, HttpError> {
    return self.send(Method::Put, path, headers, query, Some(body));
  }

  fn json<T: Serialize>(&self, method: Method, path: &str, mut headers: Vec<(&str, &str)>, query: Vec<(&str, &str)>, body: Option<&T>) -> Result<Response, HttpError> {
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

  fn send(&self, method: Method, path: &str, headers: Vec<(&str, &str)>, query: Vec<(&str, &str)>, body: Option<Vec<u8>>) -> Result<Response, HttpError> {
    let url = match path.is_empty() {
      true => self.base.clone(),
      false => format!("{}/{}", self.base, path.trim_start_matches('/'))
    };

    let client = Client::new();

    let mut request = match method {
      Method::Get => client.get(url.as_str()),
      Method::Post => client.post(url.as_str()),
      Method::Put => client.put(url.as_str()),
      Method::Delete => client.delete(url.as_str())
    };

    for (key, value) in headers {
      request = request.header(key, value);
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
