extern crate clap;
use clap::App;
use std::io::{self, BufRead};
use std::net::TcpListener;
use tungstenite::server::accept;
use tungstenite::Message;

fn main() {
    App::new("DashPipe")
        .version("0.1")
        .author("Modder Me <modderme123@gmail.com>")
        .about("Pipes command line data to dashberry.ml")
        .get_matches();

    let server = TcpListener::bind("127.0.0.1:3030").unwrap();
    for stream in server.incoming() {
        let mut websocket = accept(stream.unwrap()).unwrap();
        websocket.write_message(Message::text("welcome")).unwrap();
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = line.expect("Could not read line from standard in");
            websocket.write_message(Message::text(line + "\n")).unwrap();
        }
        break;
    }
}
