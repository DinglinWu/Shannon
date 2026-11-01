//! Preserve error classification across the private pipe.
use serde_json::{json, Value};
pub fn error_message(error: &Value) -> String {
    if let Some(message) = error.as_str() { return message.to_owned(); }
    let message = error["message"].as_str().unwrap_or("插件操作失败");
    if error["code"] == "outcome_unknown" { format!("outcome_unknown:{message}") }
    else { message.to_owned() }
}
pub fn error_value(message: &str) -> Value {
    match message.strip_prefix("outcome_unknown:") {
        Some(message) => json!({"code":"outcome_unknown","message":message}),
        None => json!({"code":"execution_failed","message":message}),
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn unknown_outcomes_survive_the_pipe() {
        let original=json!({"code":"outcome_unknown","message":"修改可能已经完成"});
        assert_eq!(error_value(&error_message(&original)),original);
        assert_eq!(error_message(&json!("旧版错误")),"旧版错误");
        assert_eq!(error_value("参数错误")["code"],"execution_failed");
    }
}
