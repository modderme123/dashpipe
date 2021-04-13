use clap::App;
use fork::{chdir, close_fd, fork, setsid, Fork};
use futures_util::{io::AsyncWriteExt as AsyncWriteExt2, StreamExt};
use log::*;
use std::{collections::HashMap, error::Error, thread::sleep, time::Duration};
use tokio::{
    io::{self, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use tokio_util::{compat::TokioAsyncWriteCompatExt, io::ReaderStream};

async fn forward(a: TcpStream, b: WebSocketStream<TcpStream>) {
    let message_stream = ReaderStream::new(a).map(|x| Ok(Message::binary(x.unwrap().to_vec())));
    message_stream.forward(b).await.unwrap();
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

/*
 * protocol for header message to browser
 *    [version number (16 bits)] [json length 16 bits] [header: json utf8]
*/
const PROTOCOL_VERSION: [u8; 2] = (1u16).to_be_bytes();
use serde_json::{json, to_vec};

#[tokio::main]
async fn client(port: u16) -> Result<(), Box<dyn Error>> {
    let address = server_address(port);
    let mut stream = TcpStream::connect(&address).await?;
    info!("[client] Connected to daemon {}", address);
    let mut header = Vec::new();
    let header_js = json!({
        "title": "fred"
    });
    let header_str = to_vec(&header_js)?;
    let length = header_str.len() as u16;
    let length_bytes = length.to_be_bytes();

    header.extend_from_slice(&PROTOCOL_VERSION);
    header.extend_from_slice(&length_bytes);
    header.extend_from_slice(&header_str);

    stream.write(&header).await.expect("Couldn't send header");
    let stdin = ReaderStream::new(io::stdin());
    stdin
        .forward(stream.compat_write().into_sink())
        .await
        .expect("Couldn't forward stream");
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
    env_logger::builder().filter_level(LevelFilter::Info).init(); // for now turn all all logging

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
                    close_fd().unwrap(); // comment out to enable debug logging in daemon
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
