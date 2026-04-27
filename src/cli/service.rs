use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::{env, fmt::Write as _, fs};

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;

use crate::db::{CaptureSettings, CaptureSkipReason};

use super::formats::format_duration_compact;

mod launchctl;
mod manage;
mod model;
mod render;
mod status;

#[cfg(test)]
pub(super) use self::launchctl::*;
pub(super) use self::manage::*;
pub(super) use self::model::*;
pub(super) use self::render::*;
pub(super) use self::status::*;

#[cfg(test)]
mod tests;
