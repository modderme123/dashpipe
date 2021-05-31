use crate::proto::{self, ping_message};
use crate::util::{ResultB, EE::MyError};
use futures_util::StreamExt;
use futures_util::{stream::FuturesUnordered, SinkExt};
use log::*;
use std::sync::Arc;
use std::{net::SocketAddr, option::Option};
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
    let addr = proto::server_address(port);
    trace!("starting server on {}", addr);
    let server_bind = TcpListener::bind(addr).await;

    match server_bind {
        Ok(server) => server_listen(server, once_only).await,
        Err(e) => warn!("{:?}", e),
    }
}

async fn server_listen(server: TcpListener, once_only: bool) {
    let connections = Connections::new();

    let mut waits = FuturesUnordered::new();
    let connections_ref = Arc::new(Mutex::new(connections));

    loop {
        tokio::select! {
            Ok(cxn) = server.accept() => {
              match handle_connect(cxn, connections_ref.clone()).await {
                Ok(Some(join_handle)) => {
                    waits.push(join_handle);
                },
                Ok(None) => {},
                Err(e) => warn!("{:?}", e),
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

/// Handle a connection to the daemon server from the browser or command line client.
async fn handle_connect(
    cxn: (TcpStream, SocketAddr),
    connections_ref: Arc<Mutex<Connections>>,
) -> ResultB<Option<JoinHandle<()>>> {
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
                                                                                // TODO try peek on ws to see if that fails
                                                                                // let p = ws.peekable();

    let reader_stream = ReaderStream::new(cli.stream);
    let message_stream = reader_stream.map(|x| Ok(Message::binary(x.unwrap().to_vec())));
    message_stream
        .forward(ws)
        .await
        .unwrap_or_else(|e| warn!("[daemon] forwarding error {:?}", e));

    debug!("[forward] done"); // this fails if the connection is closed
}

/// Connect an incoming browser web socket to an appropriate cli connection stream.
///
/// If no cli connection is available, save the browser socket in Connections.
async fn connect_ws(
    mut ws: WebSocketStream<TcpStream>,
    connections: &mut Connections,
) -> ResultB<Option<JoinHandle<()>>> {
    let header = proto::parse_browser_header(&mut ws).await?;
    debug!("[daemon] parsed ws header {:?}", header);

    let dashboard = header.current_dashboard;
    let cli_opt = matching_cli_connection(&dashboard, &mut connections.cli_connections);

    match cli_opt {
        Some(cli) => Ok(Some(tokio::spawn(forward(cli, ws)))),
        _ => {
            let ws_connection = WsConnection { dashboard, ws };
            connections.web_sockets.push(ws_connection);
            Ok(None)
        }
    }
}

/// Route an incoming cli stream to a matching browser web socket if available.
///
/// If no appropriate browser is connected, save the cli stream in Connections awaiting a future
/// browser connection
async fn connect_cli(
    mut cli: TcpStream,
    connections: &mut Connections,
) -> ResultB<Option<JoinHandle<()>>> {
    let header = proto::parse_cli_header(&mut cli).await?;
    debug!("[daemon] client header: {:?}", &header);
    let dashboard = proto::get_string_field(&header.json, "dashboard");
    let ws_opt = matching_browser_ws(&dashboard, &mut connections.web_sockets).await;

    let connection = CliConnection {
        dashboard,
        stream: cli,
        header,
    };

    match ws_opt {
        // RUST how to we write this with map & or_else?
        Some(ws) => Ok(Some(tokio::spawn(forward(connection, ws)))),
        _ => {
            connections.cli_connections.push(connection);
            Ok(None)
        }
    }
}

/// return a CliConnection for a given dashboard.
/// If no dashboard is specified, return the first CliConnection.
fn matching_cli_connection(
    dashboard: &Option<String>,
    cli_connections: &mut Vec<CliConnection>,
) -> Option<CliConnection> {
    cli_connections
        .iter()
        .position(|c| c.dashboard.eq(&dashboard))
        .or_else(|| first_index(cli_connections))
        .map(|i| cli_connections.remove(i))
}

/// return a browser websocket for a given dashboard.
/// If no dashboard is specified, return the first websocket.
async fn matching_browser_ws(
    dashboard: &Option<String>,
    web_sockets: &mut Vec<WsConnection>,
) -> Option<WebSocketStream<TcpStream>> {
    let sock = web_sockets
        .iter()
        .position(|ws| ws.dashboard.eq(&dashboard))
        .or_else(|| first_index(web_sockets))
        .map(|i| web_sockets.remove(i).ws);

    match sock {
        Some(mut ws) => {
            let pinged = ping_ws(&mut ws).await;
            match pinged {
                Ok(()) => (),
                Err(e) => {
                  // TODO loop on new socket
                  debug!("socket error: {:?}", e)
                } 
            }
            Some(ws)
        }
        None => None,
    }
}

async fn ping_ws(ws: &mut WebSocketStream<TcpStream>) -> ResultB<()> {
    ws.send(ping_message()).await?;
    let response = ws.next().await;
    match response {
        Some(Ok(pong)) => {
            // TODO verify pong message
            trace!("received pong: {:?}", pong);
            Ok(())
        }
        Some(Err(err)) => Err(err.into()),
        _ => Err(MyError("missing pong").into()),
    }
}

fn first_index<V>(vec: &[V]) -> Option<usize> {
    if !vec.is_empty() {
        Some(0)
    } else {
        None
    }
}
