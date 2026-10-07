//! Phase 3 bot door. One JSON action per line, one JSON result. No window, no model.

use std::io::{BufRead, Write};

use crate::editor::{parse_action, Action};

pub fn run() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(err) => {
                let _ = writeln!(stdout, "{}", error_result(&err.to_string()));
                let _ = stdout.flush();
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let _ = writeln!(stdout, "{}", reply(&line));
        let _ = stdout.flush();
    }
}

pub fn reply(line: &str) -> String {
    match parse_action(line) {
        Ok(action) => serde_json::json!({
            "ok": true,
            "action": action_name(&action),
        })
        .to_string(),
        Err(err) => error_result(&err),
    }
}

fn error_result(err: &str) -> String {
    serde_json::json!({"ok": false, "error": err}).to_string()
}

fn action_name(action: &Action) -> &'static str {
    match action {
        Action::SampleFrames { .. } => "sample_frames",
        Action::Propose { .. } => "propose",
        Action::SetCrop(_) => "set_crop",
        Action::SetFraming(_) => "set_framing",
        Action::SetCaptions { .. } => "set_captions",
        Action::SetHooks { .. } => "set_hooks",
        Action::SetScores(_) => "set_scores",
        Action::Offer { .. } => "offer",
        Action::Done => "done",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_90s_window_is_rejected_without_a_window() {
        let line = r#"{"action":"propose","moments":[{"start_s":0,"end_s":90,"why":"fake"}]}"#;
        let value: serde_json::Value = serde_json::from_str(&reply(line)).unwrap();
        assert_eq!(value["ok"], false);
        assert!(value["error"].as_str().unwrap().contains("60"));
    }

    #[test]
    fn a_done_line_is_accepted() {
        let value: serde_json::Value = serde_json::from_str(&reply(r#"{"action":"done"}"#)).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["action"], "done");
    }
}
