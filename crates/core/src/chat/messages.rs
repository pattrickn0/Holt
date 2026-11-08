//! What the interface writes to the CLI's stdin: user messages, permission answers and interrupts.

use serde_json::{json, Value};

// ---------- What goes to stdin ----------

/// A Claude Code user message.
pub fn user_message_json(text: &str) -> String {
    json!({ "type": "user", "message": { "role": "user", "content": text } }).to_string()
}

/// An Antigravity user message.
pub fn agy_user_message_json(text: &str) -> String {
    json!({ "event": "user", "message": { "content": text } }).to_string()
}

/// The answer to a [`PermissionRequest`]: `input` is sent back unchanged when allowing, with `rules` (the request's
/// suggestions) when the user allowed it for the rest of the session.
pub fn permission_response_json(request_id: &str, allow: bool, input: &Value, deny_message: &str, rules: &[Value]) -> String {
    let response = if allow && !rules.is_empty() {
        json!({ "behavior": "allow", "updatedInput": input, "updatedPermissions": rules })
    } else if allow {
        json!({ "behavior": "allow", "updatedInput": input })
    } else {
        json!({ "behavior": "deny", "message": deny_message })
    };
    json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id, "response": response } }).to_string()
}

/// Cuts the current turn.
pub fn interrupt_json() -> String {
    json!({ "type": "control_request", "request_id": format!("holt-int-{}", crate::util::now_ms()), "request": { "subtype": "interrupt" } })
        .to_string()
}

pub(crate) fn error_response_json(request_id: &str) -> String {
    json!({ "type": "control_response", "response": { "subtype": "error", "request_id": request_id, "error": "not supported by Holt" } })
        .to_string()
}
