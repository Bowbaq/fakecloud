/// Helper to wrap an XML body with the standard XML declaration.
pub fn wrap_xml(inner: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>{inner}")
}

/// Escape a string for safe embedding in XML content.
///
/// Handles the five standard XML entities plus control characters that are
/// invalid in XML 1.0 (everything below U+0020 except `\t`, `\n`, `\r`).
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // XML 1.0 allows \t, \n, \r as valid characters; all other control chars
            // need to be encoded as numeric character references.
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {
                out.push_str(&format!("&#x{:X};", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_xml_prepends_declaration() {
        let out = wrap_xml("<foo/>");
        assert!(out.starts_with("<?xml"));
        assert!(out.contains("<foo/>"));
    }

    #[test]
    fn xml_escape_standard_entities() {
        assert_eq!(
            xml_escape("a&b<c>d\"e'f"),
            "a&amp;b&lt;c&gt;d&quot;e&apos;f"
        );
    }

    #[test]
    fn xml_escape_preserves_whitespace() {
        assert_eq!(xml_escape("a\tb\nc\rd"), "a\tb\nc\rd");
    }

    #[test]
    fn xml_escape_control_chars() {
        let input = "a\x01b";
        let out = xml_escape(input);
        assert_eq!(out, "a&#x1;b");
    }

    #[test]
    fn xml_escape_plain_chars() {
        assert_eq!(xml_escape("hello"), "hello");
    }
}
