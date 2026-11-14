//! WING native metering transport (Remote Protocols 3.0.5-1, pp. 82–87).
//! OSC is used for control; the native TCP connection owns only channel 3.
use crate::session::keepalive_ticker;
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UdpSocket},
    sync::broadcast,
    task::{JoinHandle, JoinSet},
};

#[derive(Clone, Copy, Debug)]
pub struct WingMeterSpec {
    pub token: u8,
    pub index: u8,
}
impl WingMeterSpec {
    fn words(self) -> Option<usize> {
        match self.token {
            0xa0..=0xa4 => Some(8),
            0xa5 => Some(4),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct WingLevel {
    pub input: f32,
    pub post: f32,
}
#[derive(Clone, Debug)]
pub struct WingMeterSample {
    pub received: Instant,
    pub levels: Vec<WingLevel>,
}

/// Escape channel selectors in payload data, including UDP port/report bytes.
/// Always escaping DF as DF DE is accepted by the documented receive routine.
fn native_meter_frame(payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0xdf, 0xd3];
    for &b in payload {
        out.push(b);
        if b == 0xdf {
            out.push(0xde);
        }
    }
    out
}
pub fn meter_request(port: u16, report: u32, specs: &[WingMeterSpec]) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        !specs.is_empty() && specs.len() <= 128,
        "WING: invalid meter collection size"
    );
    let mut payload = vec![0xd3];
    payload.extend_from_slice(&port.to_be_bytes());
    payload.push(0xd4);
    payload.extend_from_slice(&report.to_be_bytes());
    payload.push(0xdc);
    for spec in specs {
        anyhow::ensure!(
            spec.words().is_some() && spec.index < 128,
            "WING: unsupported meter spec"
        );
        payload.extend_from_slice(&[spec.token, spec.index]);
    }
    payload.push(0xde);
    Ok(native_meter_frame(&payload))
}
pub fn decode_meters(data: &[u8], report: u32, specs: &[WingMeterSpec]) -> Option<Vec<WingLevel>> {
    if data.get(..4)? != report.to_be_bytes() {
        return None;
    }
    let expected = specs
        .iter()
        .try_fold(0usize, |total, spec| total.checked_add(spec.words()?))?;
    if data.len() != 4 + 2 * expected {
        return None;
    }
    let mut pos = 4;
    let mut out = Vec::with_capacity(specs.len());
    for spec in specs {
        let words = spec.words()?;
        let sample = |i: usize| {
            let offset = pos + i * 2;
            let db = i16::from_be_bytes([data[offset], data[offset + 1]]) as f32 / 256.;
            // +dBFS values remain finite and are clamped by display consumers.
            10f32.powf(db / 20.).min(1.)
        };
        out.push(WingLevel {
            input: sample(0).max(sample(1)),
            post: sample(2).max(sample(3)),
        });
        pos += words * 2;
    }
    Some(out)
}

pub struct WingMeterSession {
    latest: Arc<Mutex<Option<WingMeterSample>>>,
    tx: broadcast::Sender<WingMeterSample>,
    task: JoinHandle<()>,
}
impl WingMeterSession {
    pub async fn connect(peer: SocketAddr, specs: Vec<WingMeterSpec>) -> anyhow::Result<Self> {
        let bind = if peer.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        };
        let udp = UdpSocket::bind(bind).await?;
        let port = udp.local_addr()?.port();
        let report = 0x4f46_0000 | port as u32;
        // Keep each collection on its own control connection. This is valid
        // for desks that support several IDs per connection and also works
        // with peers that keep only one active report per TCP client.
        let mut collections = Vec::new();
        let mut start = 0;
        while start < specs.len() {
            let end = if specs.len() <= 16 {
                specs.len()
            } else {
                (start + 1..specs.len())
                    .find(|&i| i - start >= 16 || specs[i].token != specs[start].token)
                    .unwrap_or(specs.len())
            };
            let id = report ^ ((collections.len() as u32) << 16);
            collections.push((id, start..end));
            start = end;
        }
        anyhow::ensure!(!collections.is_empty(), "WING: empty meter collection");
        let mut connections = Vec::with_capacity(collections.len());
        for (id, range) in &collections {
            let request = meter_request(port, *id, &specs[range.clone()])?;
            let mut tcp =
                tokio::time::timeout(Duration::from_secs(2), TcpStream::connect(peer)).await??;
            tcp.set_nodelay(true)?;
            tokio::time::timeout(Duration::from_secs(1), tcp.write_all(&request)).await??;
            connections.push((tcp, request));
        }
        let latest = Arc::new(Mutex::new(None));
        let (tx, _) = broadcast::channel(64);
        let cache = latest.clone();
        let sender = tx.clone();
        let task = tokio::spawn(async move {
            // JoinSet owns the connections: aborting this session aborts every
            // worker, and loss of any one connection invalidates the aggregate.
            let mut workers = JoinSet::new();
            for (mut tcp, request) in connections {
                workers.spawn(async move {
                    let mut tick = keepalive_ticker(Duration::from_secs(2));
                    let mut control = [0u8; 1024];
                    loop {
                        tokio::select! {
                            _ = tick.tick() => {
                                if !matches!(tokio::time::timeout(Duration::from_secs(1), tcp.write_all(&request)).await, Ok(Ok(()))) { break; }
                            }
                            result = tcp.read(&mut control) => {
                                if !matches!(result, Ok(n) if n > 0) { break; }
                            }
                        }
                    }
                });
            }
            let mut data = [0u8; 8192];
            let mut combined = vec![WingLevel::default(); specs.len()];
            let mut received = vec![None::<Instant>; collections.len()];
            loop {
                tokio::select! {
                    _ = workers.join_next() => break,
                    packet = udp.recv_from(&mut data) => {
                        let Ok((n, from)) = packet else { break };
                        if from.ip() != peer.ip() { continue; }
                        let Some((index, (_, range), levels)) = collections.iter().enumerate().find_map(|(i, collection)| {
                            decode_meters(&data[..n], collection.0, &specs[collection.1.clone()]).map(|v| (i, collection, v))
                        }) else { continue };
                        let now = Instant::now();
                        combined[range.clone()].copy_from_slice(&levels);
                        received[index] = Some(now);
                        // Partial and stale sets carry no proof of the missing
                        // readings. Age the aggregate by its oldest component.
                        let oldest = received.iter().try_fold(now, |oldest, at| {
                            Some(oldest.min((*at)?))
                        });
                        let Some(oldest) = oldest.filter(|at| now.duration_since(*at) < Duration::from_secs(1)) else {
                            *cache.lock().unwrap() = None;
                            continue;
                        };
                        let sample = WingMeterSample { received: oldest, levels: combined.clone() };
                        *cache.lock().unwrap() = Some(sample.clone());
                        let _ = sender.send(sample);
                    }
                }
            }
            *cache.lock().unwrap() = None;
        });
        Ok(Self { latest, tx, task })
    }
    pub fn is_running(&self) -> bool {
        !self.task.is_finished()
    }
    pub fn latest(&self) -> Option<WingMeterSample> {
        self.latest
            .lock()
            .unwrap()
            .clone()
            .filter(|sample| sample.received.elapsed() < Duration::from_secs(1))
    }
    pub fn subscribe(&self) -> broadcast::Receiver<WingMeterSample> {
        self.tx.subscribe()
    }
}
impl Drop for WingMeterSession {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    const SPECS: &[WingMeterSpec] = &[
        WingMeterSpec {
            token: 0xa0,
            index: 0,
        },
        WingMeterSpec {
            token: 0xa5,
            index: 0,
        },
    ];
    fn packet(report: u32) -> Vec<u8> {
        let mut data = report.to_be_bytes().to_vec();
        for value in [
            -20i16 * 256,
            -10 * 256,
            -30 * 256,
            -40 * 256,
            0,
            0,
            0,
            0,
            -6 * 256,
            -12 * 256,
            -18 * 256,
            -24 * 256,
        ] {
            data.extend_from_slice(&value.to_be_bytes());
        }
        data
    }
    fn unframe(data: &[u8]) -> Vec<u8> {
        assert_eq!(&data[..2], &[0xdf, 0xd3]);
        let mut out = Vec::new();
        let mut i = 2;
        while i < data.len() {
            if data[i] == 0xdf {
                assert_eq!(data[i + 1], 0xde);
                out.push(0xdf);
                i += 2;
            } else {
                out.push(data[i]);
                i += 1;
            }
        }
        out
    }
    #[test]
    fn request_escapes_port_and_report_without_changing_native_channel() {
        let frame = meter_request(0xdfd1, 0xdfd30001, SPECS).unwrap();
        assert_eq!(
            unframe(&frame),
            vec![0xd3, 0xdf, 0xd1, 0xd4, 0xdf, 0xd3, 0, 1, 0xdc, 0xa0, 0, 0xa5, 0, 0xde]
        );
        assert!(meter_request(
            1,
            1,
            &[WingMeterSpec {
                token: 0xaa,
                index: 0
            }]
        )
        .is_err());
    }
    #[test]
    fn meter_lengths_ids_units_and_family_offsets_are_checked() {
        let data = packet(17);
        let levels = decode_meters(&data, 17, SPECS).unwrap();
        assert!((levels[0].input - 10f32.powf(-10. / 20.)).abs() < 0.00001);
        assert!((levels[0].post - 10f32.powf(-30. / 20.)).abs() < 0.00001);
        assert!((levels[1].input - 10f32.powf(-6. / 20.)).abs() < 0.00001);
        assert!((levels[1].post - 10f32.powf(-18. / 20.)).abs() < 0.00001);
        for n in 0..data.len() {
            assert!(decode_meters(&data[..n], 17, SPECS).is_none());
        }
        assert!(decode_meters(&data, 18, SPECS).is_none());
        let mut extra = data;
        extra.extend_from_slice(&[0, 0]);
        assert!(decode_meters(&extra, 17, SPECS).is_none());
    }
    #[tokio::test]
    async fn native_subscription_delivers_only_matching_reports_and_closes_on_drop() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let peer = listener.local_addr().unwrap();
        let (done, closed) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let (mut tcp, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 1024];
            let mut bytes = Vec::new();
            loop {
                let n = tcp.read(&mut request).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&request[..n]);
                if bytes.last() == Some(&0xde) {
                    break;
                }
            }
            let payload = unframe(&bytes);
            assert_eq!(payload[0], 0xd3);
            assert_eq!(payload[3], 0xd4);
            let port = u16::from_be_bytes(payload[1..3].try_into().unwrap());
            let report = u32::from_be_bytes(payload[4..8].try_into().unwrap());
            let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let target = SocketAddr::from(([127, 0, 0, 1], port));
            udp.send_to(&packet(report + 1), target).await.unwrap();
            udp.send_to(&packet(report)[..6], target).await.unwrap();
            udp.send_to(&packet(report), target).await.unwrap();
            loop {
                if tcp.read(&mut request).await.unwrap() == 0 {
                    break;
                }
            }
            let _ = done.send(());
        });
        let session = WingMeterSession::connect(peer, SPECS.to_vec())
            .await
            .unwrap();
        let mut rx = session.subscribe();
        let sample = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(sample.levels.len(), 2);
        assert_eq!(session.latest().unwrap().received, sample.received);
        drop(session);
        tokio::time::timeout(Duration::from_secs(2), closed)
            .await
            .unwrap()
            .unwrap();
        task.await.unwrap();
    }
}
