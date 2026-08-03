//! Where occurrence hashes come from: splitting text into identifier parts.
//!
//! Ported from the prior implementation's `src/text_similarity/source.rs`
//! (`resolution.md` §5). `CodeParts` and the n-gram window came across with
//! it upstream and are dropped here: both serve body-text similarity, which
//! §5 excludes because it prefers the definition most resembling the call
//! site's surroundings — among several same-named candidates, a
//! plausible-wrong-answer generator.

use crate::occurrences::HashFrom;
use std::{iter::Peekable, path::Path};

pub trait OccurrenceSource {
    fn occurrences_in_utf8_bytes(
        str_bytes: impl IntoIterator<Item = u8>,
    ) -> impl Iterator<Item = HashFrom<Self>>;

    fn occurrences_in_str(str: &str) -> impl Iterator<Item = HashFrom<Self>> {
        Self::occurrences_in_utf8_bytes(str.bytes())
    }

    /// Occurrences from a path, the omitting file extension. Note that this does not split on
    /// components.
    fn occurrences_in_path(path: &Path) -> impl Iterator<Item = HashFrom<Self>> {
        let path_bytes = path.as_os_str().as_encoded_bytes();
        let bytes = if let Some(extension) = path.extension() {
            &path_bytes[0..path_bytes.len() - extension.as_encoded_bytes().len()]
        } else {
            path_bytes
        };
        Self::occurrences_in_utf8_bytes(bytes.iter().cloned())
    }
}

/// Occurrences source for finding relevant code by matching parts of identifiers.
///
/// * Splits the input into runs of ascii alphanumeric or unicode characters
/// * Splits these on ascii case transitions, handling camelCase and PascalCase
/// * Lowercases each part
#[derive(Debug)]
pub struct IdentifierParts;

impl OccurrenceSource for IdentifierParts {
    fn occurrences_in_utf8_bytes(
        str_bytes: impl IntoIterator<Item = u8>,
    ) -> impl Iterator<Item = HashFrom<Self>> {
        HashedIdentifierParts::new(str_bytes.into_iter())
    }
}

struct HashedIdentifierParts<I: Iterator<Item = u8>> {
    str_bytes: Peekable<I>,
    hasher: Option<FxHasher32>,
    prev_char_is_uppercase: bool,
}

impl<I: Iterator<Item = u8>> HashedIdentifierParts<I> {
    fn new(str_bytes: I) -> Self {
        Self {
            str_bytes: str_bytes.peekable(),
            hasher: None,
            prev_char_is_uppercase: false,
        }
    }
}

impl<I: Iterator<Item = u8>> Iterator for HashedIdentifierParts<I> {
    type Item = HashFrom<IdentifierParts>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(ch) = self.str_bytes.next() {
            let included = !ch.is_ascii() || ch.is_ascii_alphanumeric();
            if let Some(mut hasher) = self.hasher.take() {
                if !included {
                    return Some(hasher.finish().into());
                }

                // camelCase and PascalCase
                let is_uppercase = ch.is_ascii_uppercase();
                let should_split = is_uppercase
                    && (!self.prev_char_is_uppercase ||
                        // sequences like "XMLParser" -> ["XML", "Parser"]
                        self.str_bytes
                            .peek()
                            .is_some_and(|c| c.is_ascii_lowercase()));

                self.prev_char_is_uppercase = is_uppercase;

                if should_split {
                    let result = hasher.finish().into();
                    let mut hasher = FxHasher32::default();
                    hasher.write_u8(ch.to_ascii_lowercase());
                    self.hasher = Some(hasher);
                    return Some(result);
                } else {
                    hasher.write_u8(ch.to_ascii_lowercase());
                    self.hasher = Some(hasher);
                }
            } else if included {
                let mut hasher = FxHasher32::default();
                hasher.write_u8(ch.to_ascii_lowercase());
                self.hasher = Some(hasher);
                self.prev_char_is_uppercase = ch.is_ascii_uppercase();
            }
        }

        if let Some(hasher) = self.hasher.take() {
            return Some(hasher.finish().into());
        }

        None
    }
}

/// 32-bit variant of FXHasher
#[derive(Default)]
struct FxHasher32(u32);

impl FxHasher32 {
    #[inline]
    fn write_u8(&mut self, value: u8) {
        self.write_u32(u32::from(value));
    }

    #[inline]
    fn write_u32(&mut self, value: u32) {
        self.0 = self.0.wrapping_add(value).wrapping_mul(0x93d765dd);
    }

    fn finish(self) -> u32 {
        self.0
    }
}
