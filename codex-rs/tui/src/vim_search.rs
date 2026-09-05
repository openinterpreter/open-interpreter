//! Surface-independent literal Vim search queries. Matches span complete graphemes;
//! callers own navigation, operator ranges, and the lifetime of accepted versus pending input.

use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum SearchDirection {
    #[default]
    Forward,
    Backward,
}

impl SearchDirection {
    pub(crate) fn reversed(self) -> Self {
        match self {
            Self::Forward => Self::Backward,
            Self::Backward => Self::Forward,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SearchQuery {
    pub(crate) text: String,
    pub(crate) direction: SearchDirection,
}

pub(crate) fn matching_ranges<'a>(
    text: &'a str,
    query: &'a str,
) -> impl Iterator<Item = Range<usize>> + 'a {
    let text = if query.is_empty() { "" } else { text };
    text.grapheme_indices(/*is_extended*/ true)
        .filter(move |&(start, _)| text[start..].starts_with(query))
        .map(move |(start, _)| {
            let end = text[start..]
                .grapheme_indices(/*is_extended*/ true)
                .map(|(offset, grapheme)| start + offset + grapheme.len())
                .find(|&end| end >= start + query.len())
                .unwrap_or(text.len());
            start..end
        })
}
