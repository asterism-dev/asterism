use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    // Daemon and app compare this to spot a daemon from another build with the same version.
    let build = git(&["describe", "--always", "--dirty"]).unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=ASTERISM_BUILD={build}");
    if let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        for file in ["HEAD", "index"] {
            println!("cargo:rerun-if-changed={git_dir}/{file}");
        }
        if let Some(head_ref) = git(&["symbolic-ref", "-q", "HEAD"]) {
            println!("cargo:rerun-if-changed={git_dir}/{head_ref}");
        }
    }
}
