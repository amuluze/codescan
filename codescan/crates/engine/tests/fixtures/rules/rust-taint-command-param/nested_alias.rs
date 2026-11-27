use std::{process::Command as NestedCmd};

fn nested_alias(user_input: String) {
    // ruleid: rust/taint-command-param-alias
    // ruleid: rust/process-command-new
    NestedCmd::new(user_input);
}
