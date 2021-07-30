use crate::proto::PipeArgs;
use clap::{App, Arg};

pub struct CmdArguments {
    pub pipe_args: PipeArgs,
    pub port: u16,
    pub daemon_only: bool,
    pub once: bool,
}

pub fn cmd_line_arguments() -> CmdArguments {
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
        .arg_from_usage("--replace 'replace named data if it already exists'")
        .arg_from_usage("--force-new 'store data set with a unique name'")
        .arg_from_usage("--daemon 'run daemon only'")
        .arg_from_usage("--dashboard=[dashboard] 'dashboard that will display the data'")
        .arg_from_usage("--chart=[chart] 'name of existing chart that will display the data'")
        .arg_from_usage("--no-show 'send the data without displaying it'")
        .arg_from_usage("--once 'do one data transfer, then quit daemon'")
        .arg_from_usage("--file=[file] 'read data from a file rather than standard input'")
        .arg_from_usage("--halt 'halt the daemon'")
        .get_matches();

    let port_str = arg_matches.value_of("port").unwrap();
    let port: u16 = port_str.parse().unwrap();
    let daemon_only = arg_matches.is_present("daemon");
    let pipe_args = PipeArgs {
        kind: "data".to_string(),
        name: arg_matches.value_of("name").map(str::to_owned),
        file: arg_matches.value_of("file").map(str::to_owned),
        dashboard: arg_matches.value_of("dashboard").map(str::to_owned),
        chart: arg_matches.value_of("chart").map(str::to_owned),
        no_show: arg_matches.is_present("no-show").then(|| true),
        replace: arg_matches.is_present("replace").then(|| true),
        halt: arg_matches.is_present("halt").then(|| true),
        force_new: arg_matches.is_present("force-new").then(|| true),
    };
    let once = arg_matches.is_present("once");

    CmdArguments {
        pipe_args,
        port,
        daemon_only,
        once,
    }
}
