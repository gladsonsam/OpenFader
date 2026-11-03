//! Remote pairing and TLS identity. This module does not authenticate local
//! administrators or accept the desktop master credential.
//!
//! Storage must protect the private key from other users and atomically
//! initialize an identity across processes. Client credentials are memory-only:
//! restarting the service requires clients to pair again. A listener must use
//! TLS, authenticate every remote request, and close active connections when a
//! client is revoked or remote access is disabled.

use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;

/// DER-encoded certificate and PKCS#8 private key. Never log this value.
#[derive(Clone)]
pub struct TlsIdentity {
    pub certificate_der: Vec<u8>,
    pub private_key_der: Vec<u8>,
}

/// The implementation must keep the key private and atomically return the same
/// identity to concurrent initializers. It must propagate storage failures.
pub trait IdentityStorage {
    fn load_or_create(
        &self,
        create: &mut dyn FnMut() -> anyhow::Result<TlsIdentity>,
    ) -> anyhow::Result<TlsIdentity>;
}

/// A bearer credential returned exactly once to a newly paired client.
pub struct ClientCredential {
    pub client_id: String,
    pub bearer_token: String,
}

struct Pending {
    secret: String,
    expires: Instant,
}
#[derive(Default)]
struct State {
    enabled: bool,
    pending: Option<Pending>,
    clients: HashMap<String, Client>,
}

struct Client {
    token: String,
    active: tokio::sync::watch::Sender<bool>,
}

pub struct RemoteAccess {
    identity: TlsIdentity,
    state: Mutex<State>,
}

fn random_hex(bytes: usize) -> anyhow::Result<String> {
    let mut data = vec![0; bytes];
    getrandom::getrandom(&mut data)
        .map_err(|e| anyhow::anyhow!("OS randomness unavailable: {e}"))?;
    Ok(hex(&data))
}
fn hex(data: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(data.len() * 2);
    for byte in data {
        write!(out, "{byte:02x}").expect("writing to String cannot fail");
    }
    out
}
fn matches_secret(a: &str, b: &str) -> bool {
    a.len() == b.len() && bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}
impl RemoteAccess {
    pub fn new(storage: &impl IdentityStorage) -> anyhow::Result<Self> {
        let identity = storage.load_or_create(&mut || {
            let generated = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
            Ok(TlsIdentity {
                certificate_der: generated.cert.der().to_vec(),
                private_key_der: generated.key_pair.serialize_der(),
            })
        })?;
        Ok(Self {
            identity,
            state: Mutex::new(State::default()),
        })
    }
    pub fn tls_identity(&self) -> &TlsIdentity {
        &self.identity
    }
    /// Lowercase hexadecimal SHA-256 of the certificate DER.
    pub fn fingerprint(&self) -> String {
        hex(&Sha256::digest(&self.identity.certificate_der))
    }
    pub fn is_enabled(&self) -> bool {
        self.state.lock().unwrap().enabled
    }
    /// Disabling revokes all clients and invalidates any pending pairing.
    pub fn set_enabled(&self, enabled: bool) {
        let mut state = self.state.lock().unwrap();
        state.enabled = enabled;
        if !enabled {
            state.pending = None;
            for client in state.clients.values() {
                client.active.send_replace(false);
            }
            state.clients.clear();
        }
    }
    /// Replaces the pending secret. Zero duration is rejected.
    pub fn begin_pairing(&self, lifetime: Duration) -> anyhow::Result<String> {
        let mut state = self.state.lock().unwrap();
        anyhow::ensure!(state.enabled, "remote access disabled");
        anyhow::ensure!(!lifetime.is_zero(), "pairing lifetime must be positive");
        let expires = Instant::now()
            .checked_add(lifetime)
            .ok_or_else(|| anyhow::anyhow!("pairing lifetime too long"))?;
        let secret = random_hex(32)?;
        state.pending = Some(Pending {
            secret: secret.clone(),
            expires,
        });
        Ok(secret)
    }
    /// A successful exchange consumes the secret atomically.
    pub fn exchange(&self, secret: &str) -> anyhow::Result<ClientCredential> {
        let mut state = self.state.lock().unwrap();
        anyhow::ensure!(state.enabled, "remote access disabled");
        let valid = state
            .pending
            .as_ref()
            .is_some_and(|p| Instant::now() < p.expires && matches_secret(&p.secret, secret));
        anyhow::ensure!(valid, "invalid or expired pairing secret");
        let client_id = random_hex(16)?;
        let bearer_token = random_hex(32)?;
        state.pending = None;
        let (active, _) = tokio::sync::watch::channel(true);
        state.clients.insert(
            client_id.clone(),
            Client {
                token: bearer_token.clone(),
                active,
            },
        );
        Ok(ClientCredential {
            client_id,
            bearer_token,
        })
    }
    /// Returns the client ID for a valid remote bearer token.
    pub fn authenticate(&self, token: &str) -> Option<String> {
        let state = self.state.lock().unwrap();
        if !state.enabled {
            return None;
        }
        state
            .clients
            .iter()
            .find(|(_, candidate)| matches_secret(&candidate.token, token))
            .map(|(id, _)| id.clone())
    }
    /// Atomically authenticate and subscribe to revocation or disable.
    /// A connection must terminate when the receiver changes to false.
    pub fn authenticate_connection(
        &self,
        token: &str,
    ) -> Option<(String, tokio::sync::watch::Receiver<bool>)> {
        let state = self.state.lock().unwrap();
        if !state.enabled {
            return None;
        }
        state
            .clients
            .iter()
            .find(|(_, client)| matches_secret(&client.token, token))
            .map(|(id, client)| (id.clone(), client.active.subscribe()))
    }
    pub fn clients(&self) -> Vec<String> {
        let mut clients: Vec<_> = self.state.lock().unwrap().clients.keys().cloned().collect();
        clients.sort();
        clients
    }
    pub fn revoke(&self, client_id: &str) -> bool {
        let mut state = self.state.lock().unwrap();
        if let Some(client) = state.clients.remove(client_id) {
            client.active.send_replace(false);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[derive(Default)]
    struct Storage(Mutex<Option<TlsIdentity>>);
    impl IdentityStorage for Storage {
        fn load_or_create(
            &self,
            create: &mut dyn FnMut() -> anyhow::Result<TlsIdentity>,
        ) -> anyhow::Result<TlsIdentity> {
            let mut value = self.0.lock().unwrap();
            if value.is_none() {
                *value = Some(create()?);
            }
            Ok(value.as_ref().unwrap().clone())
        }
    }
    #[test]
    fn pairing_revoke_disable_and_reload() {
        let storage = Storage::default();
        let service = RemoteAccess::new(&storage).unwrap();
        assert_eq!(
            service.fingerprint(),
            RemoteAccess::new(&storage).unwrap().fingerprint()
        );
        assert!(!service.is_enabled());
        assert!(service.begin_pairing(Duration::from_secs(1)).is_err());
        service.set_enabled(true);
        let secret = service.begin_pairing(Duration::from_secs(10)).unwrap();
        let client = service.exchange(&secret).unwrap();
        assert!(service.exchange(&secret).is_err());
        assert_eq!(
            service.authenticate(&client.bearer_token),
            Some(client.client_id.clone())
        );
        assert_eq!(service.clients().len(), 1);
        assert!(service.revoke(&client.client_id));
        assert!(service.authenticate(&client.bearer_token).is_none());
        let secret = service.begin_pairing(Duration::from_secs(10)).unwrap();
        service.set_enabled(false);
        service.set_enabled(true);
        assert!(service.exchange(&secret).is_err());
    }
    #[tokio::test]
    async fn connection_subscription_closes_on_revoke_and_disable() {
        let service = RemoteAccess::new(&Storage::default()).unwrap();
        service.set_enabled(true);
        let secret = service.begin_pairing(Duration::from_secs(10)).unwrap();
        let client = service.exchange(&secret).unwrap();
        let (_, mut active) = service
            .authenticate_connection(&client.bearer_token)
            .unwrap();
        assert!(service.revoke(&client.client_id));
        active.changed().await.unwrap();
        assert!(!*active.borrow());
        assert!(service
            .authenticate_connection(&client.bearer_token)
            .is_none());
        let secret = service.begin_pairing(Duration::from_secs(10)).unwrap();
        let client = service.exchange(&secret).unwrap();
        let (_, mut active) = service
            .authenticate_connection(&client.bearer_token)
            .unwrap();
        service.set_enabled(false);
        active.changed().await.unwrap();
        assert!(!*active.borrow());
    }
    #[test]
    fn expiry_and_concurrent_exchange() {
        let service = Arc::new(RemoteAccess::new(&Storage::default()).unwrap());
        service.set_enabled(true);
        let secret = service.begin_pairing(Duration::from_secs(1)).unwrap();
        service
            .state
            .lock()
            .unwrap()
            .pending
            .as_mut()
            .unwrap()
            .expires = Instant::now();
        assert!(service.exchange(&secret).is_err());
        let secret = service.begin_pairing(Duration::from_secs(10)).unwrap();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let s = service.clone();
                let secret = secret.clone();
                std::thread::spawn(move || s.exchange(&secret).is_ok())
            })
            .collect();
        assert_eq!(
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .filter(|success| *success)
                .count(),
            1
        );
    }
}
