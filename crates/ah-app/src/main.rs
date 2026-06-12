use ah_app::AppConfig;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ah_app=info".into()),
        )
        .init();

    ah_app::run(AppConfig::default())
}
