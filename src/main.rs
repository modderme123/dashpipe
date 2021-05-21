use clap::{App, Arg};
use fork::{chdir, fork, setsid, Fork};
use futures_util::SinkExt;
use futures_util::{io::AsyncWriteExt as AsyncWriteExt2, StreamExt};
use log::*;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::{collections::HashMap, net::SocketAddr, option::Option};
use std::{error::Error, thread::sleep, time::Duration};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

use futures_util::stream::FuturesUnordered;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use tokio_util::{compat::TokioAsyncWriteCompatExt, io::ReaderStream};

/** Protocol for header message to browser
 *    [version number (16 bits)] [json length 16 bits] [header: json utf8]
 */
struct ProtocolHeader {
    version: u16,
    /* length: u16 // length field is in the protocol, but here we can use header_json.len() */
    header_json: Vec<u8>,
}

const PROTOCOL_VERSION: u16 = 1u16;
const PROTOCOL_VERSION_BYTES: [u8; 2] = PROTOCOL_VERSION.to_be_bytes();

fn server_address(port: u16) -> String {
    format!("localhost:{}", port)
}

/** Consume a protocol header from a command line client tcp stream.  */
async fn parse_header(input: &mut TcpStream) -> ProtocolHeader {
    let version = input.read_u16().await.unwrap();
    assert_eq!(version, PROTOCOL_VERSION);
    let header_size = input.read_u16().await.unwrap();
    let mut header_json = vec![0u8; header_size as usize];
    let header_json_bytes = input.read_exact(&mut header_json).await.unwrap();
    assert_eq!(header_json_bytes as u16, header_size);

    ProtocolHeader {
        version,
        header_json,
    }
}

/** Write a protocol header into a byte array */
fn header_to_bytes(header: &ProtocolHeader) -> Vec<u8> {
    let json_size = header.header_json.len();
    let json_size_u16 = json_size as u16;
    let mut buffer = vec![0u8; json_size + 4];
    buffer[0..2].copy_from_slice(&header.version.to_be_bytes());
    buffer[2..4].copy_from_slice(&json_size_u16.to_be_bytes());
    buffer[4..].copy_from_slice(&header.header_json);

    buffer
}

/** Forward a protocol stream from a command line client to a browser websocket.
 * The protocol header is parsed from the command line client and sent in a single
 * websocket message. Data follows in one or more websocket messages. */
async fn forward(mut a: TcpStream, mut b: WebSocketStream<TcpStream>) {
    let header = parse_header(&mut a).await;
    let header_buffer = header_to_bytes(&header);
    let header_message = Message::binary(header_buffer);
    b.send(header_message).await.unwrap();

    let reader_stream = ReaderStream::new(a);
    let message_stream = reader_stream.map(|x| Ok(Message::binary(x.unwrap().to_vec())));
    message_stream.forward(b).await.unwrap();
    debug!("[forward] done");
}

/** Run a daemon server that listens for connections from command line clients and web browsers. */
#[tokio::main]
async fn run_daemon(port: u16, once_only: bool) {
    let server = TcpListener::bind(server_address(port)).await.unwrap();

    let web_sockets: HashMap<&str, WebSocketStream<TcpStream>> = HashMap::new();
    let cli_sockets: HashMap<&str, TcpStream> = HashMap::new();
    let mut waits = FuturesUnordered::new();
    let web_ref = Arc::new(Mutex::new(web_sockets));
    let cli_ref = Arc::new(Mutex::new(cli_sockets));

    loop {
        tokio::select! {
            Ok(cxn) = server.accept() => {
              let fwd = handle_connect(cxn, web_ref.clone(), cli_ref.clone()).await;
              match fwd {
                  Some(join_handle) => waits.push(join_handle),
                  _ => ()
              }
            },
            Some(x) = waits.next() => {
                trace!("[daemon loop] fwd complete, {:?}", x);
                if once_only {
                    debug!("[daemon loop] quit");
                    break;
                }
            }
        }
    }
}

/** Handle a connection to the daemon server from the browser or command line client.
 */
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
            return Some(handle);
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

/** json messages sent from client to daemon to browser */
#[skip_serializing_none]
#[derive(Serialize, Deserialize)]
struct PipeArgs {
    dashboard: Option<String>, // This is only needed for client to daemon
    once: Option<bool>,        // This is only needed for client to daemon
    title: Option<String>,
    chart: Option<String>,
    no_show: Option<bool>,
    append: Option<bool>,
}

/** Write command line arguments into a protocol header. */
fn client_header_bytes(args: &PipeArgs) -> Vec<u8> {
    let mut bytes = Vec::new();
    let header_vec = serde_json::to_vec(&args).unwrap();
    let length = header_vec.len() as u16;
    let length_bytes = length.to_be_bytes();

    if log::max_level() >= log::Level::Debug {
        let header_str = serde_json::to_string(&args).unwrap();
        debug!("[client] header: {}", &header_str);
    }

    bytes.extend_from_slice(&PROTOCOL_VERSION_BYTES);
    bytes.extend_from_slice(&length_bytes);
    bytes.extend_from_slice(&header_vec);

    bytes
}

/** cmd line client. Runs in its own OS process and terminates when it finishes
 * forwarding its input stream to the daemon . */
#[tokio::main]
async fn client(port: u16, args: &PipeArgs) -> Result<(), Box<dyn Error>> {
    let address = server_address(port);
    let mut stream = TcpStream::connect(&address).await?;
    debug!("[client] Connected to daemon {}", address);

    let header = client_header_bytes(&args);
    stream.write(&header).await.expect("Couldn't send header");

    let stdin = ReaderStream::new(io::stdin());
    stdin
        .forward(stream.compat_write().into_sink())
        .await
        .expect("Couldn't forward stream");
    Ok(())
}

fn main() {
    let (pipe_args, port) = cmd_line_arguments();

    if client(port, &pipe_args).is_err() {
        match fork().unwrap() {
            Fork::Parent(_) => {
                debug!("Starting daemon and sleeping 500ms"); // TODO fix race condition
                sleep(Duration::from_millis(500));
                client(port, &pipe_args).unwrap();
            }
            Fork::Child => {
                debug!("[daemon] starting");
                if setsid().is_ok() {
                    chdir().unwrap();
                    // close_fd().unwrap(); // comment out to enable debug logging in daemon
                    if let Ok(Fork::Child) = fork() {
                        run_daemon(port, pipe_args.once.unwrap_or(false));
                    }
                }
            }
        }
    }
}

fn cmd_line_arguments() -> (PipeArgs, u16) {
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
        .arg_from_usage("--once 'do one data transfer, then quit daemon'")
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
        once: arg_matches.is_present("once").then(|| true),
    };
    (pipe_args, port)
}
