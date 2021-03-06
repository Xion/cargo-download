# cargo-download

[![crates.io](https://img.shields.io/crates/v/cargo-download.svg)](https://crates.io/crates/cargo-download)
[![Build Status](https://travis-ci.org/Xion/cargo-download.svg?branch=master)](https://travis-ci.org/Xion/cargo-download)

A cargo subcommand for downloading crates from _crates.io_

## About

`cargo-download` can be used to download a gzipped archive of given crate,
in the exact form that it was uploaded to _crates.io_.

This can be useful for a variety of things, such as:

* checking in your dependencies in source control (if your team/organization follows this practice)
* mirroring _crates.io_ for reproducible CI/CD pipelines
* security auditing of crates (esp. when a crate repository is missing)
* reproducing a bug that only occurs in uploaded versions of your crate

## Installation

`cargo-download` can be installed with `cargo install`:

    $ cargo install cargo-download

This shall put the `cargo-download` executable in your Cargo binary directory
(e.g. `~/.cargo/bin`), which hopefully is in your `$PATH`.

## Usage

To download the newest version of `foo` crate, do this:

    $ cargo download foo >foo.gz

You can also use the standard _Cargo.toml_ notation to specify a particular version:

    $ cargo download foo==0.9 >foo-0.9.gz

For more detailed usage instructions, run `cargo download --help`.

## License

`cargo-download` is licensed under the terms of the MIT license.
