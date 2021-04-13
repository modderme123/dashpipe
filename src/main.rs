use clap::{App, Arg};
use fork::{chdir, close_fd, fork, setsid, Fork};
use futures_util::{io::AsyncWriteExt as AsyncWriteExt2, StreamExt};
use log::*;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::{collections::HashMap, error::Error, thread::sleep, time::Duration};
use tokio::{
    io::{self, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use tokio_util::{compat::TokioAsyncWriteCompatExt, io::ReaderStream};

fn server_address(port: u16) -> String {
    format!("localhost:{}", port)
}

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

#[skip_serializing_none]
#[derive(Serialize, Deserialize)]
struct PipeArgs {
    title: Option<String>,
    dashboard: Option<String>,
    chart: Option<String>,
    no_show: Option<bool>,
    append: Option<bool>,
}

#[tokio::main]
async fn client(port: u16, args: &PipeArgs) -> Result<(), Box<dyn Error>> {
    let address = server_address(port);
    let mut stream = TcpStream::connect(&address).await?;
    info!("[client] Connected to daemon {}", address);
    let mut header = Vec::new();
    let header_vec = serde_json::to_vec(&args)?;
    let length = header_vec.len() as u16;
    let length_bytes = length.to_be_bytes();
    let header_str = serde_json::to_string(&args)?;
    info!("header: {}", &header_str);

    header.extend_from_slice(&PROTOCOL_VERSION);
    header.extend_from_slice(&length_bytes);
    header.extend_from_slice(&header_vec);

    stream.write(&header).await.expect("Couldn't send header");
    let stdin = ReaderStream::new(io::stdin());
    stdin
        .forward(stream.compat_write().into_sink())
        .await
        .expect("Couldn't forward stream");
    Ok(())
}

fn main() {
    let arg_matches = App::new("DashPipe")
        .version("0.1")
        .author("Modder Me <modderme123@gmail.com>")
        .about("Pipes command line data to dashberry.ml")
        .arg(
            Arg::with_name("port")
                .long("port")
                .help("localhost port for daemon")
                .value_name("port")
                .takes_value(true)
                .default_value("3030"),
        )
        .arg_from_usage("--title=[title] 'title of data set'")
        .arg_from_usage("--dashboard=[dashboard] 'name of dashboard that will display the data'")
        .arg_from_usage("--chart=[chart] 'name of chart that will display the data'")
        .arg_from_usage("--no-show 'send the data without displaying it'")
        .arg_from_usage("--append 'append data to an existing chart'")
        .get_matches();

    // env_logger::init();
    env_logger::builder().filter_level(LevelFilter::Info).init(); // for now turn all all logging

    let port_str = arg_matches.value_of("port").unwrap();
    let port: u16 = port_str.parse().unwrap();
    let pipe_args = PipeArgs {
        title: arg_matches.value_of("title").map(str::to_owned),
        dashboard: arg_matches.value_of("dashboard").map(str::to_owned),
        chart: arg_matches.value_of("dashboard").map(str::to_owned),
        no_show: arg_matches.is_present("no-show").then(|| true),
        append: arg_matches.is_present("append").then(|| true),
    };

    if client(port, &pipe_args).is_err() {
        match fork().unwrap() {
            Fork::Parent(_) => {
                debug!("Starting daemon and sleeping 500ms");
                sleep(Duration::from_millis(500));
                client(port, &pipe_args).unwrap();
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
