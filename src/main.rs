//!
//! cargo-download
//!

             extern crate ansi_term;
#[macro_use] extern crate clap;
             extern crate conv;
#[macro_use] extern crate derive_error;
             extern crate exitcode;
             extern crate flate2;
             extern crate isatty;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate maplit;
             extern crate reqwest;
             extern crate semver;
             extern crate serde_json;
             extern crate slog_envlogger;
             extern crate slog_stdlog;
             extern crate slog_stream;
             extern crate time;
             extern crate tar;

// `slog` must precede `log` in declarations here, because we want to simultaneously:
// * use the standard `log` macros
// * be able to initialize the slog logger using slog macros like o!()
#[macro_use] extern crate slog;
#[macro_use] extern crate log;


mod args;
mod logging;


use std::borrow::Cow;
use std::fs;
use std::io::{self, Read, Write};
use std::error::Error;
use std::path::PathBuf;
use std::process::exit;

use log::LogLevel::*;
use reqwest::header::ContentLength;
use semver::Version;
use serde_json::Value as Json;

use args::{ArgsError, Crate, Output};


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

    let version = match opts.crate_.exact_version() {
        Some(v) => {
            debug!("Exact crate version given in arguments, not querying crates.io");
            Cow::Borrowed(v)
        }
        None => Cow::Owned(get_newest_version(&opts.crate_).unwrap_or_else(|e| {
            error!("Failed to get the newest version of crate {}: {}", opts.crate_, e);
            exit(exitcode::TEMPFAIL);
        })),
    };
    let crate_bytes = download_crate(&opts.crate_.name(), &version).unwrap_or_else(|e| {
        error!("Failed to download crate `{}=={}`: {}", opts.crate_.name(), version, e);
        exit(exitcode::TEMPFAIL);
    });

    if opts.extract {
        // Extract to a directory named $CRATE-$VERSION
        // Due to how crate archives are structured (they contain
        // single top-level directory) this is done automatically
        // if you simply extract them in $CWD.
        let dir: PathBuf = format!("./{}-{}", opts.crate_.name(), version).into();
        debug!("Extracting crate archive to {}/", dir.display());
        let gzip = flate2::read::GzDecoder::new(&crate_bytes[..]).unwrap();
        let mut archive = tar::Archive::new(gzip);
        match archive.unpack(".") {
            Ok(_) => {
                // If -x option was passed, we need to move the extracted directory
                // to wherever the user wanted.
                let mut dir = dir;
                if let Some(&Output::Path(ref p)) = opts.output.as_ref() {
                    fs::rename(&dir, p).unwrap_or_else(|e| {
                        error!("Failed to move extracted archive from {} to {}: {}",
                            dir.display(), p.display(), e);
                        exit(exitcode::IOERR)
                    });
                    dir = p.clone();
                }
                info!("Crate content extracted to {}/", dir.display());
            }
            Err(e) => {
                error!("Couldn't extract crate to {}/: {}", dir.display(), e);
                exit(exitcode::TEMPFAIL)
            }
        }
    } else {
        let output = opts.output.as_ref().unwrap_or(&Output::Stdout);
        match output {
            &Output::Stdout => { io::stdout().write(&crate_bytes).unwrap(); }
            &Output::Path(ref p) => {
                let mut file = fs::OpenOptions::new()
                    .write(true).create(true)
                    .open(p).unwrap_or_else(|e| {
                        error!("Failed to open output file {}: {}", p.display(), e);
                        exit(exitcode::IOERR)
                    });
                file.write(&crate_bytes).unwrap();
                info!("Crate's archive written to {}", p.display());
            }
        }
    }
}

// Print an error that may occur while parsing arguments.
fn print_args_error(e: ArgsError) -> io::Result<()> {
    match e {
        ArgsError::Parse(ref e) =>
            // In case of generic parse error,
            // message provided by the clap library will be the usage string.
            writeln!(&mut io::stderr(), "{}", e.message),
        e => {
            writeln!(&mut io::stderr(), "Failed to parse arguments: {}", e)
        }
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

/// Talk to crates.io to get the newest version of given crate
/// that matches specified version requirements.
fn get_newest_version(crate_: &Crate) -> Result<Version, Box<Error>> {
    let versions_url = format!("{}/{}/versions", CRATES_API_ROOT, crate_.name());
    debug!("Fetching latest matching version of crate `{}` from {}", crate_, versions_url);
    let response: Json = reqwest::get(&versions_url)?.json()?;

    // TODO: rather that silently skipping over incorrect versions,
    // report them as malformed response from crates.io
    let mut versions = response.pointer("/versions").and_then(|vs| vs.as_array()).map(|vs| {
        vs.iter().filter_map(|v| {
            v.as_object().and_then(|v| v.get("num")).and_then(|n| n.as_str())
        })
        .filter_map(|v| Version::parse(v).ok())
        .collect::<Vec<_>>()
    }).ok_or_else(|| format!("malformed response from {}", versions_url))?;

    if versions.is_empty() {
        return Err("no valid versions found".into());
    }

    let version_req = crate_.version_requirement();
    versions.sort_by(|a, b| b.cmp(a));
    versions.into_iter().find(|v| version_req.matches(v))
        .map(|v| { info!("Latest version of crate {} is {}", crate_, v); v.to_owned() })
        .ok_or_else(|| "no matching version found".into())
}

/// Download given crate and return it as a vector of gzipped bytes.
fn download_crate(name: &str, version: &Version) -> Result<Vec<u8>, Box<Error>> {
    let download_url = format!("{}/{}/{}/download", CRATES_API_ROOT, name, version);
    debug!("Downloading crate `{}=={}` from {}", name, version, download_url);
    let mut response = reqwest::get(&download_url)?;

    let content_length = response.headers().get::<ContentLength>().map(|&cl| *cl);
    trace!("Download size: {}",
        content_length.map(|cl| format!("{} bytes", cl)).unwrap_or_else(|| "<unknown>".into()));
    let mut bytes = match content_length {
        Some(cl) => Vec::with_capacity(cl as usize),
        None => Vec::new(),
    };
    response.read_to_end(&mut bytes)?;

    info!("Crate `{}=={}` downloaded successfully", name, version);
    Ok(bytes)
}
