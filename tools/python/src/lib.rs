use schemars::JsonSchema;
use schemars::schema_for;

use rustpython_vm::Interpreter;
use rustpython_vm::AsObject;
use rustpython_vm::compiler::Mode;

use serde::Deserialize;

use serde_json::from_str;
use serde_json::to_string;

use wit_bindgen::generate;

generate!({
  world: "tool-world",
  path: "../tool.wit"
});

const RESULT_GLOBAL: &str = "__OLIVIA__FINAL__RESULT__";

#[derive(Deserialize, JsonSchema)]
struct PythonParams {
  script: String
}

struct PythonTool;

impl Guest for PythonTool {
  fn info() -> ToolInfo {
    let schema = schema_for!(PythonParams);
    let schema_json = match to_string(&schema) {
      Ok(schema_json) => schema_json,
      Err(error) => {
        return ToolInfo {
          name: "Invalid tool! Do not use it".to_string(),
          description: error.to_string(),
          schema: "Invalid tool! Do not use it".to_string()
        };
      }
    };

    return ToolInfo {
      name: "python".to_string(),
      description: "Executes Python 3 (CPython >= 3.14.0) scripts. CRITICAL: To return data from the script, you MUST assign the final output as a string to the global variable __OLIVIA__FINAL__RESULT__. Example: __OLIVIA__FINAL__RESULT__ = str(my_data). Do not use print() or return statements for the final output.".to_string(),
      schema: schema_json
    };
  }

  fn run(params: String) -> ToolOutput {
    let input: PythonParams = match from_str(&params) {
      Ok(input) => input,
      Err(error) => {
        return ToolOutput {
          state: ToolOutputState::Error,
          content: format!("Invalid input received: {} {}", params, error)
        };
      }
    };

    let interpreter = Interpreter::without_stdlib(Default::default());

    return interpreter.enter(|vm| {
      let scope = vm.new_scope_with_builtins();

      let code = match vm.compile(&input.script, Mode::Exec, "<olivia>".to_owned()) {
        Ok(code) => code,
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: format!("Python compile error: {}", error)
          };
        }
      };

      match vm.run_code_obj(code, scope.clone()) {
        Err(error) => {
          let message = match error.as_object().str(vm) {
            Ok(message) => message.to_string_lossy().into_owned(),
            Err(_) => "unrenderable Python exception".to_string()
          };

          return ToolOutput {
            state: ToolOutputState::Error,
            content: format!("Python runtime error: {}", message)
          };
        },
        _ => {}
      };

      let result = match scope.globals.get_item_opt(RESULT_GLOBAL, vm) {
        Ok(Some(result)) => result,
        Ok(None) => {
          return ToolOutput {
            state: ToolOutputState::Done,
            content: format!("The script did not set the {} global variable", RESULT_GLOBAL)
          };
        },
        Err(error) => {
          let message = match error.as_object().str(vm) {
            Ok(message) => message.to_string_lossy().into_owned(),
            Err(_) => "unrenderable Python exception".to_string()
          };

          return ToolOutput {
            state: ToolOutputState::Error,
            content: format!("Could not read {}: {}", RESULT_GLOBAL, message)
          };
        }
      };

      let content = match result.str(vm) {
        Ok(content) => content.to_string_lossy().into_owned(),
        Err(error) => {
          let message = match error.as_object().str(vm) {
            Ok(message) => message.to_string_lossy().into_owned(),
            Err(_) => "unrenderable Python exception".to_string()
          };

          return ToolOutput {
            state: ToolOutputState::Error,
            content: format!("{} could not be converted to a string: {}", RESULT_GLOBAL, message)
          };
        }
      };

      return ToolOutput {
        state: ToolOutputState::Done,
        content
      };
    });
  }
}

export!(PythonTool);
