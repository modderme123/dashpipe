extern crate clap;
use clap::App;
use std::net::{TcpListener, TcpStream};
use std::{
    collections::HashMap,
    io::{self, BufRead},
};
use std::{io::prelude::*, thread::sleep, time::Duration};
use tungstenite::Message;
use tungstenite::{server::accept, WebSocket};

use fork::{chdir, close_fd, fork, setsid, Fork};

fn forward(mut a: TcpStream, mut b: WebSocket<TcpStream>) {
    loop {
        let mut buffer = String::new();
        a.read_to_string(&mut buffer).unwrap();
        b.write_message(Message::text(buffer)).unwrap();
    }
}

fn run_daemon() {
    let server = TcpListener::bind("127.0.0.1:3030").unwrap();

    let mut open_sockets: HashMap<&str, WebSocket<TcpStream>> = HashMap::new();
    let mut cli_sockets: HashMap<&str, TcpStream> = HashMap::new();

    for stream in server.incoming() {
        let stream = stream.unwrap();
        match accept(stream.try_clone().unwrap()) {
            Ok(mut ws) => {
                let name = "tmpname1232";
                ws.write_message(Message::text("welcome")).unwrap();
                if let Some(socket) = cli_sockets.remove(name) {
                    std::thread::spawn(move || forward(socket, ws));
                } else {
                    open_sockets.insert(name, ws);
                }
            }
            Err(_) => {
                let name = "tmpname1232";
                if let Some(ws) = open_sockets.remove(name) {
                    std::thread::spawn(move || forward(stream, ws));
                } else {
                    cli_sockets.insert(name, stream);
                }
            }
        }
    }
}

fn client(mut stream: TcpStream) {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        stream.write(line.unwrap().as_bytes()).unwrap();
    }
}

fn main() {
    App::new("DashPipe")
        .version("0.1")
        .author("Modder Me <modderme123@gmail.com>")
        .about("Pipes command line data to dashberry.ml")
        .get_matches();

    let stream = TcpStream::connect("127.0.0.1:3030");

    match stream {
        Ok(stream) => {
            println!("Daemon already running");
            client(stream);
        }
        Err(_) => match fork().unwrap() {
            Fork::Parent(_) => {
                println!("Starting daemon and sleeping 500ms");
                sleep(Duration::from_millis(500));
                let stream = TcpStream::connect("127.0.0.1:3030").unwrap();
                client(stream);
            }
            Fork::Child => {
                println!("[daemon] starting");
                if let Ok(_) = setsid() {
                    chdir().unwrap();
                    close_fd().unwrap();
                    if let Ok(Fork::Child) = fork() {
                        run_daemon();
                    }
                    // Do we care about the parent here??
                }
            }
        },
    }
}
