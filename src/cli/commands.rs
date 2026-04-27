use std::sync::atomic::AtomicBool;

mod entry;
mod hermes_manage;
mod hermes_validate;
mod mutate;
mod openclaw_manage;
mod openclaw_validate;
mod retrieval;
mod runtime;
mod types;

pub(super) use self::entry::run_command;
pub(super) use self::types::*;

static OCR_WORKER_RUNNING: AtomicBool = AtomicBool::new(false);
