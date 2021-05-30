mod client;
mod daemon;
mod proto;
mod util;

use client::CmdArguments;
use fork::{chdir, fork, setsid, Fork};
use log::*;
use std::{thread::sleep, time::Duration};
use util::ResultB;
use util::EE::MyError;

use crate::util::box_error;

fn main() -> ResultB<()> {
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
            fork_daemon(port, once)?
        }
    } else if client::client(port, &pipe_args).is_err() {
        match fork().unwrap() {
            Fork::Parent(_) => {
                debug!("Starting daemon and sleeping 500ms"); // TODO fix race condition
                sleep(Duration::from_millis(500));
                client::client(port, &pipe_args)?
            }
            Fork::Child => fork_daemon(port, once)?,
        }
    }
    // result.unwrap_or_else(|e| warn!("{:?}", e));
    Ok(())
}

fn fork_daemon(port: u16, once: bool) -> ResultB<()> {
    debug!("[daemon] starting");
    setsid().map_err(|_e| box_error(MyError("setsid fail")))?;
    chdir().map_err(|_e| box_error(MyError("chdir fail")))?;
    // close_fd().unwrap(); // comment out to enable debug logging in daemon
    if let Ok(Fork::Child) = fork() {
        daemon::run_daemon(port, once);
    }
    Ok(())
}
