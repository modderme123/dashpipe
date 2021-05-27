mod client;
mod daemon;
mod proto;
mod util;

use client::CmdArguments;
use fork::{chdir, fork, setsid, Fork};
use log::*;
use std::{thread::sleep, time::Duration};

fn main() {
    env_logger::init();
    // env_logger::builder().filter_level(LevelFilter::Debug).init(); // uncomment to set logging level in code

    let CmdArguments {
        pipe_args,
        port,
        daemon_only,
        once,
    } = client::cmd_line_arguments();

    if daemon_only {
        if let Ok(Fork::Child) = fork() {
            fork_daemon(port, once);
        }
    } else {
        if client::client(port, &pipe_args).is_err() {
            match fork().unwrap() {
                Fork::Parent(_) => {
                    debug!("Starting daemon and sleeping 500ms"); // TODO fix race condition
                    sleep(Duration::from_millis(500));
                    client::client(port, &pipe_args).unwrap();
                }
                Fork::Child => {
                    fork_daemon(port, once);
                }
            }
        }
    }
}

fn fork_daemon(port: u16, once: bool) {
    debug!("[daemon] starting");
    if setsid().is_ok() {
        chdir().unwrap();
        // close_fd().unwrap(); // comment out to enable debug logging in daemon
        if let Ok(Fork::Child) = fork() {
            daemon::run_daemon(port, once);
        }
    }
}
