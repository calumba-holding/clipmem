pub(in crate::db) const LIST_VALUE_SEPARATOR: char = '\u{1f}';
pub(in crate::db) const MATCHED_FIELDS_SEPARATOR: char = '\u{1e}';

#[derive(Debug, Clone)]
pub(in crate::db) struct QueryAnalysis {
    pub(in crate::db) trimmed: String,
    pub(in crate::db) lower: String,
    pub(in crate::db) exact_phrase: Option<String>,
    pub(in crate::db) literal_preferred: bool,
    pub(in crate::db) path_fragment: Option<String>,
}
