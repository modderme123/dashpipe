use crate::proto;
use futures_util::StreamExt;
use futures_util::{stream::FuturesUnordered, SinkExt};
use log::*;
use std::hash::Hash;
use std::sync::Arc;
use std::{collections::HashMap, net::SocketAddr, option::Option};
use tokio::sync::Mutex;
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use tokio_util::io::ReaderStream;

#[derive(Debug, Default)]
struct Connections {
    web_sockets: Vec<WsConnection>,
    cli_connections: Vec<CliConnection>,
}

impl Connections {
    fn new() -> Connections {
        Connections::default()
    }
}

#[derive(Debug)]
struct CliConnection {
    dashboard: Option<String>,
    stream: TcpStream,
    header: proto::ProtocolHeader,
}
#[derive(Debug)]
struct WsConnection {
    dashboard: Option<String>,
    ws: WebSocketStream<TcpStream>,
}

/** A server that listens for connections from command line clients and web browsers.
 * Data from clients is sent to browsers via handle_connect*/
#[tokio::main]
pub async fn run_daemon(port: u16, once_only: bool) {
    let server = TcpListener::bind(proto::server_address(port))
        .await
        .unwrap();
    let connections = Connections::new();

    let mut waits = FuturesUnordered::new();
    let connections_ref = Arc::new(Mutex::new(connections));

    loop {
        tokio::select! {
            Ok(cxn) = server.accept() => {
              let fwd = handle_connect(cxn, connections_ref.clone()).await;
              if let Some(join_handle) = fwd {
                  waits.push(join_handle);
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
    connections_ref: Arc<Mutex<Connections>>,
) -> Option<JoinHandle<()>> {
    let (stream, _) = cxn;
    let mut connections = connections_ref.lock().await;

    trace!(
        "[daemon] handle_connect: cli connections: {}, ws: {}",
        connections.cli_connections.len(),
        connections.web_sockets.len()
    );

    let mut front = [0u8; 14];
    stream.peek(&mut front).await.expect("peek failed");
    if front == *b"GET / HTTP/1.1" {
        let ws = accept_async(stream).await.unwrap();
        connect_ws(ws, &mut connections).await
    } else {
        connect_cli(stream, &mut connections).await
    }
}

/** Forward a protocol stream from a command line client to a browser websocket.
 * The protocol header is parsed from the command line client and sent in a single
 * websocket message. Data follows in one or more websocket messages. */
async fn forward(cli: CliConnection, mut ws: WebSocketStream<TcpStream>) {
    let header_buffer = proto::header_to_bytes(&cli.header);
    let header_message = Message::binary(header_buffer);
    ws.send(header_message)
        .await
        .unwrap_or_else(|e| warn!("[daemon] header forwarding error {:?}", e)); // note this doesn't fail, even if the connection is closed

    let reader_stream = ReaderStream::new(cli.stream);
    let message_stream = reader_stream.map(|x| Ok(Message::binary(x.unwrap().to_vec())));
    message_stream
        .forward(ws)
        .await
        .unwrap_or_else(|e| warn!("[daemon] forwarding error {:?}", e));

    debug!("[forward] done");
}

async fn connect_ws(
    mut ws: WebSocketStream<TcpStream>,
    connections: &mut Connections,
) -> Option<JoinHandle<()>> {
    let header_opt = proto::parse_browser_header(&mut ws).await;
    header_opt.and_then(|header| {
        debug!("[daemon] parsed ws header {:?}", header);

        let dashboard = header.current_dashboard;
        let cli_opt = get_cli(&dashboard, &mut connections.cli_connections);

        match cli_opt {
            Some(cli) => Some(tokio::spawn(forward(cli, ws))),
            _ => {
                let ws_connection = WsConnection { dashboard, ws };
                connections.web_sockets.push(ws_connection);
                None
            }
        }
    })
}

fn get_cli(
    dashboard: &Option<String>,
    cli_connections: &mut Vec<CliConnection>,
) -> Option<CliConnection> {
    dashboard
        .and_then(|d| {
            let found = cli_connections
                .iter()
                .position(|c| c.dashboard.eq(dashboard));
            found.map(|i| cli_connections.remove(i))
        })
        .or_else(|| cli_connections.iter_mut().next().map(|r| *r))
}

fn get_ws(
    dashboard: &Option<String>,
    web_sockets: &mut Vec<WsConnection>,
) -> Option<WebSocketStream<TcpStream>> {
    dashboard
        .and_then(|d| {
            let found = web_sockets.iter().position(|ws| ws.dashboard.eq(dashboard));
            found.map(|i| web_sockets.remove(i).ws)
        })
        .or_else(|| web_sockets.iter_mut().next().map(|w| w.ws))
}

async fn connect_cli(mut cli: TcpStream, connections: &mut Connections) -> Option<JoinHandle<()>> {
    let header_opt = proto::parse_cli_header(&mut cli).await;
    header_opt.and_then(|header| {
        debug!("[daemon] client header: {:?}", &header);
        let dashboard = proto::get_string_field(&header.json, "dashboard");
        let ws_opt = get_ws(&dashboard, &mut connections.web_sockets);

        let connection = CliConnection {
            dashboard: dashboard,
            stream: cli,
            header,
        };

        match ws_opt {
            // RUST how to we write this with map & or_else?
            Some(ws) => Some(tokio::spawn(forward(connection, ws))),
            _ => {
                connections.cli_connections.push(connection);
                None
            }
        }
    })
}

/// Remove one element from a hash map if it is non-empty
fn remove_one<K, V>(hash: &mut HashMap<K, V>) -> Option<V>
where
    K: Hash + Eq + Clone,
{
    hash.keys().next().cloned().and_then(|k| hash.remove(&k))
}
