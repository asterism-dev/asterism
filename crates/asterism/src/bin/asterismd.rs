use asterism_core::paths::Paths;

#[tokio::main]
async fn main() {
    if let Err(e) = asterism_core::run(Paths::from_env()).await {
        eprintln!("asterismd: {e}");
        std::process::exit(1);
    }
}
