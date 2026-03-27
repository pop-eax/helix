use std::{collections::HashMap, net::SocketAddr, path::Path, time::Duration};

use anyhow::{bail, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::net::TcpListener;

use crate::Channel;

// ---- Public types ----

/// Configuration for connecting to the peer network.
///
/// # File format
/// One `host:port` per line. Lines starting with `#` are ignored.
/// Use port `0` in your own entry to let the OS assign a random port.
///
/// ```text
/// # parties.txt
/// 127.0.0.1:7000   # party 0
/// 127.0.0.1:7001   # party 1
/// 127.0.0.1:0      # party 2 – random port
/// ```
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Addresses of all parties in order (index = party ID).
    pub parties: Vec<String>,
    /// Your own party ID (index into `parties`).
    pub my_id: usize,
}

impl NetworkConfig {
    /// Load party list from a text file (one `host:port` per line).
    pub fn from_file(path: impl AsRef<Path>, my_id: usize) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| anyhow::anyhow!("cannot read party file {:?}: {e}", path.as_ref()))?;
        let parties = content
            .lines()
            .map(|l| l.split('#').next().unwrap_or("").trim())
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();
        Ok(Self { parties, my_id })
    }

    /// Build from an explicit iterator of addresses.
    pub fn from_addrs(parties: impl IntoIterator<Item = impl Into<String>>, my_id: usize) -> Self {
        Self {
            parties: parties.into_iter().map(Into::into).collect(),
            my_id,
        }
    }
}

/// Information about a connected peer.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub id: usize,
    /// The peer's actual listen address (may differ from config if port was 0).
    pub addr: String,
}

/// An established peer-to-peer network ready for protocol use.
///
/// Each pair of parties has two dedicated TCP channels:
/// - **send** channel: the connection *we* initiated (used for sending to that peer)
/// - **recv** channel: the connection *they* initiated (used for receiving from that peer)
///
/// This keeps traffic fully separated and avoids head-of-line blocking.
pub struct Network {
    pub my_id: usize,
    /// Our actual listen address (port may differ from config if 0 was specified).
    pub my_addr: String,
    /// All other parties, sorted by ID.
    pub peers: Vec<PeerInfo>,
    /// Channels for sending TO a peer (keyed by peer ID).
    send: HashMap<usize, Channel>,
    /// Channels for receiving FROM a peer (keyed by peer ID).
    recv: HashMap<usize, Channel>,
}

impl Network {
    /// Send `msg` to party `to`.
    pub async fn send<T: Serialize>(&mut self, to: usize, msg: &T) -> Result<()> {
        self.send
            .get_mut(&to)
            .ok_or_else(|| anyhow::anyhow!("no send channel to party {to}"))?
            .send(msg)
            .await
    }

    /// Receive a message from party `from`.
    pub async fn recv<T: DeserializeOwned>(&mut self, from: usize) -> Result<T> {
        self.recv
            .get_mut(&from)
            .ok_or_else(|| anyhow::anyhow!("no recv channel from party {from}"))?
            .recv()
            .await
    }

    /// Send the same message to all peers.
    pub async fn broadcast<T: Serialize>(&mut self, msg: &T) -> Result<()> {
        let ids: Vec<usize> = self.send.keys().copied().collect();
        for id in ids {
            self.send(id, msg).await?;
        }
        Ok(())
    }

    /// Receive one message from every peer; returns results sorted by peer ID.
    pub async fn gather<T: DeserializeOwned>(&mut self) -> Result<Vec<(usize, T)>> {
        let mut ids: Vec<usize> = self.recv.keys().copied().collect();
        ids.sort_unstable();
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let msg: T = self.recv(id).await?;
            out.push((id, msg));
        }
        Ok(out)
    }

    /// Total number of parties (including self).
    pub fn n_parties(&self) -> usize {
        self.peers.len() + 1
    }
}

// ---- Internal setup message ----

#[derive(Serialize, Deserialize)]
enum SetupMsg {
    Hello { from_id: usize, actual_addr: String },
    CommitHash(u64),
}

// ---- Connect ----

/// Connect to the network described by `config`.
///
/// **All parties must call this concurrently.**  The call blocks until:
/// 1. Every pair has bidirectional channels established.
/// 2. All parties have verified they share the same party list (commitment).
///
/// # Errors
/// Returns an error if any connection fails after retries, or if the
/// party-list commitment does not match across parties.
pub async fn connect(config: NetworkConfig) -> Result<Network> {
    let n = config.parties.len();
    let my_id = config.my_id;

    if n == 0 {
        bail!("party list is empty");
    }
    if my_id >= n {
        bail!("my_id {my_id} is out of range for {n} parties");
    }

    // ---- Phase 1: bind listener ----
    //
    // We bind on our configured address.  Port 0 tells the OS to pick a free
    // port; we read the actual port back and announce it to peers.
    let listener = TcpListener::bind(&config.parties[my_id]).await.map_err(|e| {
        anyhow::anyhow!("failed to bind {}: {e}", config.parties[my_id])
    })?;
    let local_addr = listener.local_addr()?;
    let configured: SocketAddr = config.parties[my_id].parse().map_err(|e| {
        anyhow::anyhow!("invalid address {:?}: {e}", config.parties[my_id])
    })?;
    // Preserve the configured IP (e.g. 127.0.0.1) but use the actual port.
    let my_actual_addr = format!("{}:{}", configured.ip(), local_addr.port());

    // ---- Phase 2: accept N-1 incoming connections in the background ----
    let n_peers = n - 1;
    let accept_task = tokio::spawn(async move {
        let mut incoming = Vec::with_capacity(n_peers);
        for _ in 0..n_peers {
            incoming.push(Channel::accept(&listener).await?);
        }
        Ok::<Vec<Channel>, anyhow::Error>(incoming)
    });

    // ---- Phase 3: connect to every other party and announce ourselves ----
    let mut send: HashMap<usize, Channel> = HashMap::with_capacity(n_peers);
    for (peer_id, peer_addr) in config.parties.iter().enumerate() {
        if peer_id == my_id {
            continue;
        }
        let mut ch = connect_with_retry(peer_addr).await.map_err(|e| {
            anyhow::anyhow!("could not reach party {peer_id} at {peer_addr}: {e}")
        })?;
        ch.send(&SetupMsg::Hello {
            from_id: my_id,
            actual_addr: my_actual_addr.clone(),
        })
        .await?;
        send.insert(peer_id, ch);
    }

    // ---- Phase 4: collect incoming connections and read their Hellos ----
    let incoming = accept_task
        .await
        .map_err(|e| anyhow::anyhow!("accept task panicked: {e}"))??;

    let mut recv: HashMap<usize, Channel> = HashMap::with_capacity(n_peers);
    let mut peer_actual_addrs: HashMap<usize, String> = HashMap::with_capacity(n_peers);

    for mut ch in incoming {
        match ch.recv::<SetupMsg>().await? {
            SetupMsg::Hello { from_id, actual_addr } => {
                if from_id == my_id || from_id >= n {
                    bail!("received Hello with invalid from_id {from_id}");
                }
                if recv.contains_key(&from_id) {
                    bail!("duplicate Hello from party {from_id}");
                }
                peer_actual_addrs.insert(from_id, actual_addr);
                recv.insert(from_id, ch);
            }
            _ => bail!("expected Hello during handshake"),
        }
    }

    // ---- Phase 5: build canonical party list ----
    //
    // Sort by ID so every party constructs the list in the same order.
    let mut all_parties: Vec<(usize, String)> = peer_actual_addrs.into_iter().collect();
    all_parties.push((my_id, my_actual_addr.clone()));
    all_parties.sort_unstable_by_key(|&(id, _)| id);

    // ---- Phase 6: commitment — exchange and verify party-list hashes ----
    //
    // We send our hash on every outgoing channel, then receive from every
    // incoming channel.  Sending all before receiving avoids deadlock.
    let my_hash = commitment_hash(&all_parties);

    for ch in send.values_mut() {
        ch.send(&SetupMsg::CommitHash(my_hash)).await?;
    }
    for (&peer_id, ch) in recv.iter_mut() {
        match ch.recv::<SetupMsg>().await? {
            SetupMsg::CommitHash(their_hash) => {
                if their_hash != my_hash {
                    bail!(
                        "party-list commitment mismatch with party {peer_id}: \
                         our hash {my_hash:#016x} ≠ their hash {their_hash:#016x}"
                    );
                }
            }
            _ => bail!("expected CommitHash from party {peer_id}"),
        }
    }

    // ---- Done ----
    let peers: Vec<PeerInfo> = all_parties
        .into_iter()
        .filter(|(id, _)| *id != my_id)
        .map(|(id, addr)| PeerInfo { id, addr })
        .collect();

    Ok(Network { my_id, my_addr: my_actual_addr, peers, send, recv })
}

// ---- Helpers ----

/// Connect to `addr` with exponential-backoff retry (≈10 s total window).
async fn connect_with_retry(addr: &str) -> Result<Channel> {
    let mut delay = Duration::from_millis(100);
    let mut last_err = anyhow::anyhow!("no attempts made");
    for _ in 0..8 {
        match Channel::connect(addr).await {
            Ok(ch) => return Ok(ch),
            Err(e) => {
                last_err = e;
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(2));
            }
        }
    }
    Err(last_err)
}

/// Deterministic FNV-1a hash of the canonical party list.
/// Used to verify all parties agree on the same set of participants.
fn commitment_hash(parties: &[(usize, String)]) -> u64 {
    const BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x00000100000001b3;
    let mut h = BASIS;
    for (id, addr) in parties {
        for &b in id.to_le_bytes().iter().chain(addr.as_bytes()) {
            h ^= b as u64;
            h = h.wrapping_mul(PRIME);
        }
        // Field separator so ("ab","c") ≠ ("a","bc")
        h ^= b'|' as u64;
        h = h.wrapping_mul(PRIME);
    }
    h
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::task::JoinSet;

    /// Spin up N parties locally and verify they all connect and commit.
    async fn three_party_network() -> Vec<Network> {
        // Bind N listeners on port 0 to discover free ports.
        let listeners: Vec<TcpListener> = {
            let mut ls = Vec::new();
            for _ in 0..3 {
                ls.push(TcpListener::bind("127.0.0.1:0").await.unwrap());
            }
            ls
        };
        let addrs: Vec<String> = listeners
            .iter()
            .map(|l| l.local_addr().unwrap().to_string())
            .collect();

        // Drop listeners so the ports are free for the parties to rebind.
        drop(listeners);

        let mut set = JoinSet::new();
        for my_id in 0..3 {
            let cfg = NetworkConfig::from_addrs(addrs.clone(), my_id);
            set.spawn(async move { connect(cfg).await.unwrap() });
        }

        let mut networks = Vec::new();
        while let Some(result) = set.join_next().await {
            networks.push(result.unwrap());
        }
        networks.sort_unstable_by_key(|n| n.my_id);
        networks
    }

    #[tokio::test]
    async fn all_parties_connect_and_commit() {
        let networks = three_party_network().await;
        assert_eq!(networks.len(), 3);
        for net in &networks {
            assert_eq!(net.n_parties(), 3);
            assert_eq!(net.peers.len(), 2);
        }
    }

    #[tokio::test]
    async fn send_recv_between_parties() {
        let mut networks = three_party_network().await;

        // Party 0 sends a value to party 1; party 1 sends a value to party 2.
        let (mut n0, mut n1, mut n2) = {
            let mut it = networks.drain(..);
            (it.next().unwrap(), it.next().unwrap(), it.next().unwrap())
        };

        let t0 = tokio::spawn(async move { n0.send(1, &42u64).await.unwrap() });
        let t1 = tokio::spawn(async move {
            let v: u64 = n1.recv(0).await.unwrap();
            assert_eq!(v, 42);
            n1.send(2, &99u64).await.unwrap();
        });
        let t2 = tokio::spawn(async move {
            let v: u64 = n2.recv(1).await.unwrap();
            assert_eq!(v, 99);
        });

        t0.await.unwrap();
        t1.await.unwrap();
        t2.await.unwrap();
    }

    #[tokio::test]
    async fn broadcast_and_gather() {
        let mut networks = three_party_network().await;
        let (mut n0, mut n1, mut n2) = {
            let mut it = networks.drain(..);
            (it.next().unwrap(), it.next().unwrap(), it.next().unwrap())
        };

        // Every party broadcasts its own ID, then every party gathers from all peers.
        // broadcast+gather must be concurrent across parties to avoid deadlock.
        let t0 = tokio::spawn(async move {
            n0.broadcast(&0u64).await.unwrap();
            let msgs: Vec<(usize, u64)> = n0.gather().await.unwrap();
            assert!(msgs.iter().any(|&(id, v)| id == 1 && v == 1));
            assert!(msgs.iter().any(|&(id, v)| id == 2 && v == 2));
        });
        let t1 = tokio::spawn(async move {
            n1.broadcast(&1u64).await.unwrap();
            let msgs: Vec<(usize, u64)> = n1.gather().await.unwrap();
            assert!(msgs.iter().any(|&(id, v)| id == 0 && v == 0));
            assert!(msgs.iter().any(|&(id, v)| id == 2 && v == 2));
        });
        let t2 = tokio::spawn(async move {
            n2.broadcast(&2u64).await.unwrap();
            let msgs: Vec<(usize, u64)> = n2.gather().await.unwrap();
            assert!(msgs.iter().any(|&(id, v)| id == 0 && v == 0));
            assert!(msgs.iter().any(|&(id, v)| id == 1 && v == 1));
        });

        t0.await.unwrap();
        t1.await.unwrap();
        t2.await.unwrap();
    }
}
