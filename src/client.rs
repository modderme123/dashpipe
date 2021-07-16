use crate::proto::{self, PipeArgs};
use anyhow::Result;
use futures_util::{io::AsyncWriteExt as AsyncWriteExt2, StreamExt};
use log::*;
use tokio::{
    self,
    io::{self, AsyncWriteExt},
    net::TcpStream,
};
use tokio_util::{compat::TokioAsyncWriteCompatExt, io::ReaderStream};

#[tokio::main]
pub async fn client(port: u16, args: &PipeArgs, halt: bool) -> Result<()> {
    let address = proto::server_address(port);
    let mut stream = TcpStream::connect(&address).await?;
    debug!("[client] Connected to daemon {}", address);

    let header = proto::client_header_bytes(&args);
    stream.write(&header).await.expect("Couldn't send header");

    if !halt {
        let stdin = ReaderStream::new(io::stdin());
        stdin
            .forward(stream.compat_write().into_sink())
            .await
            .expect("Couldn't forward stream");
    }
    Ok(())
}
