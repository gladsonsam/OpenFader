//! Read-only endpoint identification without session initialization.

use super::{decode_one, Command, MackieSession, Message, MessageType, DEFAULT_PORT};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Identify a Mackie control endpoint without initializing a mixer session,
/// changing meter subscriptions, or requesting channel state. An open TCP
/// port alone is insufficient: require a correlated, framed device-name reply.
pub async fn identify(ip: std::net::IpAddr, budget: Duration) -> anyhow::Result<String> {
    tokio::time::timeout(budget, async {
        let mut stream = TcpStream::connect(SocketAddr::new(ip, DEFAULT_PORT)).await?;
        let request = Message::new(
            1,
            MessageType::Request,
            Command::GeneralInfo,
            3u32.to_be_bytes().to_vec(),
        )?;
        stream.write_all(&request.encode()).await?;
        let mut buf = Vec::new();
        loop {
            let mut chunk = [0u8; 2048];
            let n = stream.read(&mut chunk).await?;
            anyhow::ensure!(n > 0, "Mackie identification connection closed");
            buf.extend_from_slice(&chunk[..n]);
            anyhow::ensure!(buf.len() <= 65536, "Mackie identification reply too large");
            while let Some((reply, used)) = decode_one(&buf)? {
                buf.drain(..used);
                if reply.seq == 1
                    && reply.msg_type == MessageType::Response
                    && reply.command == Command::GeneralInfo
                    && reply.body.starts_with(&3u32.to_be_bytes())
                    && reply.body.len() >= 8
                {
                    return Ok(MackieSession::info_string(&reply.body));
                }
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("Mackie identification timed out"))?
}
