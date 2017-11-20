//! Module for handling command line arguments.

use std::borrow::Cow;
use std::env;
use std::error::Error;
use std::fmt;
use std::ffi::OsString;
use std::iter::IntoIterator;
use std::str::FromStr;

use clap::{self, AppSettings, Arg, ArgMatches};
use conv::TryFrom;
use semver::{Version, VersionReq, ReqParseError, SemVerError};

use super::{NAME, VERSION};


// Parse command line arguments and return `Options` object.
#[inline]
pub fn parse() -> Result<Options, ArgsError> {
    parse_from_argv(env::args_os())
}

/// Parse application options from given array of arguments
/// (*all* arguments, including binary name).
#[inline]
pub fn parse_from_argv<I, T>(argv: I) -> Result<Options, ArgsError>
    where I: IntoIterator<Item=T>, T: Clone + Into<OsString>
{
    let parser = create_parser();
    let matches = try!(parser.get_matches_from_safe(argv));
    Options::try_from(matches)
}


/// Structure to hold options received from the command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Verbosity of the logging output.
    ///
    /// Corresponds to the number of times the -v flag has been passed.
    /// If -q has been used instead, this will be negative.
    pub verbosity: isize,
    /// Crate to download.
    pub crate_: Crate,
    /// Whether to extract the crate's archive.
    pub extract: bool,
}

#[allow(dead_code)]
impl Options {
    #[inline]
    pub fn verbose(&self) -> bool { self.verbosity > 0 }
    #[inline]
    pub fn quiet(&self) -> bool { self.verbosity < 0 }
}

impl<'a> TryFrom<ArgMatches<'a>> for Options {
    type Err = ArgsError;

    fn try_from(matches: ArgMatches<'a>) -> Result<Self, Self::Err> {
        let verbose_count = matches.occurrences_of(OPT_VERBOSE) as isize;
        let quiet_count = matches.occurrences_of(OPT_QUIET) as isize;
        let verbosity = verbose_count - quiet_count;

        let crate_ = Crate::from_str(matches.value_of(ARG_CRATE).unwrap())?;
        let extract = matches.is_present(OPT_EXTRACT);

        Ok(Options{verbosity, crate_, extract})
    }
}


/// Specification of a crate to download.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Crate {
    name: String,
    version: CrateVersion,
}
impl FromStr for Crate {
    type Err = CrateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<_> = s.splitn(2, "=").map(|p| p.trim()).collect();
        let name = parts[0].to_owned();
        if parts.len() < 2 {
            let valid_name =
                name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_');
            if valid_name {
                Ok(Crate{
                    name,
                    version: CrateVersion::Other(VersionReq::any()),
                })
            } else {
                Err(CrateError::Name(name))
            }
        } else {
            let version = CrateVersion::from_str(parts[1])?;
            Ok(Crate{name, version})
        }
    }
}
impl Crate {
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn exact_version(&self) -> Option<&Version> {
        match self.version {
            CrateVersion::Exact(ref v) => Some(v),
            _ => None,
        }
    }

    pub fn version_requirement(&self) -> Cow<VersionReq> {
        match self.version {
            CrateVersion::Exact(ref v) => Cow::Owned(VersionReq::exact(v)),
            CrateVersion::Other(ref r) => Cow::Borrowed(r),
        }
    }
}
impl fmt::Display for Crate {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}={}", self.name, self.version)
    }
}

/// Crate version.
#[derive(Debug, Clone, Eq, PartialEq)]
enum CrateVersion {
    /// Exact version, like =1.0.0.
    Exact(Version),
    /// Non-exact version, like ^1.0.0.
    Other(VersionReq)
}
impl FromStr for CrateVersion {
    type Err = CrateVersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("=") {
            let version = Version::from_str(&s[1..])?;
            Ok(CrateVersion::Exact(version))
        } else {
            let version_req = VersionReq::from_str(s)?;
            Ok(CrateVersion::Other(version_req))
        }
    }
}
impl fmt::Display for CrateVersion {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &CrateVersion::Exact(ref v) => write!(fmt, "={}", v),
            &CrateVersion::Other(ref r) => write!(fmt, "{}", r),
        }
    }
}


/// Error that can occur while parsing of command line arguments.
#[derive(Debug, Error)]
pub enum ArgsError {
    /// General when parsing the arguments.
    Parse(clap::Error),
    /// Error when parsing crate version.
    Crate(CrateError),
}

/// Error that can occur while parsing CRATE argument.
#[derive(Debug)]
pub enum CrateError {
    /// General syntax error of the crate specification.
    Name(String),
    /// Error parsing the semver spec of the crate.
    Version(CrateVersionError),
}
impl From<CrateVersionError> for CrateError {
    fn from(input: CrateVersionError) -> Self {
        CrateError::Version(input)
    }
}
impl Error for CrateError {
    fn description(&self) -> &str { "invalid crate specification" }
    fn cause(&self) -> Option<&Error> {
        match self {
            &CrateError::Version(ref e) => Some(e),
            _ => None,
        }
    }
}
impl fmt::Display for CrateError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &CrateError::Name(ref n) => write!(fmt, "invalid crate name `{}`", n),
            &CrateError::Version(ref e) => write!(fmt, "invalid crate version: {}", e),
        }
    }
}

/// Error that can occur while parsing crate version.
#[derive(Debug, Error)]
pub enum CrateVersionError {
    Syntax(SemVerError),
    Semantics(ReqParseError),
}
// impl fmt::Display for CrateVersionError {
//     fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
//         match self {
//             &CrateVersionError::Syntax(ref e) => write!(fmt, "invalid crate version: {}", e),
//             $CrateVersionError::Semantics(ref e) =>
//                 write!(fmt, "incorrect crate version requirement: {}", e),
//         }
//     }
// }


// Parser configuration

/// Type of the argument parser object
/// (which is called an "App" in clap's silly nomenclature).
type Parser<'p> = clap::App<'p, 'p>;


lazy_static! {
    static ref ABOUT: &'static str = option_env!("CARGO_PKG_DESCRIPTION").unwrap_or("");
}

const ARG_CRATE: &'static str = "crate";
const OPT_EXTRACT: &'static str = "extract";
const OPT_VERBOSE: &'static str = "verbose";
const OPT_QUIET: &'static str = "quiet";

/// Create the parser for application's command line.
fn create_parser<'p>() -> Parser<'p> {
    let mut parser = Parser::new(*NAME);
    if let Some(version) = *VERSION {
        parser = parser.version(version);
    }
    parser
        .bin_name("cargo download")
        .about(*ABOUT)
        .author(crate_authors!(", "))

        .setting(AppSettings::StrictUtf8)

        .setting(AppSettings::UnifiedHelpMessage)
        .setting(AppSettings::DontCollapseArgsInUsage)
        .setting(AppSettings::DeriveDisplayOrder)
        .setting(AppSettings::ColorNever)

        .arg(Arg::with_name(ARG_CRATE)
            .value_name("CRATE[=VERSION]")
            .required(true)
            .help("Crate to download")
            .long_help(concat!(
                "The crate to download.\n\n",
                "This can be just a crate name (like \"foo\"), in which case ",
                "the newest version of the crate is fetched. ",
                "Alternatively, the VERSION requirement can be given after ",
                "the equal sign (=) in the usual Cargo.toml format ",
                "(e.g. \"foo==0.9\" for the exact version).")))

        .arg(Arg::with_name(OPT_EXTRACT)
            .long("extract").short("x")
            .required(false)
            .multiple(false)
            .takes_value(false)
            .help("Whether to automatically extract the crate's archive")
            .long_help(concat!(
                "Specify this flag to have the crate extracted automatically.",
                "\n\nNote that unless changed via the --output flag, ",
                "this will extract the files to a new subdirectory ",
                "bearing the name of the downloaded crate archive.")))

        // Verbosity flags.
        .arg(Arg::with_name(OPT_VERBOSE)
            .long("verbose").short("v")
            .multiple(true)
            .conflicts_with(OPT_QUIET)
            .help("Increase logging verbosity"))
        .arg(Arg::with_name(OPT_QUIET)
            .long("quiet").short("q")
            .multiple(true)
            .conflicts_with(OPT_VERBOSE)
            .help("Decrease logging verbosity"))

        .help_short("H")
        .version_short("V")
}
