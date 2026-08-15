mod endpoints;
mod galleryinfo;
mod model;
mod nozomi;
mod routing;

/// Increment when a parser change alters the typed meaning of saved source fixtures.
pub const HITOMI_PARSER_VERSION: u32 = 1;
/// Increment when routing or candidate ordering changes for the same source metadata.
pub const HITOMI_RESOLVER_VERSION: u32 = 1;

pub use endpoints::{
    galleryinfo_script_url, gg_script_url, index_all_nozomi_url, HITOMI_METADATA_ORIGIN,
    NOZOMI_CONTENT_TYPE,
};
pub use galleryinfo::parse_galleryinfo_script;
pub use model::{
    HitomiGalleryDetail, HitomiGalleryMetadata, HitomiGallerySummary, HitomiPageFile, HitomiTag,
    HitomiTagKind, SourceRevision, HITOMI_CONTENT_DOMAIN, HITOMI_ORIGIN,
};
pub use nozomi::{parse_nozomi_ids, parse_nozomi_range, NozomiByteRange, MAX_NOZOMI_RANGE_ITEMS};
pub use routing::{
    parse_gg_routing, webp_full_candidates, webp_thumbnail_candidates, GgRoutingTable,
    HitomiImageCandidate, HitomiImageKind, ThumbnailSize,
};

#[cfg(test)]
mod tests;
