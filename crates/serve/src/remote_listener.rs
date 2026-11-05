//! Reusable TLS listener. The caller is responsible for authenticating routes.
use crate::remote_access::TlsIdentity;
use axum::Router;
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder,
    service::TowerToHyperService,
};
use std::{
    collections::HashMap,
    future::Future,
    io,
    net::Shutdown,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpListener,
    task::JoinSet,
};
use tokio_rustls::{
    rustls::{
        self,
        pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
    },
    TlsAcceptor,
};

type Sockets = Arc<Mutex<HashMap<u64, std::net::TcpStream>>>;

// Hyper transfers this IO into an upgraded connection, so its guard survives
// the HTTP connection task and also covers WebSocket lifetimes.
struct Tracked<T> {
    inner: T,
    id: u64,
    sockets: Sockets,
}
impl<T> Drop for Tracked<T> {
    fn drop(&mut self) {
        self.sockets.lock().unwrap().remove(&self.id);
    }
}
impl<T: AsyncRead + Unpin> AsyncRead for Tracked<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}
impl<T: AsyncWrite + Unpin> AsyncWrite for Tracked<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// Serve an injected router until `shutdown` resolves. Shutdown closes every
/// accepted socket, including upgraded connections, before returning.
pub async fn serve<F: Future<Output = ()>>(
    listener: TcpListener,
    app: Router,
    identity: TlsIdentity,
    shutdown: F,
) -> anyhow::Result<()> {
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(identity.certificate_der)],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.private_key_der)),
        )?;
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let sockets: Sockets = Arc::default();
    let mut tasks = JoinSet::new();
    let mut next_id = 0u64;
    tokio::pin!(shutdown);
    let result = loop {
        tokio::select! {
            _ = &mut shutdown => break Ok(()),
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {},
            accepted = listener.accept() => {
                let (stream, _) = match accepted { Ok(v) => v, Err(e) => break Err(e.into()) };
                let result = (|| -> io::Result<_> {
                    crate::configure_accepted(&stream)?;
                    let stream = stream.into_std()?;
                    let control = stream.try_clone()?;
                    Ok((tokio::net::TcpStream::from_std(stream)?, control))
                })();
                let (stream, control) = match result { Ok(v) => v, Err(_) => continue };
                let id = next_id;
                next_id = next_id.wrapping_add(1);
                sockets.lock().unwrap().insert(id, control);
                let tracked = Tracked { inner: stream, id, sockets: sockets.clone() };
                let acceptor = acceptor.clone();
                let svc = TowerToHyperService::new(app.clone());
                tasks.spawn(async move {
                    if let Ok(Ok(stream)) = tokio::time::timeout(Duration::from_secs(5), acceptor.accept(tracked)).await {
                        let _ = Builder::new(TokioExecutor::new()).serve_connection_with_upgrades(TokioIo::new(stream), svc).await;
                    }
                });
            }
        }
    };
    for socket in sockets.lock().unwrap().values() {
        let _ = socket.shutdown(Shutdown::Both);
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::ws::WebSocketUpgrade, routing::get};
    use futures::StreamExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_rustls::{
        rustls::{pki_types::ServerName, ClientConfig, RootCertStore},
        TlsConnector,
    };

    fn identity() -> TlsIdentity {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        TlsIdentity {
            certificate_der: cert.cert.der().to_vec(),
            private_key_der: cert.signing_key.serialize_der(),
        }
    }
    fn connector(identity: &TlsIdentity) -> TlsConnector {
        let mut roots = RootCertStore::empty();
        roots
            .add(CertificateDer::from(identity.certificate_der.clone()))
            .unwrap();
        TlsConnector::from(Arc::new(
            ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        ))
    }
    async fn connect(
        addr: std::net::SocketAddr,
        identity: &TlsIdentity,
    ) -> io::Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>> {
        connector(identity)
            .connect(
                ServerName::try_from("localhost").unwrap(),
                tokio::net::TcpStream::connect(addr).await?,
            )
            .await
    }
    #[tokio::test]
    async fn tls_routes_trust_and_shutdown_upgrades() {
        let identity = identity();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/", get(|| async { "injected route" }))
            .route(
                "/ws",
                get(|ws: WebSocketUpgrade| async {
                    ws.on_upgrade(
                        |mut socket| async move { while socket.recv().await.is_some() {} },
                    )
                }),
            );
        let (stop, shutdown) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(serve(listener, app, identity.clone(), async {
            let _ = shutdown.await;
        }));
        let mut client = connect(addr, &identity).await.unwrap();
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut response = Vec::new();
        // TLS peers may close without close_notify after the HTTP response.
        let _ = client.read_to_end(&mut response).await;
        assert!(String::from_utf8_lossy(&response).contains("injected route"));
        assert!(connect(addr, &super::tests::identity()).await.is_err());
        let stream = connect(addr, &identity).await.unwrap();
        let (mut ws, _) = tokio_tungstenite::client_async("wss://localhost/ws", stream)
            .await
            .unwrap();
        stop.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let end = tokio::time::timeout(Duration::from_secs(2), ws.next())
            .await
            .unwrap();
        assert!(end.is_none() || end.unwrap().is_err());
        assert!(tokio::net::TcpStream::connect(addr).await.is_err());
    }
}
