#[macro_use] extern crate rocket;
use std::process::exit;
use std::path::Path;
use std::net::IpAddr;
use std::str::FromStr;
use std::path::PathBuf;
use rocket::{State};
use rocket::config::Config;
use clap::{App, Arg, ArgMatches};
use chrono::Local;
use fern::Dispatch;
use log::LevelFilter;
use tempfile::TempDir;
use tantivy::{Index, IndexSettings};
use tantivy::directory::MmapDirectory;
use winvanilla::index::{
    clone_vanilla_reference_repo,
    generate_schema_from_vanilla,
    WindowsRefIndexReader,
    WindowRefIndexWriter
};
use winvanilla::service::path::{known_file_name, known_full_name, lookup_file_name, lookup_full_name};
use winvanilla::service::hash::lookup_hash;

#[cfg(all(feature = "fast-alloc", not(windows)))]
use jemallocator::Jemalloc;

#[cfg(all(feature = "fast-alloc", not(windows)))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[cfg(all(feature = "fast-alloc", windows))]
#[global_allocator]
static ALLOC: rpmalloc::RpMalloc = rpmalloc::RpMalloc;


/// Create and return an App that is used to parse the command line params
/// that were specified by the user.
///
fn get_argument_parser<'a, 'b>() -> App<'a, 'b> {
    let source_arg = Arg::with_name("source")
        .short("-s")
        .long("source")
        .required(false)
        .value_name("SOURCE")
        .takes_value(true)
        .help("The source folder (otherwise a temp folder will be used and clone the Vanilla repo.)");

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

    let address_arg = Arg::with_name("address")
        .short("-a")
        .long("address")
        .required(false)
        .value_name("IPADDRESS")
        .takes_value(true)
        .default_value("127.0.0.1")
        .help("Specific ip address.");

    let port_arg = Arg::with_name("port")
        .short("-p")
        .long("port")
        .required(false)
        .value_name("PORT")
        .takes_value(true)
        .default_value("8000")
        .help("Specific port.");

    let logging_arg = Arg::with_name("logging")
        .long("logging")
        .value_name("LOGGING LEVEL")
        .takes_value(true)
        .default_value("Info")
        .possible_values(&["Off", "Error", "Warn", "Info", "Debug", "Trace"])
        .help("Logging level to use.");

    App::new("vanilla_service")
        .author("Matthew Seyer <https://github.com/forensicmatt/VanillaWindowsTools>")
        .about("Lookup service for VanillaWindows References.")
        .arg(source_arg)
        .arg(index_arg)
        .arg(overall_memory_arg)
        .arg(address_arg)
        .arg(port_arg)
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


#[get("/")]
fn index(_index_reader: &State<WindowsRefIndexReader>) {
    
}


#[launch]
fn rocket() -> _ {
    let arg_parser = get_argument_parser();
    let options = arg_parser.get_matches();

    set_logging_level(&options);

    let overall_memory: usize = options.value_of("overall_memory")
        .map_or(100_000_000, |v| v.parse::<usize>().expect("Unable to parse overall_memory as usize!"));

    let (source, _temp_dir): (PathBuf, Option<Box<TempDir>>) = options.value_of("source")
        .map(|v|(PathBuf::from(v), None))
        .unwrap_or_else(move ||{
            let temp_dir = Box::new(
                TempDir::new()
                    .expect("Error creating temp dir for source.")
            );
            let path = temp_dir.path().to_path_buf();
            (path, Some(temp_dir))
        });

    eprintln!("source: {}", &source.to_string_lossy());

    let port = options.value_of("port")
        .map(|v| v.parse::<u16>().expect("port cannot be parsed as u16."))
        .expect("No port provided.");

    let address: IpAddr = options.value_of("address")
        .map(|v|IpAddr::from_str(v).expect("Could not parse IP Address."))
        .expect("No IpAddress provided.");

    let index_location = options.value_of("index_location")
        .expect("No index_location provided.");
    let index_location = Path::new(index_location);

    if !index_location.exists() {
        std::fs::create_dir_all(index_location)
            .expect("Error creating index_location");
    } else if !index_location.is_dir() {
        eprintln!("{} is not a directory!", index_location.to_string_lossy());
        exit(1);
    }

    match source.read_dir() {
        Ok(r) => {
            if r.count() == 0 {
                // There are no files in this directory, we should clone the repo.
                clone_vanilla_reference_repo(&source)
                    .expect("Error cloning vanilla repo from folder that has no entries.");
            }
        },
        Err(_e) => {
            eprintln!("cloning vanilla repo");
            clone_vanilla_reference_repo(&source)
                .expect("Error cloning vanilla repo.");
        }
    }
    eprintln!("init vanilla data");

    let index = match Index::open_in_dir(index_location) {
        Ok(i) => i,
        Err(_e) => {
            let schema = generate_schema_from_vanilla(&source)
                .expect("Error generating schema from vanilla path.");

            let index_directory = MmapDirectory::open(index_location)
                .expect("Error opening index_location");
            
            let settings = IndexSettings::default();

            let index = Index::create(index_directory, schema.clone(), settings)
                .expect("Error opening or creating index.");

            let mut writer = WindowRefIndexWriter::from_index(&source, index.clone(), overall_memory)
                .expect("Error creating WindowRefIndexWriter!");
        
            writer.delete_all_documents(true).expect("Error deleting documents!");
            writer.index_mt().expect("Error indexing documents!");

            index
        }
    };

    let reader = WindowsRefIndexReader::try_from(index)
        .expect("Error creating WindowsRefIndexReader!");

    let mut config = Config::release_default();
    // Set port
    config.port = port;
    // Set address
    config.address = address;

    rocket::custom(config)
        .manage(reader)
        .mount("/", routes![
            index,
            known_file_name, known_full_name,
            lookup_file_name, lookup_full_name,
            lookup_hash
        ])
}
