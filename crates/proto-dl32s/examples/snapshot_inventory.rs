use openfader_core::MixerDriver;
use openfader_proto_dl32s::Dl32sDriver;
use std::time::Duration;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let driver = Dl32sDriver::new();
    let addr = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: snapshot_inventory HOST:PORT"))?;
    driver.connect(&addr).await?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(65);
    loop {
        let view = driver.native_snapshots().unwrap();
        if view.complete || view.error.is_some() || tokio::time::Instant::now() >= deadline {
            println!("{view:#?}");
            driver.disconnect().await?;
            anyhow::ensure!(view.complete, "catalogue unavailable: {:?}", view.error);
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
