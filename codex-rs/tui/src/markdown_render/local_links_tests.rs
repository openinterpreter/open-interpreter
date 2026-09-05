//! Tests for local-link parsing and root-preserving separator normalization.

use super::COLON_LOCATION_SUFFIX_RE;
use super::HASH_LOCATION_SUFFIX_RE;
use super::trim_trailing_local_path_separator;
use pretty_assertions::assert_eq;

#[test]
fn load_location_suffix_regexes() {
    let _colon = &*COLON_LOCATION_SUFFIX_RE;
    let _hash = &*HASH_LOCATION_SUFFIX_RE;
}

#[test]
fn trailing_separator_trimming_preserves_local_roots() {
    let paths = ["/", "//", "C:/", "dir/", "//server/share/", "C:/dir/"];
    assert_eq!(
        paths.map(trim_trailing_local_path_separator),
        ["/", "//", "C:/", "dir", "//server/share", "C:/dir"]
    );
}
