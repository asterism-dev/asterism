use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use tokio::process::Command;

const APP_BUNDLE: &str = "/Applications/Tailscale.app/Contents/MacOS/Tailscale";
const TIMEOUT: Duration = Duration::from_secs(30);
pub const CERTS_DISABLED: &str =
    "HTTPS certificates are not enabled for this tailnet (Tailscale admin console → DNS → HTTPS Certificates)";

/// `ASTERISM_TAILSCALE`, then `tailscale` on PATH, then the macOS app bundle, whose CLI is not on PATH.
pub fn default_bin() -> PathBuf {
    if let Some(bin) = std::env::var_os("ASTERISM_TAILSCALE") {
        return bin.into();
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|dir| dir.join("tailscale"))
        .find(|bin| bin.is_file())
        .unwrap_or_else(|| APP_BUNDLE.into())
}

fn serve_on_args(https_port: u16, local_port: u16) -> Vec<String> {
    vec![
        "serve".into(),
        "--bg".into(),
        format!("--https={https_port}"),
        format!("http://127.0.0.1:{local_port}"),
    ]
}

fn shell_word(s: &str) -> String {
    if s.contains(|c: char| c.is_whitespace() || c == '\'') {
        format!("'{}'", s.replace('\'', r"'\''"))
    } else {
        s.to_string()
    }
}

pub fn serve_command(bin: &Path, https_port: u16, local_port: u16) -> String {
    std::iter::once(shell_word(&bin.to_string_lossy()))
        .chain(serve_on_args(https_port, local_port))
        .collect::<Vec<_>>()
        .join(" ")
}

async fn run(bin: &Path, args: &[String]) -> Result<Vec<u8>, String> {
    let mut command = Command::new(bin);
    command.args(args).kill_on_drop(true);
    let output = tokio::time::timeout(TIMEOUT, command.output())
        .await
        .map_err(|_| format!("tailscale {} timed out", args.join(" ")))?
        .map_err(|e| format!("cannot run {}: {e}", bin.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "tailscale {} failed ({}): {}",
            args.join(" "),
            output.status,
            stderr.trim()
        ));
    }
    Ok(output.stdout)
}

pub async fn serve_on(bin: &Path, https_port: u16, local_port: u16) -> Result<(), String> {
    run(bin, &serve_on_args(https_port, local_port))
        .await
        .map(drop)
}

pub async fn serve_off(bin: &Path, https_port: u16) -> Result<(), String> {
    let args = [
        "serve".into(),
        format!("--https={https_port}"),
        "off".into(),
    ];
    run(bin, &args).await.map(drop)
}

/// The HTTPS base URL phones open, from this node's MagicDNS name.
pub async fn public_url(bin: &Path, https_port: u16) -> Result<String, String> {
    let out = run(bin, &["status".into(), "--json".into()]).await?;
    let status: Value =
        serde_json::from_slice(&out).map_err(|e| format!("cannot read tailscale status: {e}"))?;
    parse_public_url(&status, https_port)
}

fn parse_public_url(status: &Value, https_port: u16) -> Result<String, String> {
    let dns = status["Self"]["DNSName"]
        .as_str()
        .map(|name| name.trim_end_matches('.'))
        .filter(|name| !name.is_empty())
        .ok_or("Tailscale reports no DNS name for this Mac; is it logged in?")?;
    if status["CertDomains"].as_array().is_none_or(Vec::is_empty) {
        return Err(CERTS_DISABLED.into());
    }
    Ok(match https_port {
        443 => format!("https://{dns}"),
        port => format!("https://{dns}:{port}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn public_url_omits_the_default_port() {
        let status = json!({"Self": {"DNSName": "mac.example.ts.net."}, "CertDomains": ["mac.example.ts.net"]});
        assert_eq!(
            parse_public_url(&status, 443).unwrap(),
            "https://mac.example.ts.net"
        );
        assert_eq!(
            parse_public_url(&status, 8443).unwrap(),
            "https://mac.example.ts.net:8443"
        );
    }

    #[test]
    fn command_quotes_paths_with_spaces() {
        assert_eq!(
            serve_command(Path::new("/a b/tailscale"), 8443, 7420),
            "'/a b/tailscale' serve --bg --https=8443 http://127.0.0.1:7420"
        );
    }
}
