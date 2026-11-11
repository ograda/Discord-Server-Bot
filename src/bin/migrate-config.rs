fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--check") {
        let root = std::env::args().nth(2).unwrap_or_else(|| "config".into());
        let config = discord_server_bot::config::load(std::path::Path::new(&root))?;
        println!(
            "Configuração válida: {} guilda(s), {} habilitada(s).",
            config.guilds.len(),
            config.guilds.values().filter(|g| g.enabled).count()
        );
        return Ok(());
    }
    let root = std::env::args().nth(1).unwrap_or_else(|| "config".into());
    let templates = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "templates".into());
    let count = discord_server_bot::config::migration::migrate(
        std::path::Path::new(&root),
        std::path::Path::new(&templates),
    )?;
    println!(
        "{count} guilda(s) migrada(s); fontes e backup preservados; nenhum estado JSON alterado."
    );
    Ok(())
}
