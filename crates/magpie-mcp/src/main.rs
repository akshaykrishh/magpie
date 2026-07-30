use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let db_path = magpie_core::default_db_path()
        .ok_or_else(|| anyhow::anyhow!("could not determine a data directory for this platform"))?;
    let store = Arc::new(magpie_core::Store::open(db_path)?);
    magpie_mcp::serve_stdio(store).await
}
