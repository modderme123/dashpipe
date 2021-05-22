mod client;
mod daemon;
mod proto;

use fork::{chdir, fork, setsid, Fork};
use log::*;
use std::{thread::sleep, time::Duration};

fn main() {
    env_logger::init();
    // env_logger::builder().filter_level(LevelFilter::Debug).init(); // uncomment to set logging level in code

    let (pipe_args, port) = client::cmd_line_arguments();

    if client::client(port, &pipe_args).is_err() {
        match fork().unwrap() {
            Fork::Parent(_) => {
                debug!("Starting daemon and sleeping 500ms"); // TODO fix race condition
                sleep(Duration::from_millis(500));
                client::client(port, &pipe_args).unwrap();
            }
            Fork::Child => {
                debug!("[daemon] starting");
                if setsid().is_ok() {
                    chdir().unwrap();
                    // close_fd().unwrap(); // comment out to enable debug logging in daemon
                    if let Ok(Fork::Child) = fork() {
                        daemon::run_daemon(port, pipe_args.once.unwrap_or(false));
                    }
                }
            }
        }
    }
}
