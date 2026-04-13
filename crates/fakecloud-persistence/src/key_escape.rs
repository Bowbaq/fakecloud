use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use sha2::{Digest, Sha256};

const MAX_SEGMENT_BYTES: usize = 200;
const HASH_SUFFIX_HEX: usize = 12;

// Encode everything that isn't in A-Za-z0-9._-
const SAFE: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'!')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}')
    .add(b'~');

pub fn escape_key_segment(segment: &str) -> String {
    let encoded: String = utf8_percent_encode(segment, SAFE).collect();
    let placeholder = if encoded.is_empty() {
        "_empty_".to_string()
    } else {
        encoded
    };

    if placeholder.len() <= MAX_SEGMENT_BYTES {
        return placeholder;
    }

    let mut hasher = Sha256::new();
    hasher.update(segment.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest
        .iter()
        .take(HASH_SUFFIX_HEX.div_ceil(2))
        .map(|b| format!("{:02x}", b))
        .collect();
    let hex = &hex[..HASH_SUFFIX_HEX];

    let keep = MAX_SEGMENT_BYTES.saturating_sub(HASH_SUFFIX_HEX + 1);
    // Truncate at a char boundary within `keep` bytes.
    let mut end = keep.min(placeholder.len());
    while end > 0 && !placeholder.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}-{}", &placeholder[..end], hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string() {
        assert_eq!(escape_key_segment(""), "_empty_");
    }

    #[test]
    fn slash_is_encoded() {
        assert_eq!(escape_key_segment("a/b"), "a%2Fb");
    }

    #[test]
    fn unicode() {
        let out = escape_key_segment("日本語");
        assert!(out.is_ascii());
        assert!(out.contains('%'));
    }

    #[test]
    fn dotfile() {
        assert_eq!(escape_key_segment(".hidden"), ".hidden");
    }

    #[test]
    fn long_key_is_truncated_with_hash() {
        let raw = "a".repeat(500);
        let out = escape_key_segment(&raw);
        assert!(out.len() <= MAX_SEGMENT_BYTES);
        assert!(out.contains('-'));
    }

    #[test]
    fn differs_after_truncation_point_round_trip_unique() {
        let a = format!("{}{}", "a".repeat(500), "X");
        let b = format!("{}{}", "a".repeat(500), "Y");
        let ea = escape_key_segment(&a);
        let eb = escape_key_segment(&b);
        assert_ne!(ea, eb);
    }

    #[test]
    fn preserves_safe_chars() {
        assert_eq!(escape_key_segment("Foo.Bar-baz_1"), "Foo.Bar-baz_1");
    }
}
