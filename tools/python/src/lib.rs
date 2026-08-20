use schemars::JsonSchema;

use rustpython_vm::Interpreter;
use rustpython_vm::AsObject;
use rustpython_vm::compiler::Mode;

use serde::Deserialize;

use common::define_tool;

const RESULT_GLOBAL: &str = "__OLIVIA__FINAL__RESULT__";

#[derive(Deserialize, JsonSchema)]
struct PythonParams {
  script: String
}

fn run(input: PythonParams) -> ToolOutput {
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

define_tool!(
  Python,
  PythonParams,
  "Executes Python 3 (CPython >= 3.14.0) scripts. CRITICAL: To return data from the script, you MUST assign the final output as a string to the global variable __OLIVIA__FINAL__RESULT__. Example: __OLIVIA__FINAL__RESULT__ = str(my_data). Do not use print() or return statements for the final output.",
  vec![Permission::FileSystem],
  run
);
