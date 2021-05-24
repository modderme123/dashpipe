use crate::proto;
use futures_util::stream::FuturesUnordered;
use futures_util::SinkExt;
use futures_util::StreamExt;
use log::*;
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
    /// webssockets indexed by dashboard name or "" if unspecified
    web_sockets: HashMap<String, WebSocketStream<TcpStream>>,

    /// cli sockets indexed by target dashboard name or "" if unspecified
    cli_sockets: HashMap<String, TcpStream>,
}

impl Connections {
    fn new() -> Connections {
        Connections::default()
    }
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
    let (mut stream, _) = cxn;
    let mut connections = connections_ref.lock().await;

    let mut front = [0u8; 14];
    stream.peek(&mut front).await.expect("peek failed");
    if front == *b"GET / HTTP/1.1" {
        let mut ws = accept_async(stream).await.unwrap();
        connect_ws(&mut ws, &mut connections).await
    } else {
        connect_cli(&mut stream, &mut connections).await
    }
}

/** Forward a protocol stream from a command line client to a browser websocket.
 * The protocol header is parsed from the command line client and sent in a single
 * websocket message. Data follows in one or more websocket messages. */
async fn forward(a: TcpStream, b: WebSocketStream<TcpStream>) {
    // let header = proto::parse_cli_header(a).await;
    // let header_buffer = proto::header_to_bytes(&header);
    // let header_message = Message::binary(header_buffer);
    // b.send(header_message).await.unwrap();

    let reader_stream = ReaderStream::new(a);
    let message_stream = reader_stream.map(|x| Ok(Message::binary(x.unwrap().to_vec())));
    message_stream.forward(b).await.unwrap();
    debug!("[forward] done");
}

async fn connect_ws(
    ws: &mut WebSocketStream<TcpStream>,
    connections: &mut Connections,
) -> Option<JoinHandle<()>> {
    let header_opt = proto::parse_browser_header(ws).await;
    let Connections {
        cli_sockets,
        web_sockets,
    } = connections;
    header_opt.and_then(|header| {
        debug!("[daemon] parsed header {:?}", header);

        let dash_opt = header.current_dashboard;

        let cli_opt = dash_opt
            .and_then(|d| cli_sockets.remove(d.as_ref()))
            .as_ref()
            .or_else(|| cli_sockets.values().next());

        cli_opt
            .map(|cli| tokio::spawn(forward(*cli, *ws)))
            .or_else(|| {
                let dash_name = dash_opt.unwrap_or_else(|| "".to_owned());
                connections.web_sockets.insert(dash_name, *ws);
                None
            })
    })
}

async fn connect_cli(cli: &mut TcpStream, connections: &mut Connections) -> Option<JoinHandle<()>> {
    let Connections {
        cli_sockets,
        web_sockets,
    } = connections;
    let header_opt = proto::parse_cli_header(cli).await;
    header_opt.and_then(|header| {
        let dash_opt = proto::get_string_field(&header.json, "dashboard");
        let ws_opt = dash_opt
            .and_then(|d| web_sockets.remove(d.as_ref()))
            .as_ref()
            .or_else(|| web_sockets.values().next());
        ws_opt
            .map(|ws| tokio::spawn(forward(*cli, *ws)))
            .or_else(|| {
                let dash_name = dash_opt.unwrap_or_else(|| "".to_owned());
                cli_sockets.insert(dash_name, *cli);
                None
            })
    })
}
// let name = "tmpname1232".to_owned();
// if let Some(ws) = connections.web_sockets.remove(&name) {
//     let handle = tokio::spawn(forward(cli, ws));
//     Some(handle)
// } else {
//     connections.cli_sockets.insert(name, stream);
//     None
// }
