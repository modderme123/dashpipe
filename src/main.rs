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
    let mut buffer = [0u8; 256];
    loop {
        let bytes = a.read(&mut buffer[..]).unwrap();
        if bytes > 0 {
            b.write_message(Message::binary(buffer[0..bytes].to_vec()))
                .unwrap();
        }
    }
}

fn run_daemon() {
    let server = TcpListener::bind("127.0.0.1:3030").unwrap();

    let mut web_sockets: HashMap<&str, WebSocket<TcpStream>> = HashMap::new();
    let mut cli_sockets: HashMap<&str, TcpStream> = HashMap::new();

    for stream in server.incoming() {
        let stream = stream.unwrap();
        let mut front = [0u8; 14];
        stream.peek(&mut front).expect("peek failed");
        if front == b"GET / HTTP/1.1".to_owned() {
            let ws = accept(stream.try_clone().unwrap()).unwrap();

            let name = "tmpname1232";
            if let Some(socket) = cli_sockets.remove(name) {
                std::thread::spawn(move || forward(socket, ws));
            } else {
                web_sockets.insert(name, ws);
            }
        } else {
            let name = "tmpname1232";
            if let Some(ws) = web_sockets.remove(name) {
                std::thread::spawn(move || forward(stream, ws));
            } else {
                cli_sockets.insert(name, stream);
            }
        }
    }
}

fn client(mut stream: TcpStream) {
    stream.write(b"welcome, testing 123").unwrap();
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
                }
            }
        },
    }
}
