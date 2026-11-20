use asterism_core::daemon::DaemonOptions;
use asterism_core::paths::Paths;
use asterism_proto::types::Role;

#[tokio::main]
async fn main() {
    if let Err(e) = asterism_core::run_with_options(
        Paths::from_env(),
        DaemonOptions {
            role: Role::from_env(),
            ..Default::default()
        },
    )
    .await
    {
        eprintln!("asterismd: {e}");
        std::process::exit(1);
    }
}
