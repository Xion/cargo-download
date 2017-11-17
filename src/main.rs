//!
//! cargo-download
//!

             extern crate ansi_term;
#[macro_use] extern crate clap;
             extern crate conv;
#[macro_use] extern crate derive_error;
             extern crate exitcode;
             extern crate isatty;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate maplit;
             extern crate reqwest;
             extern crate serde_json;
             extern crate slog_envlogger;
             extern crate slog_stdlog;
             extern crate slog_stream;
             extern crate time;

// `slog` must precede `log` in declarations here, because we want to simultaneously:
// * use the standard `log` macros
// * be able to initialize the slog logger using slog macros like o!()
#[macro_use] extern crate slog;
#[macro_use] extern crate log;


mod args;
mod logging;


use std::io::{self, Read, Write};
use std::error::Error;
use std::process::exit;

use log::LogLevel::*;
use reqwest::header::ContentLength;
use serde_json::Value as Json;

use args::ArgsError;


lazy_static! {
    /// Application / package name, as filled out by Cargo.
    static ref NAME: &'static str = option_env!("CARGO_PKG_NAME")
        .unwrap_or("cargo-download");

    /// Application version, as filled out by Cargo.
    static ref VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");
}


fn main() {
    let opts = args::parse().unwrap_or_else(|e| {
        print_args_error(e).unwrap();
        exit(exitcode::USAGE);
    });

    logging::init(opts.verbosity).unwrap();
    log_signature();

    let version = get_newest_version(&opts.crate_).unwrap_or_else(|e| {
        error!("Failed to get the newest version of crate {}: {}", opts.crate_, e);
        exit(exitcode::TEMPFAIL);
    });
    let crate_bytes = download_crate(&opts.crate_, &version).unwrap_or_else(|e| {
        error!("Failed to download crate `{}=={}`: {}", opts.crate_, version, e);
        exit(exitcode::TEMPFAIL);
    });

    // TODO: add option for writing somewhere else than stdout
    // TODO: add option for extracting the gzipped crate as a directory
    io::stdout().write(&crate_bytes).unwrap();
}

// Print an error that may occur while parsing arguments.
fn print_args_error(e: ArgsError) -> io::Result<()> {
    match e {
        ArgsError::Parse(ref e) =>
            // In case of generic parse error,
            // message provided by the clap library will be the usage string.
            writeln!(&mut io::stderr(), "{}", e.message),
    }
}

/// Log the program name, version, and other metadata.
#[inline]
fn log_signature() {
    if log_enabled!(Info) {
        let version = VERSION.map(|v| format!("v{}", v))
            .unwrap_or_else(|| "<UNKNOWN VERSION>".into());
        info!("{} {}", *NAME, version);
    }
}


const CRATES_API_ROOT: &'static str = "https://crates.io/api/v1/crates";

/// Fetch the newest version of given crate from crates.io.
fn get_newest_version(crate_: &str) -> Result<String, Box<Error>> {
    let versions_url = format!("{}/{}/versions", CRATES_API_ROOT, crate_);
    debug!("Fetching latest version of crate `{}` from {}", crate_, versions_url);
    let response: Json = reqwest::get(&versions_url)?.json()?;
    response.pointer("/versions/0/num")
        .and_then(|v| v.as_str())
        .map(|v| { info!("Latest version of crate {} is {}", crate_, v); v.to_owned() })
        .ok_or_else(|| format!("malformed response from {}", versions_url).into())
}

/// Download given crate and return it as a vector of gzipped bytes.
fn download_crate(crate_: &str, version: &str) -> Result<Vec<u8>, Box<Error>> {
    let download_url = format!("{}/{}/{}/download", CRATES_API_ROOT, crate_, version);
    debug!("Downloading crate `{}=={}` from {}", crate_, version, download_url);
    let mut response = reqwest::get(&download_url)?;

    let content_length = response.headers().get::<ContentLength>().map(|&cl| *cl);
    trace!("Download size: {}",
        content_length.map(|cl| format!("{} bytes", cl)).unwrap_or_else(|| "<unknown>".into()));
    let mut bytes = match content_length {
        Some(cl) => Vec::with_capacity(cl as usize),
        None => Vec::new(),
    };
    response.read_to_end(&mut bytes)?;

    info!("Crate `{}=={}` downloaded successfully", crate_, version);
    Ok(bytes)
}
