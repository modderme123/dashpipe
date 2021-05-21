use clap::{App, Arg};
use fork::{chdir, fork, setsid, Fork};
use futures_util::{io::AsyncWriteExt as AsyncWriteExt2, StreamExt};
use futures_util::{Future, SinkExt};
use log::*;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::{collections::HashMap, net::SocketAddr, option::Option};
use std::{error::Error, thread::sleep, time::Duration};
use tokio::{io::{self, AsyncReadExt, AsyncWriteExt}, net::{TcpListener, TcpStream}, task::JoinHandle};

use futures_util::stream::FuturesUnordered;
use futures_util::TryFutureExt;
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use tokio_util::{compat::TokioAsyncWriteCompatExt, io::ReaderStream};
// use futures_util::future::select_all;
use std::sync::Arc;
use tokio::sync::Mutex;

fn server_address(port: u16) -> String {
    format!("localhost:{}", port)
}

async fn forward(mut a: TcpStream, mut b: WebSocketStream<TcpStream>) {
    let version = a.read_u16().await.unwrap();
    assert_eq!(version, 1);
    let header_size = a.read_u16().await.unwrap();
    let mut buf = vec![0u8; header_size as usize];
    let header_json = a.read_exact(&mut buf).await.unwrap();
    info!("[server] received header size {}", header_size);
    info!("[server] received header {:?}", header_json);
    let mut header_buffer = vec![0u8; header_size as usize + 4];
    header_buffer[0..2].copy_from_slice(&version.to_be_bytes());
    header_buffer[2..4].copy_from_slice(&header_size.to_be_bytes());
    header_buffer[4..].copy_from_slice(&buf);

    info!("[server] header buffer: {:?}", &header_buffer);

    let header_message = Message::binary(header_buffer);
    b.send(header_message).await.unwrap();

    let reader_stream = ReaderStream::new(a);
    let logged_stream = reader_stream.map(|x| {
        info!("[server] {:?}", x);
        x
    });
    let message_stream = logged_stream.map(|x| Ok(Message::binary(x.unwrap().to_vec())));
    let logged_messages = message_stream.map(|m| {
        info!("[server] message");
        m
    });
    logged_messages.forward(b).await.unwrap();
    info!("[server] done");
}

#[tokio::main]
async fn run_daemon(port: u16) {
    let server = TcpListener::bind(server_address(port)).await.unwrap();

    let web_sockets: HashMap<&str, WebSocketStream<TcpStream>> = HashMap::new();
    let cli_sockets: HashMap<&str, TcpStream> = HashMap::new();
    // let mut waits= FuturesUnordered::new();
    let web_ref = Arc::new(Mutex::new(web_sockets));
    let cli_ref = Arc::new(Mutex::new(cli_sockets));

    loop {
        tokio::select! {
            Ok(cxn) = server.accept() => {
              let fwd = handle_connect(cxn, web_ref.clone(), cli_ref.clone()).await;
            }
        }
    }
}

async fn handle_connect(
    cxn: (TcpStream, SocketAddr),
    web_sockets_ref: Arc<Mutex<HashMap<&str, WebSocketStream<TcpStream>>>>,
    cli_sockets_ref: Arc<Mutex<HashMap<&str, TcpStream>>>,
) -> Option<JoinHandle<()>> {
    let (stream, _) = cxn;
    let mut cli_sockets = cli_sockets_ref.lock().await;
    let mut web_sockets = web_sockets_ref.lock().await;

    let mut front = [0u8; 14];
    stream.peek(&mut front).await.expect("peek failed");
    if front == *b"GET / HTTP/1.1" {
        let ws = accept_async(stream).await.unwrap();

        let name = "tmpname1232";
        if let Some(socket) = cli_sockets.remove(name) {
            let handle = tokio::spawn(forward(socket, ws));
            return Some(handle)
        } else {
            web_sockets.insert(name, ws);
            return None;
        }
    } else {
        let name = "tmpname1232";
        if let Some(ws) = web_sockets.remove(name) {
            let handle = tokio::spawn(forward(stream, ws));
            return Some(handle);
        } else {
            cli_sockets.insert(name, stream);
            return None;
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
    info!("[client] header: {}", &header_str);

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
    env_logger::builder().filter_level(LevelFilter::Info).init(); // for now set logging level in code

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
                debug!("Starting daemon and sleeping 500ms"); // TODO fix race condition
                sleep(Duration::from_millis(500));
                client(port, &pipe_args).unwrap();
            }
            Fork::Child => {
                info!("[daemon] starting");
                if setsid().is_ok() {
                    chdir().unwrap();
                    // close_fd().unwrap(); // comment out to enable debug logging in daemon
                    if let Ok(Fork::Child) = fork() {
                        println!("[forked] foo");
                        run_daemon(port);
                    }
                }
            }
        }
    }
}
