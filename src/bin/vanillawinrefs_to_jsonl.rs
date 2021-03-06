#[macro_use] extern crate log;
use std::process::exit;
use chrono::Local;
use fern::Dispatch;
use log::LevelFilter;
use clap::{App, Arg, ArgMatches};
use winvanilla::vanilla::WinFileListIterator;

#[cfg(all(windows))]
#[global_allocator]
static ALLOC: rpmalloc::RpMalloc = rpmalloc::RpMalloc;

static VERSION: &str = env!("CARGO_PKG_VERSION");


/// Create and return an App that is used to parse the command line params
/// that were specified by the user.
///
fn get_argument_parser<'a, 'b>() -> App<'a, 'b> {
    let source_arg = Arg::with_name("source")
        .short("-s")
        .long("source")
        .required(true)
        .value_name("SOURCE")
        .takes_value(true)
        .help("The source folder");

    let logging_arg = Arg::with_name("logging")
        .long("logging")
        .value_name("LOGGING LEVEL")
        .takes_value(true)
        .default_value("Info")
        .possible_values(&["Off", "Error", "Warn", "Info", "Debug", "Trace"])
        .help("Logging level to use.");

    App::new("vanilla_to_jsonl")
        .version(VERSION)
        .author("Matthew Seyer <https://github.com/forensicmatt/VanillaWindowsTools>")
        .about("Transform VanillaWindowsReference csv files into JSONL for backends.")
        .arg(source_arg)
        .arg(logging_arg)
}


/// Set the logging level from the CLI parsed parameters.
///
fn set_logging_level(matches: &ArgMatches){
    // Get the logging level supplied by the user
    let message_level = match matches.value_of("logging") {
        Some("Off") => LevelFilter::Off,
        Some("Error") => LevelFilter::Error,
        Some("Warn") => LevelFilter::Warn,
        Some("Info") => LevelFilter::Info,
        Some("Debug") => LevelFilter::Debug,
        Some("Trace") => LevelFilter::Trace,
        Some(unknown) => {
            eprintln!("Unknown log level [{}]", unknown);
            exit(-1);
        },
        None => {
            LevelFilter::Off
        }
    };

    // Create logging with debug level that prints to stderr
    // See https://docs.rs/fern/0.6.0/fern/#example-setup
    let result = Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                Local::now().format("[%Y-%m-%d %H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        .level(message_level)
        .chain(std::io::stderr())
        .apply();
    
    // Ensure that logger was dispatched
    match result {
        Ok(_) => trace!("Logging as been initialized!"),
        Err(error) => {
            eprintln!("Error initializing fern logging: {}", error);
            exit(-1);
        }
    }
}


/// The main entry point for this tool.
///
fn main() {
    let arg_parser = get_argument_parser();
    let options = arg_parser.get_matches();

    set_logging_level(&options);

    let source = options.value_of("source")
        .expect("No source folder was provided.");

    let iter = WinFileListIterator::from_path(source);
    for (location, file_list) in iter {
        let rec_iter = match file_list.into_iter() {
            Ok(i) => i,
            Err(e) => {
                error!("{}: {}", location.to_string_lossy(), e);
                continue;
            }
        };
        for record in rec_iter {
            println!("{}", record);
        }
    }
}