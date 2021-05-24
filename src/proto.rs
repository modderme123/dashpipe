use futures_util::StreamExt;
use log::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::skip_serializing_none;
use tokio::{io::AsyncReadExt, net::TcpStream};
use tokio_tungstenite::WebSocketStream;

/** Protocol for header message to browser
 *    [version number (16 bits)] [json length 16 bits] [header: json utf8]
 */
pub struct ProtocolHeader {
    pub version: u16,
    // length: u16 // length field is in the protocol, but here we can use header_json.len() */
    pub json_buffer: Vec<u8>,
    pub json: serde_json::Value,
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
pub async fn parse_cli_header(input: &mut TcpStream) -> Option<ProtocolHeader> {
    let version = input.read_u16().await.unwrap();
    assert_eq!(version, PROTOCOL_VERSION);
    let header_size = input.read_u16().await.unwrap();
    let mut json_buffer = vec![0u8; header_size as usize];
    let header_json_bytes = input.read_exact(&mut json_buffer).await.unwrap();
    assert_eq!(header_json_bytes as u16, header_size);

    let json: Option<serde_json::Value> = serde_json::from_slice(&json_buffer).ok();
    json.map(|json| ProtocolHeader {
        version,
        json_buffer,
        json
    })
}

/** json messages sent from client to daemon to browser */
#[skip_serializing_none]
#[derive(Serialize, Deserialize)]
pub struct PipeArgs {
    pub dashboard: Option<String>, // This is only needed for client to daemon
    pub once: Option<bool>,        // This is only needed for client to daemon
    pub name: Option<String>,
    pub chart: Option<String>,
    #[serde(rename = "noShow")]
    pub no_show: Option<bool>,
    pub append: Option<bool>,
}

/** Write a protocol header into a byte array */
pub fn header_to_bytes(header: &ProtocolHeader) -> Vec<u8> {
    let json_size = header.json_buffer.len();
    let json_size_u16 = json_size as u16;
    let mut buffer = vec![0u8; json_size + 4];
    buffer[0..2].copy_from_slice(&header.version.to_be_bytes());
    buffer[2..4].copy_from_slice(&json_size_u16.to_be_bytes());
    buffer[4..].copy_from_slice(&header.json_buffer);

    buffer
}

/** the browser sends this a json message to the daemon when it connects.
   The message contains the fields:
       currentDashboard: string
       browserId: string
*/
#[derive(Debug)]
pub struct BrowserHeader {
    pub current_dashboard: Option<String>,
    pub browser_id: Option<String>,
}

/** Parse a single websocket message sent by the browser when it connects */
pub async fn parse_browser_header(ws: &mut WebSocketStream<TcpStream>) -> Option<BrowserHeader> {
    let header_opt = read_ws_header(ws).await;
    header_opt.map(|header_value| BrowserHeader {
        current_dashboard: get_string_field(&header_value, "currentDashboard"),
        browser_id: get_string_field(&header_value, "browserId"),
    })
}

/** Return a string field from a serde_json object.
 *
 * Returns None if the provided value is not an object, or the specified field doesn't exist on the object,
 * or the field value doesn't contain a string, returns None. */
pub fn get_string_field(value: &serde_json::Value, field: &str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(str::to_string)
}

/** Read the header sent by the browser to the daemon */
async fn read_ws_header(ws: &mut WebSocketStream<TcpStream>) -> Option<serde_json::Value> {
    let next_msg = (*ws).next().await;
    match next_msg {
        Some(Ok(msg)) => {
            debug!("[daemon] ws message {:?}", msg);
            msg.to_text()
                .map(|text| serde_json::from_str(text).unwrap())
                .ok()
        }
        _ => {
            warn!("[daemon] no message received {:?}", next_msg);
            None
        }
    }
}
