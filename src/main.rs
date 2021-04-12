use clap::App;
use env_logger::Builder;
use fork::{chdir, close_fd, fork, setsid, Fork};
use futures_util::{io::AsyncWriteExt as AsyncWriteExt2, SinkExt, StreamExt};
use log::*;
use std::{collections::HashMap, error::Error, thread::sleep, time::Duration};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use tokio_util::{compat::TokioAsyncWriteCompatExt, io::ReaderStream};

async fn forward(mut a: TcpStream, mut b: WebSocketStream<TcpStream>) {
    let mut buffer = [0u8; 256];
    loop {
        let bytes = a.read(&mut buffer[..]).await.unwrap();
        if bytes > 0 {
            b.send(Message::binary(buffer[0..bytes].to_vec()))
                .await
                .unwrap();
        }
    }
}

#[tokio::main]
async fn run_daemon(port: u16) {
    let server = TcpListener::bind(server_address(port)).await.unwrap();

    let mut web_sockets: HashMap<&str, WebSocketStream<TcpStream>> = HashMap::new();
    let mut cli_sockets: HashMap<&str, TcpStream> = HashMap::new();

    while let Ok((stream, _)) = server.accept().await {
        let mut front = [0u8; 14];
        stream.peek(&mut front).await.expect("peek failed");
        if front == *b"GET / HTTP/1.1" {
            let ws = accept_async(stream).await.unwrap();

            let name = "tmpname1232";
            if let Some(socket) = cli_sockets.remove(name) {
                tokio::spawn(forward(socket, ws));
            } else {
                web_sockets.insert(name, ws);
            }
        } else {
            let name = "tmpname1232";
            if let Some(ws) = web_sockets.remove(name) {
                tokio::spawn(forward(stream, ws));
            } else {
                cli_sockets.insert(name, stream);
            }
        }
    }
}

#[tokio::main]
async fn client(port: u16) -> Result<(), Box<dyn Error>> {
    let address = server_address(port);
    info!("client address {}", address);
    let mut stream = TcpStream::connect(address).await?;
    info!("[client] Connected to daemon");
    stream.write(b"welcome, testing 123").await.unwrap();
    let stdin = ReaderStream::new(io::stdin());
    stdin
        .forward(stream.compat_write().into_sink())
        .await
        .unwrap();
    Ok(())
}

fn main() {
    let matches = App::new("DashPipe")
        .version("0.1")
        .author("Modder Me <modderme123@gmail.com>")
        .about("Pipes command line data to dashberry.ml")
        .args_from_usage("--port=[port] 'localhost port for daemon")
        .get_matches();

    // env_logger::init();
    Builder::new().filter_level(LevelFilter::Info).init(); // for now turn all all logging

    let port_str = matches.value_of("port").unwrap_or("3030");
    let port: u16 = port_str.parse().unwrap();
    

    if client(port).is_err() {
        match fork().unwrap() {
            Fork::Parent(_) => {
                debug!("Starting daemon and sleeping 500ms");
                sleep(Duration::from_millis(500));
                client(port).unwrap();
            }
            Fork::Child => {
                info!("[daemon] starting");
                if setsid().is_ok() {
                    chdir().unwrap();
                    close_fd().unwrap();    // comment out to enable debug logging in daemon
                    if let Ok(Fork::Child) = fork() {
                        run_daemon(port);
                    }
                }
            }
        }
    }
}


fn server_address(port: u16) -> String {
    let mut address = "localhost:".to_owned();
    address.push_str(&port.to_string());
    address
}