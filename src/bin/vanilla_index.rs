#[macro_use] extern crate log;
use std::process::exit;
use std::path::Path;
use chrono::Local;
use fern::Dispatch;
use log::LevelFilter;
use clap::{App, Arg, ArgMatches};
use tantivy::Index;
use tantivy::directory::MmapDirectory;
use winvanilla::index::{generate_schema_from_vanilla, WindowRefIndexWriter};

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
    
    let index_arg = Arg::with_name("index_location")
        .short("-i")
        .long("index-location")
        .required(true)
        .value_name("INDEX_LOCATION")
        .takes_value(true)
        .help("The index folder");

    let overall_memory_arg = Arg::with_name("overall_memory")
        .short("-m")
        .long("overall_memory")
        .required(false)
        .default_value("100000000")
        .value_name("MEMORY_SIZE")
        .takes_value(true)
        .help("The total target memory usage that will be split between writer threads.");

    let logging_arg = Arg::with_name("logging")
        .long("logging")
        .value_name("LOGGING LEVEL")
        .takes_value(true)
        .default_value("Info")
        .possible_values(&["Off", "Error", "Warn", "Info", "Debug", "Trace"])
        .help("Logging level to use.");

    App::new("vanilla_index")
        .version(VERSION)
        .author("Matthew Seyer <https://github.com/forensicmatt/VanillaWindowsTools>")
        .about("Index VanillaWindowsReference files.")
        .arg(source_arg)
        .arg(index_arg)
        .arg(overall_memory_arg)
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

    let overall_memory: usize = options.value_of("overall_memory")
        .map_or(100_000_000, |v| v.parse::<usize>().expect("Unable to parse overall_memory as usize!"));

    let source = options.value_of("source")
        .expect("No source folder was provided.");

    let index_location = options.value_of("index_location")
        .expect("No index_location folder was provided.");
    let index_location = Path::new(index_location);

    let schema = generate_schema_from_vanilla(source)
        .expect("Error generating schema from vanilla path.");

    if !index_location.exists() {
        std::fs::create_dir_all(index_location)
            .expect("Error creating index_location");
    } else if !index_location.is_dir() {
        eprintln!("{} is not a directory!", index_location.to_string_lossy());
    }

    let index_directory = MmapDirectory::open(index_location)
        .expect("Error opening index_location");

    let index = Index::open_or_create(index_directory, schema)
        .expect("Error opening or creating index.");

    let mut writer = WindowRefIndexWriter::from_index(source, index, overall_memory)
        .expect("Error creating WindowRefIndexWriter!");

    writer.delete_all_documents(true).expect("Error deleting documents!");
    writer.index_mt().expect("Error indexing documents!");
}