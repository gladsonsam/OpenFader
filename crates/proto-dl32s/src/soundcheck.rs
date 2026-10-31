//! Soundcheck eligibility is separate from the USB recording capability.
//!
//! Output patch descriptors identify USB send source selectors, but do not
//! establish the physical signal tap or a complete, independently verified
//! snapshot of those selectors and input link dependencies.

/// Why physical soundcheck routing cannot be offered by this driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnavailableReason {
    /// The DL32S USB patch and input routing require physical verification.
    UnverifiedSignalPath,
}

impl std::fmt::Display for UnavailableReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnverifiedSignalPath => write!(f, "DL32S soundcheck unavailable: USB send selector 5226–5257 source-to-signal mapping and Mic Pre tap position have not been physically verified; fresh complete USB patch and linked-input readback semantics are also unverified"),
        }
    }
}

impl std::error::Error for UnavailableReason {}

/// Refuse to create a physical route plan until its signal path is proven.
pub fn assess() -> Result<(), UnavailableReason> {
    Err(UnavailableReason::UnverifiedSignalPath)
}

/// One raw parameter and the transport sequence of its most recent receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingReceipt {
    pub pid: u32,
    pub value: u32,
    pub receipt_sequence: u64,
}

/// Two complete, matching fresh passes; this is not an atomic hardware snapshot.
#[derive(Debug, Clone)]
pub struct FreshRoutingSnapshot {
    pub generation: u64,
    /// Uninterpreted GeneralInfo 3 reply body, identical before and after.
    pub device_info: Vec<u8>,
    pub first_pass: Vec<RoutingReceipt>,
    pub second_pass: Vec<RoutingReceipt>,
}

impl crate::Dl32sDriver {
    /// Read two matching sets of desk-originated routing receipts. Each pass
    /// is bounded independently; an ACK or a previously cached value is not evidence.
    pub async fn fresh_routing_snapshot(
        &self,
        pass_timeout: std::time::Duration,
    ) -> anyhow::Result<FreshRoutingSnapshot> {
        use openfader_transport::mackie::REQUEST_TIMEOUT;
        use std::sync::atomic::Ordering;
        let guard = self.session().await?;
        let session = guard.as_ref().expect("session checked");
        let generation = self.generation.load(Ordering::Acquire);
        let check = || -> anyhow::Result<()> {
            anyhow::ensure!(
                session.is_alive(),
                "dl32s: transport closed during routing readback"
            );
            anyhow::ensure!(
                self.generation.load(Ordering::Acquire) == generation,
                "dl32s: generation changed during routing readback"
            );
            Ok(())
        };
        let info = session.request_general_info(3, REQUEST_TIMEOUT).await?.body;
        anyhow::ensure!(
            info.starts_with(&3u32.to_be_bytes()) && info.len() >= 8,
            "dl32s: invalid GeneralInfo 3 reply"
        );
        check()?;
        let mut addresses = Vec::new();
        for channel in 1..=crate::addrs::INPUT_COUNT {
            for field in ["source_a", "source_b", "source_select"] {
                addresses.push(crate::addrs::dl32s_inputs().address(channel, field)?);
            }
        }
        for slot in 1..=32 {
            addresses.push(crate::output_routing::usb_send_address(slot)?);
        }
        let cache = session.value_cache();
        let mut passes = Vec::new();
        for pass in 1..=2 {
            check()?;
            let deadline = tokio::time::Instant::now() + pass_timeout;
            let barrier = cache.receipt_barrier();
            let request = tokio::time::timeout_at(deadline, session.request_value_snapshot()).await;
            match request {
                Ok(Ok(())) => (),
                Ok(Err(error)) => {
                    return Err(error.context(format!("dl32s: routing pass {pass} request")))
                }
                Err(_) => (),
            }
            loop {
                check()?;
                match cache.received_since(&barrier, &addresses) {
                    Ok(values) => {
                        passes.push(
                            addresses
                                .iter()
                                .copied()
                                .zip(values)
                                .map(
                                    |(pid, (value, _revision, receipt_sequence))| RoutingReceipt {
                                        pid,
                                        value,
                                        receipt_sequence,
                                    },
                                )
                                .collect::<Vec<_>>(),
                        );
                        break;
                    }
                    Err(missing) if tokio::time::Instant::now() >= deadline => anyhow::bail!(
                        "dl32s: routing pass {pass} timed out; missing PIDs: {missing:?}"
                    ),
                    Err(_) => {
                        tokio::time::sleep_until(
                            (tokio::time::Instant::now() + std::time::Duration::from_millis(50))
                                .min(deadline),
                        )
                        .await
                    }
                }
            }
        }
        let after = session.request_general_info(3, REQUEST_TIMEOUT).await?.body;
        check()?;
        anyhow::ensure!(
            after == info,
            "dl32s: GeneralInfo 3 identity changed during routing readback"
        );
        anyhow::ensure!(
            passes[0]
                .iter()
                .zip(&passes[1])
                .all(|(a, b)| a.value == b.value),
            "dl32s: fresh routing passes differ"
        );
        let second_pass = passes.pop().expect("two passes");
        let first_pass = passes.pop().expect("two passes");
        Ok(FreshRoutingSnapshot {
            generation,
            device_info: info,
            first_pass,
            second_pass,
        })
    }
}
