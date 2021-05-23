use crate::proto::{self, PipeArgs};
use clap::{App, Arg};
use futures_util::{io::AsyncWriteExt as AsyncWriteExt2, StreamExt};
use log::*;
use std::error::Error;
use tokio::{
    self,
    io::{self, AsyncWriteExt},
    net::TcpStream,
};
use tokio_util::{compat::TokioAsyncWriteCompatExt, io::ReaderStream};

#[tokio::main]
pub async fn client(port: u16, args: &PipeArgs) -> Result<(), Box<dyn Error>> {
    let address = proto::server_address(port);
    let mut stream = TcpStream::connect(&address).await?;
    debug!("[client] Connected to daemon {}", address);

    let header = proto::client_header_bytes(&args);
    stream.write(&header).await.expect("Couldn't send header");

    let stdin = ReaderStream::new(io::stdin());
    stdin
        .forward(stream.compat_write().into_sink())
        .await
        .expect("Couldn't forward stream");
    Ok(())
}

pub fn cmd_line_arguments() -> (PipeArgs, u16) {
    let arg_matches = App::new("DashPipe")
        .version("0.1")
        .author("Modder Me <modderme123@gmail.com>")
        .about("Pipes command line data to dashberry.ml")
        .arg(
            Arg::with_name("port")
                .long("port")
                .help("localhost port for daemon")
                .value_name("port")
                .takes_value(true)
                .default_value("3030"),
        )
        .arg_from_usage("--name=[name] 'name for uploaded data set'")
        .arg_from_usage("--dashboard=[dashboard] 'name of dashboard that will display the data'")
        .arg_from_usage("--chart=[chart] 'name of existing chart that will display the data'")
        .arg_from_usage("--no-show 'send the data without displaying it'")
        .arg_from_usage("--append 'append data to an existing chart'")
        .arg_from_usage("--once 'do one data transfer, then quit daemon'")
        .get_matches();


    let port_str = arg_matches.value_of("port").unwrap();
    let port: u16 = port_str.parse().unwrap();
    let pipe_args = PipeArgs {
        name: arg_matches.value_of("name").map(str::to_owned),
        dashboard: arg_matches.value_of("dashboard").map(str::to_owned),
        chart: arg_matches.value_of("dashboard").map(str::to_owned),
        no_show: arg_matches.is_present("no-show").then(|| true),
        append: arg_matches.is_present("append").then(|| true),
        once: arg_matches.is_present("once").then(|| true),
    };
    (pipe_args, port)
}
