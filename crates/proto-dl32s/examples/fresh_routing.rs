use openfader_core::MixerDriver;
use openfader_proto_dl32s::Dl32sDriver;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: fresh_routing HOST:PORT"))?;
    let driver = Dl32sDriver::new();
    driver.connect(&addr).await?;
    let result = driver.fresh_routing_snapshot(Duration::from_secs(20)).await;
    println!("{result:#?}");
    result.map(|_| ())
}
