use futures_util::StreamExt;
use log::*;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use tokio::{io::AsyncReadExt, net::TcpStream};
use tokio_tungstenite::WebSocketStream;

/** Protocol for header message to browser
 *    [version number (16 bits)] [json length 16 bits] [header: json utf8]
 */
pub struct ProtocolHeader {
    pub version: u16,
    /* length: u16 // length field is in the protocol, but here we can use header_json.len() */
    pub header_json: Vec<u8>,
}

const PROTOCOL_VERSION: u16 = 1u16;
const PROTOCOL_VERSION_BYTES: [u8; 2] = PROTOCOL_VERSION.to_be_bytes();

pub fn server_address(port: u16) -> String {
    format!("localhost:{}", port)
}

/** Write command line arguments into a protocol header. */
pub fn client_header_bytes(args: &PipeArgs) -> Vec<u8> {
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

/** Consume a protocol header from a command line client tcp stream.  */
pub async fn parse_header(input: &mut TcpStream) -> ProtocolHeader {
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

/** json messages sent from client to daemon to browser */
#[skip_serializing_none]
#[derive(Serialize, Deserialize)]
pub struct PipeArgs {
    pub dashboard: Option<String>, // This is only needed for client to daemon
    pub once: Option<bool>,        // This is only needed for client to daemon
    pub title: Option<String>,
    pub chart: Option<String>,
    pub no_show: Option<bool>,
    pub append: Option<bool>,
}

/** Write a protocol header into a byte array */
pub fn header_to_bytes(header: &ProtocolHeader) -> Vec<u8> {
    let json_size = header.header_json.len();
    let json_size_u16 = json_size as u16;
    let mut buffer = vec![0u8; json_size + 4];
    buffer[0..2].copy_from_slice(&header.version.to_be_bytes());
    buffer[2..4].copy_from_slice(&json_size_u16.to_be_bytes());
    buffer[4..].copy_from_slice(&header.header_json);

    buffer
}

#[derive(Debug)]
pub struct BrowserHeader {
    pub current_dashboard: Option<String>,
    pub browser_id: Option<String>,
}

pub async fn parse_browser_header(ws: &mut WebSocketStream<TcpStream>) -> Option<BrowserHeader> {
    let header_opt = read_ws_header(ws).await;
    return match header_opt {
        Some(header_value) => match header_value.as_object() {
            Some(header_obj) => {
                Some(BrowserHeader {
                    current_dashboard: Some("foo".to_owned()),
                    browser_id: Some("bar".to_owned()),
                })
            }
            _ => None,
        },
        _ => None,
    };
}

async fn read_ws_header(ws: &mut WebSocketStream<TcpStream>) -> Option<serde_json::Value> {
    let next_msg = (*ws).next().await;
    return match next_msg {
        Some(Ok(msg)) => {
            debug!("ws message {:?}", msg);
            msg.to_text()
                .map(|text| {
                    return serde_json::from_str(text).unwrap();
                })
                .map_or(None, Some)
        }
        _ => {
            debug!("[daemon] no message received {:?}", next_msg);
            None
        }
    };
}
