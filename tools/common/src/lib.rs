pub mod tcp_socket;

pub use paste::paste;

#[macro_export]
macro_rules! define_tool {
  ($tool:ident, $params:ty, $description:expr, $permissions:expr, $env:expr, $execute:expr) => {
    ::wit_bindgen::generate!({
      world: "tool-world",
      path: "../tool.wit"
    });

    struct $tool;

    impl Guest for $tool {
      fn info() -> ToolInfo {
        let schema = ::schemars::schema_for!($params);
        let schema_json = match ::serde_json::to_string(&schema) {
          Ok(schema_json) => schema_json,
          Err(error) => {
            return ToolInfo {
              name: "Invalid tool! Do not use it".to_string(),
              description: error.to_string(),
              schema: "Invalid tool! Do not use it".to_string(),
              permissions: Vec::new(),
              env: Vec::new()
            };
          }
        };

        return ToolInfo {
          name: $crate::paste!(stringify!([< $tool:snake >])).to_string(),
          description: $description.to_string(),
          schema: schema_json,
          permissions: $permissions,
          env: $env
        };
      }

      fn run(params: String) -> ToolOutput {
        let input: $params = match ::serde_json::from_str(&params) {
          Ok(input) => input,
          Err(error) => {
            return ToolOutput {
              state: ToolOutputState::Error,
              content: format!("Invalid input received: {} {}", params, error)
            };
          }
        };

        return $execute(input);
      }
    }

    export!($tool);
  };
}
