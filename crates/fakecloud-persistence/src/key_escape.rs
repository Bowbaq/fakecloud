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
    if encoded.is_empty() {
        // '@' is not in the percent-encoding output alphabet (A-Za-z0-9._-%),
        // so this sentinel cannot collide with any legitimately encoded segment.
        return "@empty".to_string();
    }

    if encoded.len() <= MAX_SEGMENT_BYTES {
        return encoded;
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

    // Sentinel prefix + hash + truncated head of the encoded segment.
    // The '@trunc-' prefix cannot appear in a normal percent-encoded segment.
    let prefix = format!("@trunc-{hex}-");
    let keep = MAX_SEGMENT_BYTES.saturating_sub(prefix.len());
    let mut end = keep.min(encoded.len());
    while end > 0 && !encoded.is_char_boundary(end) {
        end -= 1;
    }
    format!("{prefix}{}", &encoded[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string() {
        assert_eq!(escape_key_segment(""), "@empty");
    }

    #[test]
    fn literal_underscore_empty_does_not_collide_with_empty_sentinel() {
        // A user key that literally equals the old sentinel must not escape to
        // the new one. Percent-encoding leaves `_empty_` untouched (safe
        // chars), so it stays distinct from `@empty`.
        assert_eq!(escape_key_segment("_empty_"), "_empty_");
        assert_ne!(escape_key_segment("_empty_"), escape_key_segment(""));
    }

    #[test]
    fn long_keys_with_shared_prefix_get_distinct_hashes() {
        let a = format!("{}X", "a".repeat(500));
        let b = format!("{}Y", "a".repeat(500));
        let ea = escape_key_segment(&a);
        let eb = escape_key_segment(&b);
        assert_ne!(ea, eb);
        assert!(ea.starts_with("@trunc-"));
        assert!(eb.starts_with("@trunc-"));
        // A short literal that happens to equal the truncated head cannot
        // collide with either, because the overflow form is prefixed with `@trunc-`.
        let short = "a".repeat(100);
        let es = escape_key_segment(&short);
        assert!(!es.starts_with('@'));
        assert_ne!(es, ea);
        assert_ne!(es, eb);
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
