use std::collections::HashMap;

use anyhow::{anyhow, Result};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::Network;

// ---- Trait ----

/// Abstraction over the transport used by a [`Runner`].
///
/// Implementors:
/// - [`Network`] — real TCP, established via [`crate::connect`].
/// - [`StubNetwork`] — in-memory channels for unit/integration tests.
///
/// Anyone can plug in their own implementation and the runner will work
/// without modification.
#[allow(async_fn_in_trait)]
pub trait NetworkLayer {
    fn my_id(&self) -> usize;
    fn n_parties(&self) -> usize;

    /// Send raw bytes to party `to`.
    async fn send_to(&mut self, to: usize, msg: Vec<u8>) -> Result<()>;

    /// Receive raw bytes from party `from`.
    async fn recv_from(&mut self, from: usize) -> Result<Vec<u8>>;

    /// Send to every peer (all parties except self).
    async fn broadcast(&mut self, msg: Vec<u8>) -> Result<()> {
        let my_id = self.my_id();
        let n = self.n_parties();
        for to in 0..n {
            if to != my_id {
                self.send_to(to, msg.clone()).await?;
            }
        }
        Ok(())
    }

    /// Receive one message from every peer, returned sorted by sender ID.
    async fn gather(&mut self) -> Result<Vec<(usize, Vec<u8>)>> {
        let my_id = self.my_id();
        let n = self.n_parties();
        let mut out = Vec::with_capacity(n - 1);
        for from in 0..n {
            if from != my_id {
                out.push((from, self.recv_from(from).await?));
            }
        }
        Ok(out)
    }
}

// ---- impl for real Network ----

impl NetworkLayer for Network {
    fn my_id(&self) -> usize {
        self.my_id
    }

    fn n_parties(&self) -> usize {
        self.n_parties()
    }

    async fn send_to(&mut self, to: usize, msg: Vec<u8>) -> Result<()> {
        // Reuse the Channel's bincode framing; Vec<u8> round-trips transparently.
        self.send(to, &msg).await
    }

    async fn recv_from(&mut self, from: usize) -> Result<Vec<u8>> {
        self.recv::<Vec<u8>>(from).await
    }
}

// ---- StubNetwork ----

/// In-memory network for testing — no TCP, no ports.
///
/// Create a set of `n` connected stubs with [`stub_networks`] and hand one
/// to each party's [`Runner`].  Messages are passed through tokio channels,
/// so tests can run entirely within a single process.
pub struct StubNetwork {
    my_id: usize,
    n: usize,
    /// Senders indexed by destination party ID.
    senders: HashMap<usize, UnboundedSender<Vec<u8>>>,
    /// Receivers indexed by source party ID.
    receivers: HashMap<usize, UnboundedReceiver<Vec<u8>>>,
}

/// Create `n` mutually connected [`StubNetwork`]s.
///
/// ```
/// # use net::stub_networks;
/// let stubs = stub_networks(3);
/// // stubs[0], stubs[1], stubs[2] can now exchange messages.
/// ```
pub fn stub_networks(n: usize) -> Vec<StubNetwork> {
    // For every ordered pair (i, j) with i ≠ j, create a directed channel.
    // The sender of (i→j) goes to stub i; the receiver of (i→j) goes to stub j.
    let mut senders: Vec<HashMap<usize, UnboundedSender<Vec<u8>>>> =
        (0..n).map(|_| HashMap::with_capacity(n.saturating_sub(1))).collect();
    let mut receivers: Vec<HashMap<usize, UnboundedReceiver<Vec<u8>>>> =
        (0..n).map(|_| HashMap::with_capacity(n.saturating_sub(1))).collect();

    for i in 0..n {
        for j in 0..n {
            if i != j {
                let (tx, rx) = mpsc::unbounded_channel::<Vec<u8>>();
                senders[i].insert(j, tx);   // stub i sends to j
                receivers[j].insert(i, rx); // stub j receives from i
            }
        }
    }

    senders
        .into_iter()
        .zip(receivers)
        .enumerate()
        .map(|(my_id, (s, r))| StubNetwork { my_id, n, senders: s, receivers: r })
        .collect()
}

impl NetworkLayer for StubNetwork {
    fn my_id(&self) -> usize {
        self.my_id
    }

    fn n_parties(&self) -> usize {
        self.n
    }

    async fn send_to(&mut self, to: usize, msg: Vec<u8>) -> Result<()> {
        self.senders
            .get(&to)
            .ok_or_else(|| anyhow!("no sender to party {to}"))?
            .send(msg)
            .map_err(|_| anyhow!("channel to party {to} closed"))
    }

    async fn recv_from(&mut self, from: usize) -> Result<Vec<u8>> {
        self.receivers
            .get_mut(&from)
            .ok_or_else(|| anyhow!("no receiver from party {from}"))?
            .recv()
            .await
            .ok_or_else(|| anyhow!("channel from party {from} closed"))
    }
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_send_recv() {
        let mut stubs = stub_networks(3);
        let mut s0 = stubs.remove(0);
        let mut s1 = stubs.remove(0);
        let mut s2 = stubs.remove(0);

        s0.send_to(1, b"hello".to_vec()).await.unwrap();
        s1.send_to(2, b"world".to_vec()).await.unwrap();

        let msg = s1.recv_from(0).await.unwrap();
        assert_eq!(msg, b"hello");
        let msg = s2.recv_from(1).await.unwrap();
        assert_eq!(msg, b"world");
    }

    #[tokio::test]
    async fn stub_broadcast_gather() {
        let mut stubs = stub_networks(3);
        let (mut s0, mut s1, mut s2) = {
            let mut it = stubs.drain(..);
            (it.next().unwrap(), it.next().unwrap(), it.next().unwrap())
        };

        // All parties broadcast their ID byte, then gather.
        let t0 = tokio::spawn(async move {
            s0.broadcast(vec![0]).await.unwrap();
            let got = s0.gather().await.unwrap();
            assert!(got.iter().any(|(id, v)| *id == 1 && v == &[1u8]));
            assert!(got.iter().any(|(id, v)| *id == 2 && v == &[2u8]));
        });
        let t1 = tokio::spawn(async move {
            s1.broadcast(vec![1]).await.unwrap();
            s1.gather().await.unwrap();
        });
        let t2 = tokio::spawn(async move {
            s2.broadcast(vec![2]).await.unwrap();
            s2.gather().await.unwrap();
        });

        t0.await.unwrap();
        t1.await.unwrap();
        t2.await.unwrap();
    }
}
