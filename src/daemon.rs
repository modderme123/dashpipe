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
    web_sockets: HashMap<String, WebSocketStream<TcpStream>>,
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
    connections_ref:Arc<Mutex<Connections>>,
) -> Option<JoinHandle<()>> {
    let (stream, _) = cxn;
    let mut connections = connections_ref.lock().await;

    let mut front = [0u8; 14];
    stream.peek(&mut front).await.expect("peek failed");
    if front == *b"GET / HTTP/1.1" {
        let ws = accept_async(stream).await.unwrap();

        let name = "tmpname1232".to_owned();
        if let Some(socket) = connections.cli_sockets.remove(&name) {
            let handle = tokio::spawn(forward(socket, ws));
            return Some(handle);
        } else {
            connections.web_sockets.insert(name, ws);
            return None;
        }
    } else {
        let name = "tmpname1232".to_owned();
        if let Some(ws) = connections.web_sockets.remove(&name) {
            let handle = tokio::spawn(forward(stream, ws));
            return Some(handle);
        } else {
            connections.cli_sockets.insert(name, stream);
            return None;
        }
    }
}


/** Forward a protocol stream from a command line client to a browser websocket.
 * The protocol header is parsed from the command line client and sent in a single
 * websocket message. Data follows in one or more websocket messages. */
async fn forward(mut a: TcpStream, mut b: WebSocketStream<TcpStream>) {
    let header = proto::parse_header(&mut a).await;
    let header_buffer = proto::header_to_bytes(&header);
    let header_message = Message::binary(header_buffer);
    b.send(header_message).await.unwrap();

    let reader_stream = ReaderStream::new(a);
    let message_stream = reader_stream.map(|x| Ok(Message::binary(x.unwrap().to_vec())));
    message_stream.forward(b).await.unwrap();
    debug!("[forward] done");
}