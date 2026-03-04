use anyhow::Result;
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use serde::{de::DeserializeOwned, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

pub struct Channel {
    tx: FramedWrite<tokio::net::tcp::OwnedWriteHalf, LengthDelimitedCodec>,
    rx: FramedRead<tokio::net::tcp::OwnedReadHalf, LengthDelimitedCodec>,
}

impl Channel {
    fn from_stream(stream: TcpStream) -> Self {
        let (read, write) = stream.into_split();
        Self {
            tx: FramedWrite::new(write, LengthDelimitedCodec::new()),
            rx: FramedRead::new(read, LengthDelimitedCodec::new()),
        }
    }

    pub async fn connect(addr: &str) -> Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        Ok(Self::from_stream(stream))
    }

    pub async fn accept(listener: &TcpListener) -> Result<Self> {
        let (stream, _) = listener.accept().await?;
        Ok(Self::from_stream(stream))
    }

    pub async fn send<T: Serialize>(&mut self, msg: &T) -> Result<()> {
        let encoded = bincode::serialize(msg)?;
        self.tx.send(Bytes::from(encoded)).await?;
        Ok(())
    }

    pub async fn recv<T: DeserializeOwned>(&mut self) -> Result<T> {
        let frame = self
            .rx
            .next()
            .await
            .ok_or_else(|| anyhow::anyhow!("connection closed"))??;
        let msg = bincode::deserialize(&frame)?;
        Ok(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_send_recv_vec_u64() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        let server = tokio::spawn(async move {
            let mut ch = Channel::accept(&listener).await.unwrap();
            let received: Vec<u64> = ch.recv().await.unwrap();
            received
        });

        let mut client = Channel::connect(&addr).await.unwrap();
        let data: Vec<u64> = vec![1, 2, 3, 42, 999];
        client.send(&data).await.unwrap();

        let received = server.await.unwrap();
        assert_eq!(received, vec![1, 2, 3, 42, 999]);
    }

    #[tokio::test]
    async fn test_bidirectional() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        let server = tokio::spawn(async move {
            let mut ch = Channel::accept(&listener).await.unwrap();
            let msg: String = ch.recv().await.unwrap();
            ch.send(&format!("echo: {msg}")).await.unwrap();
        });

        let mut client = Channel::connect(&addr).await.unwrap();
        client.send(&String::from("hello")).await.unwrap();
        let reply: String = client.recv().await.unwrap();
        server.await.unwrap();
        assert_eq!(reply, "echo: hello");
    }
}
