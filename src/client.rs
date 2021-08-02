use crate::{cmd_line::CmdArguments, proto};
use anyhow::Result;
use futures_util::{io::AsyncWriteExt as AsyncWriteExt2, StreamExt};
use log::*;
use std::borrow::Borrow;
use tokio::{
    self,
    io::{self, AsyncBufReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{sleep, Duration},
};
use tokio_util::{compat::TokioAsyncWriteCompatExt, io::ReaderStream};

#[tokio::main]
pub async fn client(command_args: &CmdArguments) -> Result<()> {
    let address = proto::server_address(command_args.port);
    let mut stream = TcpStream::connect(&address).await?;
    debug!("[client] Connected to daemon {}", address);
    let header = proto::client_header_bytes(&command_args.pipe_args);
    stream.write(&header).await.expect("Couldn't send header");
    let halt = command_args.pipe_args.halt.unwrap_or(false);

    if halt {
        return Ok(());
    } else if let Some(file_name) = command_args.file.borrow() {
        return stream_file_by_line(&mut stream, file_name, &command_args.trickle).await;
    } else {
        let stdin = ReaderStream::new(io::stdin());
        let result = stdin.forward(stream.compat_write().into_sink()).await;
        return result.map_err(|e| e.into());
    }
}

async fn stream_file_by_line(
    to_stream: &mut TcpStream,
    file_name: &String,
    trickle: &Option<u16>,
) -> Result<()> {
    let file = tokio::fs::File::open(file_name).await?;
    let mut reader = tokio::io::BufReader::new(file);

    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line).await?;
        if read == 0 {
            break;
        } else {
            let s: &[u8] = line.as_ref();
            debug!("read line: {:?}", s);
            to_stream.write(s).await?;
            if let Some(delay) = trickle {
                sleep(Duration::from_millis(u64::from(*delay))).await;
            }
        }
    }
    Ok(())
}
