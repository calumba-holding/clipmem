use anyhow::Result;
use clap::{error::ErrorKind, Args, CommandFactory, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::db::{RetrievalFilters, RetrievalKind, SearchMode, TimelineSort};

mod commands;
mod db_path;
mod errors;
mod exit;
mod formats;
mod help;
mod human;
mod output;
mod parsing;
mod runtime;
mod schema;
mod service;
mod validate;

pub use self::exit::{CliError, CliExitCode};
pub use self::runtime::{run, run_cli, run_from};

use self::errors::*;
use self::formats::*;
use self::help::*;
use self::parsing::*;
use self::schema::*;
use self::validate::*;

#[cfg(test)]
mod tests;
