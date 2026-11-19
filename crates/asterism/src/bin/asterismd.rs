use std::path::PathBuf;
use std::time::Duration;

use asterism_core::daemon::DaemonOptions;
use asterism_core::login_path::{merge_path, probe_login_path};
use asterism_core::paths::Paths;
use asterism_proto::types::Role;

fn main() {
    let role = Role::from_env();
    if role == Role::Machine {
        adopt_login_path();
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("asterismd: {e}");
            std::process::exit(1);
        }
    };
    let result = runtime.block_on(asterism_core::run_with_options(
        Paths::from_env(),
        DaemonOptions {
            role,
            ..Default::default()
        },
    ));
    if let Err(e) = result {
        eprintln!("asterismd: {e}");
        std::process::exit(1);
    }
}

/// Agent CLIs live on the login shell's PATH; must run before any thread starts, as `set_var` requires.
fn adopt_login_path() {
    let shell = std::env::var_os("SHELL").map_or_else(|| PathBuf::from("/bin/sh"), PathBuf::from);
    let Some(login) = probe_login_path(&shell, Duration::from_secs(5)) else {
        return;
    };
    let inherited = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", merge_path(&login, &inherited));
}
