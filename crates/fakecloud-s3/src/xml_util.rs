/// Pull the text content of the first ``<tag>...</tag>`` element found in
/// ``body``, trimming surrounding whitespace. Returns ``None`` if the element
/// isn't present or isn't closed.
///
/// This is a deliberately tiny scanner used to read single-field values out of
/// the small S3 XML configuration documents (lifecycle rules, inventory
/// destinations, logging config, ...). It doesn't understand attributes,
/// namespaces, self-closing tags, or nested elements of the same name — any
/// caller that needs more than "fetch one leaf value" should reach for a real
/// XML parser instead.
pub(crate) fn extract_tag(body: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = body.find(&open)?;
    let content_start = start + open.len();
    let end = body[content_start..].find(&close)?;
    Some(body[content_start..content_start + end].trim().to_string())
}
