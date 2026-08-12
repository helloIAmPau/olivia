use std::env::var;
use std::fs::write;

use schemars::JsonSchema;

use serde::Deserialize;
use serde::Serialize;

use waki::Client;

use common::define_tool;

// Shared working directory preopened for every tool; downloads are saved here.
const SANDBOX_PATH: &str = "/sandbox";

#[derive(Deserialize, JsonSchema)]
struct DownloadParams {
  /// The body of a JavaScript module that default-exports an async function
  /// receiving `{ page }` (a Puppeteer Page). It MUST trigger a real browser
  /// download event so browserless can capture the file. A plain
  /// `page.goto(fileUrl)` does NOT work: Chrome renders PDFs/images inline
  /// instead of downloading them. Instead, either click an existing download
  /// link on the page, or create an anchor with a `download` attribute and
  /// click it, then wait for the download to finish.
  /// Example (click a link):
  /// export default async function ({ page }) { await page.goto('https://example.com', { waitUntil: 'networkidle2' }); await page.click('a[href$=".pdf"]'); await new Promise(r => setTimeout(r, 5000)); }
  /// Example (force-download a URL):
  /// export default async function ({ page }) { await page.goto('https://example.com'); await page.evaluate(() => { const a = document.createElement('a'); a.href = 'https://example.com/report.pdf'; a.download = ''; document.body.appendChild(a); a.click(); }); await new Promise(r => setTimeout(r, 5000)); }
  code: String,
  /// The name to save the downloaded file under, inside the /sandbox directory
  /// (e.g. report.pdf). Must be a plain file name: no directory separators and
  /// no "..".
  filename: String
}

#[derive(Serialize)]
struct FunctionRequest {
  code: String
}

fn run(input: DownloadParams) -> ToolOutput {
  let token = match var("BROWSERLESS_TOKEN") {
    Ok(token) => token,
    Err(_) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: "BROWSERLESS_TOKEN not set".to_string()
      };
    }
  };

  let host = match var("BROWSERLESS_HOST") {
    Ok(host) => host,
    Err(_) => "http://browserless:3000".to_string()
  };

  // Keep the write confined to the sandbox: reject anything that could escape it.
  if input.filename.is_empty() || input.filename.contains('/') || input.filename.contains("..") {
    return ToolOutput {
      state: ToolOutputState::Error,
      content: format!("Invalid filename {}: it must be a plain file name with no '/' or '..'", input.filename)
    };
  }

  let endpoint = format!("{}/download", host);
  let payload = FunctionRequest {
    code: input.code
  };

  let response = match Client::new().post(endpoint.as_str()).query([("token", token), ("launch", "{\"stealth\":true}".to_string())]).json(&payload).send() {
    Ok(response) => response,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Request to browserless failed: {}", error)
      };
    }
  };

  let status = response.status_code();

  let bytes = match response.body() {
    Ok(bytes) => bytes,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Could not read the browserless response: {}", error)
      };
    }
  };

  if status < 200 || status >= 300 {
    let body = String::from_utf8_lossy(&bytes).into_owned();

    return ToolOutput {
      state: ToolOutputState::Error,
      content: format!("browserless /download returned status {}: {}", status, body)
    };
  }

  let path = format!("{}/{}", SANDBOX_PATH, input.filename);
  let size = bytes.len();

  println!("[download] Saving {} bytes to {}", size, path);

  match write(&path, &bytes) {
    Ok(_) => {},
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Unable to write the downloaded file to {}: {}", path, error)
      };
    }
  };

  return ToolOutput {
    state: ToolOutputState::Done,
    content: format!("Downloaded {} bytes to {}", size, path)
  };
}

define_tool!(
  Download,
  DownloadParams,
  "Downloads a file through the browserless /download API and saves it into the /sandbox directory. Provide the body of a JavaScript module that default-exports an async function receiving { page } (a Puppeteer Page) plus the filename to save it under in /sandbox. The function MUST trigger a real browser download event — click an existing download link, or create an anchor with a `download` attribute and click it — because a plain page.goto(fileUrl) does NOT download (Chrome renders PDFs/images inline). Browserless captures whatever file the page downloads and this tool writes its bytes to /sandbox/<filename>, returning the saved path. Use it to fetch documents, images, PDFs or other binary files — especially ones behind JavaScript, forms or auth — so other tools can read them from the sandbox afterwards.",
  run
);
