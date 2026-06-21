use agenthouse::AppConfig;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "agenthouse=info".into()),
        )
        .init();

    agenthouse::run(AppConfig::default())
}
