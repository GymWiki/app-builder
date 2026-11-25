//! Parser voor `claude -p --output-format stream-json`. Onbekende regels worden genegeerd,
//! zodat nieuwe berichttypen in toekomstige CLI-versies niets stukmaken.

use super::ChatEvent;
use serde_json::Value;
use std::path::Path;

const MAX_RESULT: usize = 1500;
const MAX_DETAIL: usize = 2000;

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// Pad relatief aan het project tonen (en nooit meer dan nodig van de rest van het systeem).
pub fn display_path(path: &str, project_dir: &Path) -> String {
    let p = Path::new(path);
    p.strip_prefix(project_dir)
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string())
}

pub struct ToolInfo {
    pub kind: &'static str,
    pub summary: String,
    pub path: Option<String>,
    pub detail: Option<String>,
}

pub fn describe_tool(name: &str, input: &Value, project_dir: &Path) -> ToolInfo {
    let s = |k: &str| input.get(k).and_then(Value::as_str).map(str::to_string);
    let path = s("file_path").or_else(|| s("notebook_path")).or_else(|| s("path"));
    let rel = path.as_deref().map(|p| display_path(p, project_dir));
    match name {
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => ToolInfo {
            kind: "edit",
            summary: rel.clone().unwrap_or_default(),
            path: rel,
            detail: None,
        },
        "Read" | "Glob" | "Grep" | "LS" => ToolInfo {
            kind: "read",
            summary: rel.clone().or_else(|| s("pattern")).unwrap_or_default(),
            path: rel,
            detail: None,
        },
        "Bash" => {
            let cmd = s("command").unwrap_or_default();
            ToolInfo { kind: "command", summary: clip(&cmd, 120), path: None, detail: Some(clip(&cmd, MAX_DETAIL)) }
        }
        "WebFetch" | "WebSearch" => {
            let what = s("url").or_else(|| s("query")).unwrap_or_default();
            ToolInfo { kind: "network", summary: clip(&what, 120), path: None, detail: Some(clip(&what, MAX_DETAIL)) }
        }
        n if n.starts_with("mcp__studio__") => {
            ToolInfo { kind: "studio", summary: n.trim_start_matches("mcp__studio__").to_string(), path: None, detail: None }
        }
        n => ToolInfo { kind: "other", summary: n.to_string(), path: rel, detail: None },
    }
}

fn result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|i| i.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Zet één JSON-regel om in nul of meer genormaliseerde gebeurtenissen.
pub fn parse_line(line: &str, project_dir: &Path) -> Vec<ChatEvent> {
    let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { return vec![] };
    let kind = v.get("type").and_then(Value::as_str).unwrap_or("");
    // Berichten van subagents laten we buiten beeld; de hoofdgesprekslijn is genoeg.
    let is_sub = !v.get("parent_tool_use_id").map(Value::is_null).unwrap_or(true);
    match kind {
        "system" if v.get("subtype").and_then(Value::as_str) == Some("init") => {
            vec![ChatEvent::Init {
                session_id: v.get("session_id").and_then(Value::as_str).unwrap_or_default().to_string(),
                model: v.get("model").and_then(Value::as_str).map(str::to_string),
            }]
        }
        "stream_event" if !is_sub => {
            let ev = &v["event"];
            let delta = &ev["delta"];
            if ev["type"] == "content_block_delta" && delta["type"] == "text_delta" {
                let id = v.get("api_message_id").and_then(Value::as_str).unwrap_or("live").to_string();
                if let Some(t) = delta["text"].as_str() {
                    return vec![ChatEvent::TextDelta { message_id: id, text: t.to_string() }];
                }
            }
            vec![]
        }
        "assistant" if !is_sub => {
            let msg = &v["message"];
            let mid = msg["id"].as_str().unwrap_or("msg").to_string();
            let mut out = vec![];
            for block in msg["content"].as_array().into_iter().flatten() {
                match block["type"].as_str() {
                    Some("text") => {
                        let t = block["text"].as_str().unwrap_or("").trim();
                        if !t.is_empty() {
                            out.push(ChatEvent::Text { message_id: mid.clone(), text: t.to_string() });
                        }
                    }
                    Some("tool_use") => {
                        let name = block["name"].as_str().unwrap_or("tool");
                        let info = describe_tool(name, &block["input"], project_dir);
                        out.push(ChatEvent::ToolUse {
                            id: block["id"].as_str().unwrap_or_default().to_string(),
                            name: name.to_string(),
                            kind: info.kind.to_string(),
                            summary: info.summary,
                            path: info.path,
                            detail: info.detail,
                        });
                    }
                    _ => {}
                }
            }
            out
        }
        "user" if !is_sub => v["message"]["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|b| b["type"] == "tool_result")
            .map(|b| ChatEvent::ToolResult {
                tool_use_id: b["tool_use_id"].as_str().unwrap_or_default().to_string(),
                ok: !b["is_error"].as_bool().unwrap_or(false),
                text: clip(&result_text(&b["content"]), MAX_RESULT),
            })
            .collect(),
        "result" => {
            let text = v["result"].as_str().unwrap_or("").to_string();
            let ok = !v["is_error"].as_bool().unwrap_or(false) && v["subtype"].as_str().unwrap_or("success") == "success";
            let lower = text.to_lowercase();
            let code = (!ok).then(|| {
                if lower.contains("/login") || lower.contains("not logged in") || lower.contains("invalid api key") {
                    "not_logged_in"
                } else {
                    "failed"
                }
                .to_string()
            });
            vec![ChatEvent::Done { ok, text, duration_ms: v["duration_ms"].as_u64(), code }]
        }
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dir() -> PathBuf {
        PathBuf::from("/work/app")
    }

    #[test]
    fn parses_a_real_recorded_session() {
        let raw = include_str!("../../tests/fixtures/claude_stream.jsonl");
        let events: Vec<ChatEvent> = raw.lines().flat_map(|l| parse_line(l, &dir())).collect();
        assert!(matches!(events.first(), Some(ChatEvent::Init { session_id, .. }) if session_id.len() > 10));
        assert!(events.iter().any(|e| matches!(e, ChatEvent::ToolUse { name, kind, .. } if name == "Write" && kind == "edit")));
        assert!(events.iter().any(|e| matches!(e, ChatEvent::ToolResult { ok: true, .. })));
        assert!(events.iter().any(|e| matches!(e, ChatEvent::Text { text, .. } if text == "done")));
        assert!(events.iter().any(|e| matches!(e, ChatEvent::TextDelta { .. })));
        assert!(matches!(events.last(), Some(ChatEvent::Done { ok: true, text, code: None, .. }) if text == "done"));
    }

    #[test]
    fn shows_paths_relative_to_the_project() {
        let line = r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":"/work/app/lib/main.dart","old_string":"a","new_string":"b"}}]}}"#;
        match &parse_line(line, &dir())[0] {
            ChatEvent::ToolUse { summary, path, detail, .. } => {
                assert_eq!(summary, "lib/main.dart");
                assert_eq!(path.as_deref(), Some("lib/main.dart"));
                assert!(detail.is_none(), "file contents must not be copied into the chat");
            }
            e => panic!("unexpected {e:?}"),
        }
    }

    #[test]
    fn classifies_tools() {
        let cases = [
            ("Bash", r#"{"command":"flutter analyze"}"#, "command"),
            ("WebFetch", r#"{"url":"https://example.com"}"#, "network"),
            ("Grep", r#"{"pattern":"foo"}"#, "read"),
            ("mcp__studio__create_snapshot", "{}", "studio"),
            ("TodoWrite", "{}", "other"),
        ];
        for (name, input, kind) in cases {
            let v: Value = serde_json::from_str(input).unwrap();
            assert_eq!(describe_tool(name, &v, &dir()).kind, kind, "{name}");
        }
    }

    #[test]
    fn tool_errors_and_array_content() {
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":[{"type":"text","text":"nope"}]}]}}"#;
        assert_eq!(
            parse_line(line, &dir()),
            vec![ChatEvent::ToolResult { tool_use_id: "t1".into(), ok: false, text: "nope".into() }]
        );
    }

    #[test]
    fn detects_login_problems_and_other_failures() {
        let login = r#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in · Please run /login"}"#;
        assert!(matches!(&parse_line(login, &dir())[0], ChatEvent::Done { ok: false, code: Some(c), .. } if c == "not_logged_in"));
        let other = r#"{"type":"result","subtype":"error_max_turns","is_error":false,"result":""}"#;
        assert!(matches!(&parse_line(other, &dir())[0], ChatEvent::Done { ok: false, code: Some(c), .. } if c == "failed"));
    }

    #[test]
    fn ignores_noise_subagents_and_garbage() {
        assert!(parse_line("not json", &dir()).is_empty());
        assert!(parse_line(r#"{"type":"rate_limit_event"}"#, &dir()).is_empty());
        assert!(parse_line(r#"{"type":"system","subtype":"hook_started"}"#, &dir()).is_empty());
        let sub = r#"{"type":"assistant","parent_tool_use_id":"x","message":{"id":"m","content":[{"type":"text","text":"hi"}]}}"#;
        assert!(parse_line(sub, &dir()).is_empty());
    }

    #[test]
    fn long_results_are_clipped() {
        let big = "x".repeat(5000);
        let line = format!(r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"t","content":"{big}"}}]}}}}"#);
        match &parse_line(&line, &dir())[0] {
            ChatEvent::ToolResult { text, .. } => assert!(text.chars().count() <= MAX_RESULT + 1),
            e => panic!("{e:?}"),
        }
    }
}
