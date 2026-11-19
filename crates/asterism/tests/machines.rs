mod common;

use asterism_core::machines::install::{self, InstallError};
use common::{host_platform, FakeSsh};

#[tokio::test]
async fn probe_reports_host_platform() {
    let fake = FakeSsh::new();
    let platform = install::probe(&fake.ssh("vm")).await.unwrap();
    assert_eq!(platform.to_string(), host_platform());
}

#[tokio::test]
async fn install_uploads_binaries_once_into_a_home_with_spaces() {
    let fake = FakeSsh::new();
    assert!(fake.home().to_string_lossy().contains(' '));
    let ssh = fake.ssh("vm");
    let platform = install::probe(&ssh).await.unwrap();
    let first = install::ensure_installed(&ssh, &platform, &fake.bins_dir())
        .await
        .unwrap();
    let dir = fake.home().join(".asterism-machine/bin").join(&first.key);
    assert!(dir.join("asterismd").is_file() && dir.join("asterism").is_file());
    let mtime = std::fs::metadata(dir.join("asterismd"))
        .unwrap()
        .modified()
        .unwrap();
    let second = install::ensure_installed(&ssh, &platform, &fake.bins_dir())
        .await
        .unwrap();
    assert_eq!(first.key, second.key);
    assert_eq!(
        mtime,
        std::fs::metadata(dir.join("asterismd"))
            .unwrap()
            .modified()
            .unwrap()
    );
}

#[tokio::test]
async fn install_fails_without_binaries_for_the_platform() {
    let fake = FakeSsh::new();
    let empty = tempfile::tempdir().unwrap();
    let ssh = fake.ssh("vm");
    let platform = install::probe(&ssh).await.unwrap();
    let err = install::ensure_installed(&ssh, &platform, empty.path())
        .await
        .unwrap_err();
    assert!(matches!(err, InstallError::MissingBinaries(_)));
}

#[tokio::test]
async fn ssh_failure_reports_the_last_stderr_line() {
    let fake = FakeSsh::new();
    fake.fail_with("Permission denied (publickey).", 255);
    let err = install::probe(&fake.ssh("vm")).await.unwrap_err();
    assert!(matches!(err, InstallError::Ssh(ref line) if line == "Permission denied (publickey)."));
}
