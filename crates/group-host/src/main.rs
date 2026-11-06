//! `group-host`: the host, as its environment configures it.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(group_host::logs::Tee::default)
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    group_host::run(&args, group_host::config::Config::from_env()?).await
}
