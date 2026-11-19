use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use sha2::{Digest, Sha256};

const SSH_OPTIONS: &[&str] = &[
    "-T",
    "-o",
    "BatchMode=yes",
    "-o",
    "ConnectTimeout=10",
    "-o",
    "ServerAliveInterval=15",
    "-o",
    "ServerAliveCountMax=2",
];
const BINARIES: [&str; 2] = ["asterismd", "asterism"];
const INSTALL_DIR: &str = r#"d="$HOME/.asterism-machine/bin/$1""#;

pub fn default_ssh_bin() -> PathBuf {
    std::env::var_os("ASTERISM_SSH").map_or_else(|| "ssh".into(), PathBuf::from)
}

/// Inside `asterism.app` the per-platform machine binaries sit in `Contents/Resources/machine`.
pub fn default_bins_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("ASTERISM_MACHINE_BINS") {
        return Some(dir.into());
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?.join("Resources").join("machine");
    dir.is_dir().then_some(dir)
}

pub struct Ssh {
    bin: PathBuf,
    destination: String,
}

impl Ssh {
    pub fn new(bin: PathBuf, destination: String) -> Self {
        Self { bin, destination }
    }

    /// Runs `script` under `sh -c` on the machine with `args` as `$1…`, whatever the login shell is.
    pub fn command(&self, script: &str, args: &[&str]) -> tokio::process::Command {
        let mut remote = format!("sh -c {} sh", quote(script));
        for arg in args {
            remote.push(' ');
            remote.push_str(&quote(arg));
        }
        let mut cmd = tokio::process::Command::new(&self.bin);
        cmd.args(SSH_OPTIONS)
            .arg(&self.destination)
            .arg(remote)
            .kill_on_drop(true);
        cmd
    }
}

/// Single-quotes `word` for sh, bash, zsh and fish alike; fish treats `\` inside quotes as an escape, so it stays outside.
fn quote(word: &str) -> String {
    let mut out = String::from("'");
    for c in word.chars() {
        match c {
            '\'' => out.push_str(r"'\''"),
            '\\' => out.push_str(r"'\\'"),
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    LinuxX86_64,
    LinuxAarch64,
    DarwinAarch64,
    DarwinX86_64,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Platform::LinuxX86_64 => "linux-x86_64",
            Platform::LinuxAarch64 => "linux-aarch64",
            Platform::DarwinAarch64 => "darwin-aarch64",
            Platform::DarwinX86_64 => "darwin-x86_64",
        })
    }
}

#[derive(Debug)]
pub enum InstallError {
    /// `status` is the ssh exit code; 255 means ssh itself failed, not the remote command.
    Ssh {
        status: Option<i32>,
        line: String,
    },
    Unsupported(String),
    MissingBinaries(String),
    Checksum,
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstallError::Ssh { line, .. } => write!(f, "ssh failed: {line}"),
            InstallError::Unsupported(uname) => write!(f, "unsupported platform: {uname}"),
            InstallError::MissingBinaries(path) => write!(f, "missing machine binaries: {path}"),
            InstallError::Checksum => f.write_str("uploaded binaries failed the checksum"),
        }
    }
}

impl std::error::Error for InstallError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteInstall {
    pub key: String,
}

pub async fn probe(ssh: &Ssh) -> Result<Platform, InstallError> {
    let uname = run(ssh, "uname -sm", &[], None).await?;
    let uname = uname.trim();
    let mut words = uname.split_whitespace();
    let os = words.next().unwrap_or_default();
    let arch = match words.next().unwrap_or_default() {
        "arm64" => "aarch64",
        arch => arch,
    };
    match (os, arch) {
        ("Linux", "x86_64") => Ok(Platform::LinuxX86_64),
        ("Linux", "aarch64") => Ok(Platform::LinuxAarch64),
        ("Darwin", "aarch64") => Ok(Platform::DarwinAarch64),
        ("Darwin", "x86_64") => Ok(Platform::DarwinX86_64),
        _ => Err(InstallError::Unsupported(uname.to_string())),
    }
}

pub async fn ensure_installed(
    ssh: &Ssh,
    platform: &Platform,
    bins_dir: &Path,
) -> Result<RemoteInstall, InstallError> {
    let local = bins_dir.join(platform.to_string());
    let mut hashes = Vec::new();
    for name in BINARIES {
        let path = local.join(name);
        let hash = sha256_file(&path)
            .map_err(|e| InstallError::MissingBinaries(format!("{}: {e}", path.display())))?;
        hashes.push(hash);
    }
    let key = format!("{:x}", Sha256::digest(hashes.concat()))[..16].to_string();

    let check = format!(r#"{INSTALL_DIR}; test -f "$d/ok" && echo installed || true"#);
    if run(ssh, &check, &[&key], None).await?.trim() == "installed" {
        return Ok(RemoteInstall { key });
    }

    let upload = format!(
        r#"{INSTALL_DIR}; mkdir -p "$d" && cat > "$d/$2.part" && chmod 755 "$d/$2.part" && mv "$d/$2.part" "$d/$2""#
    );
    for name in BINARIES {
        let file = std::fs::File::open(local.join(name))
            .map_err(|e| InstallError::MissingBinaries(e.to_string()))?;
        run(ssh, &upload, &[&key, name], Some(file)).await?;
    }

    let verify = format!(
        r#"{INSTALL_DIR}; if command -v sha256sum >/dev/null 2>&1; then sha256sum "$d/asterismd" "$d/asterism"; else shasum -a 256 "$d/asterismd" "$d/asterism"; fi"#
    );
    let sums = run(ssh, &verify, &[&key], None).await?;
    let remote: Vec<&str> = sums
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    if remote != hashes {
        run(
            ssh,
            &format!(r#"{INSTALL_DIR}; rm -rf "$d""#),
            &[&key],
            None,
        )
        .await?;
        return Err(InstallError::Checksum);
    }
    // Written only after verification, so an interrupted upload never counts as installed.
    run(
        ssh,
        &format!(r#"{INSTALL_DIR}; touch "$d/ok""#),
        &[&key],
        None,
    )
    .await?;
    Ok(RemoteInstall { key })
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    std::io::copy(&mut std::fs::File::open(path)?, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

async fn run(
    ssh: &Ssh,
    script: &str,
    args: &[&str],
    stdin: Option<std::fs::File>,
) -> Result<String, InstallError> {
    let out = ssh
        .command(script, args)
        .stdin(stdin.map_or_else(Stdio::null, Stdio::from))
        .output()
        .await
        .map_err(|e| InstallError::Ssh {
            status: None,
            line: e.to_string(),
        })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let line = stderr
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .map_or_else(|| format!("ssh exited with {}", out.status), str::to_string);
        return Err(InstallError::Ssh {
            status: out.status.code(),
            line,
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::quote;

    #[test]
    fn quote_survives_quotes_backslashes_and_spaces() {
        let word = r#"it's a \ "path" $HOME"#;
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("printf %s {}", quote(word)))
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), word);
        assert_eq!(quote("a'b\\c"), r"'a'\''b'\\'c'");
    }
}
