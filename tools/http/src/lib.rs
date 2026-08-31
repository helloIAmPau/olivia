use schemars::JsonSchema;

use serde::Deserialize;

use waki::Client;
use waki::header::HeaderName;

use common::define_tool;

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
enum HttpMethod {
  Get,
  Post,
  Put,
  Patch,
  Delete,
  Head
}

#[derive(Deserialize, JsonSchema)]
struct HttpHeader {
  /// The header name, e.g. Authorization or Content-Type.
  name: String,
  /// The header value, e.g. Bearer <token> or application/json.
  value: String
}

#[derive(Deserialize, JsonSchema)]
struct HttpParams {
  /// The HTTP method to use: GET, POST, PUT, PATCH, DELETE or HEAD.
  method: HttpMethod,
  /// The absolute URL to request, including the scheme (http:// or https://), e.g. https://api.example.com/v1/items?limit=10.
  url: String,
  /// Optional request headers as name/value pairs.
  headers: Option<Vec<HttpHeader>>,
  /// Optional request body, sent verbatim (e.g. a JSON string). Set the matching Content-Type header yourself. Ignored for GET and HEAD.
  body: Option<String>
}

fn label(method: &HttpMethod) -> &'static str {
  return match method {
    HttpMethod::Get => "GET",
    HttpMethod::Post => "POST",
    HttpMethod::Put => "PUT",
    HttpMethod::Patch => "PATCH",
    HttpMethod::Delete => "DELETE",
    HttpMethod::Head => "HEAD"
  };
}

fn run(input: HttpParams) -> ToolOutput {
  println!("[http] {} {}", label(&input.method), input.url);

  let client = Client::new();

  let mut builder = match input.method {
    HttpMethod::Get => client.get(input.url.as_str()),
    HttpMethod::Post => client.post(input.url.as_str()),
    HttpMethod::Put => client.put(input.url.as_str()),
    HttpMethod::Patch => client.patch(input.url.as_str()),
    HttpMethod::Delete => client.delete(input.url.as_str()),
    HttpMethod::Head => client.head(input.url.as_str())
  };

  let headers = match input.headers {
    Some(headers) => headers,
    None => Vec::new()
  };

  for header in headers {
    let name = match HeaderName::from_bytes(header.name.as_bytes()) {
      Ok(name) => name,
      Err(error) => {
        return ToolOutput {
          state: ToolOutputState::Error,
          content: format!("Invalid header name {}: {}", header.name, error)
        };
      }
    };

    builder = builder.header(name, header.value);
  }

  match input.body {
    Some(body) => {
      builder = builder.body(body.into_bytes());
    },
    None => {}
  };

  let response = match builder.send() {
    Ok(response) => response,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Request to {} failed: {}", input.url, error)
      };
    }
  };

  let status = response.status_code();

  let bytes = match response.body() {
    Ok(bytes) => bytes,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Could not read the response body: {}", error)
      };
    }
  };

  let body = String::from_utf8_lossy(&bytes).into_owned();

  return ToolOutput {
    state: ToolOutputState::Done,
    content: format!("HTTP {}\n\n{}", status, body)
  };
}

define_tool!(
  Http,
  HttpParams,
  "Makes a direct HTTP request to any URL and returns the response. Provide a method (GET, POST, PUT, PATCH, DELETE or HEAD), an absolute URL, optional request headers as name/value pairs, and an optional request body sent verbatim (set your own Content-Type when sending JSON). The result is the response status line followed by its body decoded as text. Any status code (including 4xx/5xx) is returned as a normal result so you can react to it; only network-level failures are errors. Use it to call REST/JSON APIs, webhooks and other plain HTTP endpoints; for pages that need a real browser (JavaScript, scraping, screenshots) use the web tool instead.",
  vec![Permission::Network],
  vec![],
  run
);
