mod client;
mod cmd_line;
mod daemon;
mod proto;
mod util;

use anyhow::{anyhow, Result};
use cmd_line::{cmd_line_arguments};
use fork::{chdir, fork, setsid, Fork};
use log::*;
use std::{thread::sleep, time::Duration};

fn main() -> Result<()> {
    env_logger::init();
    // env_logger::builder().filter_level(LevelFilter::Debug).init(); // uncomment to set logging level in code

    let command_args = cmd_line_arguments();
    let once = command_args.once;
    let halt = command_args.pipe_args.halt.unwrap_or(false);
    let port = command_args.port;
    let daemon_only = command_args.daemon_only;

    if daemon_only {
        if let Ok(Fork::Child) = fork() {
            fork_daemon(port, once)?
        }
    } else if client::client(&command_args).is_err() {
        if !halt {
            match fork().unwrap() {
                Fork::Parent(_) => {
                    debug!("Starting daemon and sleeping 500ms"); // TODO fix race condition
                    sleep(Duration::from_millis(500));
                    client::client(&command_args)?
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
