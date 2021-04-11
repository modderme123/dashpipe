use clap::App;
use futures_util::{io::AsyncWriteExt as AsyncWriteExt2, SinkExt, StreamExt};
use std::{collections::HashMap, error::Error, thread::sleep, time::Duration};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use tokio_util::{compat::TokioAsyncWriteCompatExt, io::ReaderStream};

use fork::{chdir, close_fd, fork, setsid, Fork};

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
async fn run_daemon() {
    let server = TcpListener::bind("127.0.0.1:3030").await.unwrap();

    let mut web_sockets: HashMap<&str, WebSocketStream<TcpStream>> = HashMap::new();
    let mut cli_sockets: HashMap<&str, TcpStream> = HashMap::new();

    while let Ok((stream, _)) = server.accept().await {
        let mut front = [0u8; 14];
        stream.peek(&mut front).await.expect("peek failed");
        if front == b"GET / HTTP/1.1".to_owned() {
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
async fn client() -> Result<(), Box<dyn Error>> {
    let mut stream = TcpStream::connect("127.0.0.1:3030").await?;
    println!("[client] Connected to daemon");
    stream.write(b"welcome, testing 123").await.unwrap();
    let stdin = ReaderStream::new(io::stdin());
    stdin
        .forward(stream.compat_write().into_sink())
        .await
        .unwrap();
    Ok(())
}

fn main() {
    App::new("DashPipe")
        .version("0.1")
        .author("Modder Me <modderme123@gmail.com>")
        .about("Pipes command line data to dashberry.ml")
        .get_matches();

    if let Err(_) = client() {
        match fork().unwrap() {
            Fork::Parent(_) => {
                println!("Starting daemon and sleeping 500ms");
                sleep(Duration::from_millis(500));
                client().unwrap();
            }
            Fork::Child => {
                println!("[daemon] starting");
                if let Ok(_) = setsid() {
                    chdir().unwrap();
                    close_fd().unwrap();
                    if let Ok(Fork::Child) = fork() {
                        run_daemon();
                    }
                }
            }
        }
    }
}
