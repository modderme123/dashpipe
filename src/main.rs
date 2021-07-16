mod client;
mod daemon;
mod proto;
mod util;

use anyhow::{anyhow, Result};
use client::CmdArguments;
use fork::{chdir, fork, setsid, Fork};
use log::*;
use std::{thread::sleep, time::Duration};

fn main() -> Result<()> {
    env_logger::init();
    // env_logger::builder().filter_level(LevelFilter::Debug).init(); // uncomment to set logging level in code

    let CmdArguments {
        pipe_args,
        port,
        daemon_only,
        once,
    } = client::cmd_line_arguments();

    let halt = pipe_args.halt.unwrap_or(false);
    if daemon_only {
        if let Ok(Fork::Child) = fork() {
            fork_daemon(port, once)?
        }
    } else if client::client(port, &pipe_args, halt).is_err() {
        if !halt {
            match fork().unwrap() {
                Fork::Parent(_) => {
                    debug!("Starting daemon and sleeping 500ms"); // TODO fix race condition
                    sleep(Duration::from_millis(500));
                    client::client(port, &pipe_args, false)?
                }
                Fork::Child => fork_daemon(port, once)?,
            }
        }
    }
    Ok(())
}

fn fork_daemon(port: u16, once: bool) -> Result<()> {
    debug!("[daemon] starting");
    setsid().map_err(|_e| anyhow!("setsid fail"))?;
    chdir().map_err(|_e| anyhow!("chdir fail"))?;
    // close_fd().unwrap(); // comment out to enable debug logging in daemon
    if let Ok(Fork::Child) = fork() {
        daemon::run_daemon(port, once);
    }
    Ok(())
}
