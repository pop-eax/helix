use anyhow::Result;
use tokio::net::TcpListener;

use crate::Channel;

pub struct PartyConfig {
    pub id: usize,
    pub addr: String,
}

impl PartyConfig {
    pub fn new(id: usize, addr: impl Into<String>) -> Self {
        Self { id, addr: addr.into() }
    }

    pub async fn listen(&self) -> Result<TcpListener> {
        let listener = TcpListener::bind(&self.addr).await?;
        Ok(listener)
    }

    pub async fn connect(&self) -> Result<Channel> {
        Channel::connect(&self.addr).await
    }
}
