mod capture;
mod config;
mod ocr;
mod optimize;
mod purge;
mod rebuild;
pub(in crate::db) mod revision;
pub(in crate::db) mod search_document;
mod settings;

#[cfg(test)]
mod tests;
