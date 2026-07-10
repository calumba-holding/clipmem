pub(in crate::db) const WEBP_UTI: &str = "org.webmproject.webp";
pub(in crate::db) const IMAGE_OPTIMIZATION_FORMAT: &str = "webp_lossless";
pub(in crate::db) const IMAGE_OPTIMIZATION_MIN_ABSOLUTE_SAVINGS: usize = 64 * 1024;
pub(in crate::db) const IMAGE_OPTIMIZATION_MIN_RELATIVE_SAVINGS_NUMERATOR: usize = 1;
pub(in crate::db) const IMAGE_OPTIMIZATION_MIN_RELATIVE_SAVINGS_DENOMINATOR: usize = 10;
pub(in crate::db) const IMAGE_OPTIMIZATION_MAX_DIMENSION: u32 = 16_384;

#[derive(Debug, Clone)]
pub(in crate::db) struct ImageOptimizationCandidate {
    pub(in crate::db) snapshot_id: i64,
    pub(in crate::db) item_index: i64,
    pub(in crate::db) uti: String,
    pub(in crate::db) byte_len: usize,
    pub(in crate::db) raw_sha256: String,
    pub(in crate::db) blob_value: Vec<u8>,
}
