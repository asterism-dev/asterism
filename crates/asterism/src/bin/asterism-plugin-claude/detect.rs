use std::path::Path;

use asterism_plugin::protocol::Detected;
use serde_json::Value;

/// Claude writes `<sessions_dir>/<pid>.json` for every running interactive process.
pub fn detect(sessions_dir: &Path, pids: &[u32]) -> Option<Detected> {
    pids.iter().find_map(|pid| {
        let raw = std::fs::read_to_string(sessions_dir.join(format!("{pid}.json"))).ok()?;
        let session: Value = serde_json::from_str(&raw).ok()?;
        Some(Detected {
            agent_ref: session["sessionId"].as_str()?.to_string(),
            idle: session["status"] == "idle",
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_claude_process_among_the_pids() {
        let dir = tempfile::tempdir().unwrap();
        let write = |pid: u32, body: &str| {
            std::fs::write(dir.path().join(format!("{pid}.json")), body).unwrap()
        };
        write(7, r#"{"pid":7,"sessionId":"s7","status":"busy"}"#);
        write(9, r#"{"pid":9,"sessionId":"s9","status":"idle"}"#);
        write(11, "not json");

        let found = |pids: &[u32]| detect(dir.path(), pids);
        assert_eq!(
            found(&[1, 9]),
            Some(Detected {
                agent_ref: "s9".into(),
                idle: true
            })
        );
        assert_eq!(
            found(&[7]),
            Some(Detected {
                agent_ref: "s7".into(),
                idle: false
            })
        );
        assert_eq!(found(&[1, 11]), None);
    }
}
