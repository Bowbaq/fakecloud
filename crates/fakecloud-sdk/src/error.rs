use std::fmt;

/// Errors returned by the fakecloud SDK client.
#[derive(Debug)]
pub enum Error {
    /// HTTP transport error from reqwest.
    Http(reqwest::Error),
    /// The server returned a non-success HTTP status.
    Api { status: u16, body: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Http(e) => write!(f, "HTTP error: {e}"),
            Error::Api { status, body } => write!(f, "API error (HTTP {status}): {body}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Http(e) => Some(e),
            Error::Api { .. } => None,
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Http(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    #[test]
    fn api_error_display_contains_status_and_body() {
        let err = Error::Api {
            status: 404,
            body: "not found".to_string(),
        };
        let text = format!("{err}");
        assert!(text.contains("404"));
        assert!(text.contains("not found"));
    }

    #[test]
    fn api_error_has_no_source() {
        let err = Error::Api {
            status: 500,
            body: "oops".to_string(),
        };
        assert!(err.source().is_none());
    }

    #[test]
    fn debug_impl_works() {
        let err = Error::Api {
            status: 400,
            body: "bad".to_string(),
        };
        let d = format!("{err:?}");
        assert!(d.contains("Api"));
    }

    #[tokio::test]
    async fn http_error_display_includes_prefix() {
        let resp = reqwest::get("http://127.0.0.1:1/nope").await;
        let reqwest_err = resp.err().unwrap();
        let err: Error = reqwest_err.into();
        let text = format!("{err}");
        assert!(text.starts_with("HTTP error:"));
        assert!(err.source().is_some());
    }
}
