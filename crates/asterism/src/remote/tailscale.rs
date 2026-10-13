use std::path::{Path, PathBuf};
use std::time::Duration;

use std::process::Stdio;

use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::task::JoinHandle;

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
    let plain = |c: char| c.is_ascii_alphanumeric() || "/._-".contains(c);
    if s.is_empty() || !s.chars().all(plain) {
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

fn read_all<R: AsyncRead + Unpin + Send + 'static>(pipe: Option<R>) -> JoinHandle<Vec<u8>> {
    tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut buf).await;
        }
        buf
    })
}

async fn run(bin: &Path, args: &[String]) -> Result<Vec<u8>, String> {
    run_with_timeout(bin, args, TIMEOUT).await
}

async fn run_with_timeout(
    bin: &Path,
    args: &[String],
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let command_line = format!("tailscale {}", args.join(" "));
    let mut child = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("cannot run {}: {e}", bin.display()))?;
    let stdout = read_all(child.stdout.take());
    let stderr = read_all(child.stderr.take());
    let Ok(status) = tokio::time::timeout(timeout, child.wait()).await else {
        let _ = child.kill().await;
        let partial = tokio::time::timeout(Duration::from_secs(1), stderr)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        return Err(format!(
            "{command_line} timed out; Tailscale serve may need to be enabled for this tailnet in the Tailscale admin console. {}",
            String::from_utf8_lossy(&partial).trim()
        )
        .trim_end()
        .to_string());
    };
    let status = status.map_err(|e| format!("{command_line} failed: {e}"))?;
    let stderr = stderr.await.unwrap_or_default();
    if !status.success() {
        return Err(format!(
            "{command_line} failed ({status}): {}",
            String::from_utf8_lossy(&stderr).trim()
        ));
    }
    Ok(stdout.await.unwrap_or_default())
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
        assert_eq!(shell_word("/bin/ts$x"), "'/bin/ts$x'");
        assert_eq!(shell_word("it's"), r"'it'\''s'");
        assert_eq!(
            shell_word("/Applications/Tailscale.app/Contents/MacOS/Tailscale"),
            "/Applications/Tailscale.app/Contents/MacOS/Tailscale"
        );
    }

    #[tokio::test]
    async fn timeout_keeps_stderr_and_points_at_serve_setup() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("tailscale");
        std::fs::write(
            &bin,
            "#!/bin/sh\necho 'visit https://login.example' >&2\nexec sleep 5\n",
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let err = run_with_timeout(&bin, &["serve".into()], Duration::from_millis(500))
            .await
            .unwrap_err();
        assert!(err.contains("admin console"), "{err}");
        assert!(err.contains("visit https://login.example"), "{err}");
    }
}
