//! Inflate a compressed `<diagram>` payload back to `<mxGraphModel>` XML.
//!
//! drawio interactively-saved files compress the `<diagram>` body via
//! `Graph.compress` ([`Graph.js:2192`] in `jgraph/drawio`):
//!
//! 1. URL-encode the `<mxGraphModel>...</mxGraphModel>` UTF-8 string.
//! 2. Raw DEFLATE compress (no zlib header).
//! 3. Base64-encode the deflate output.
//!
//! Inflation reverses this: base64 decode -> raw inflate -> URL decode.
//!
//! [`Graph.js:2192`]: https://github.com/jgraph/drawio/blob/master/src/main/webapp/js/Graph.js

use std::io::Read as _;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use flate2::read::DeflateDecoder;
use percent_encoding::percent_decode_str;

use crate::RenderError;

/// Decide whether a `<diagram>` body is compressed.
///
/// Uncompressed bodies start with `<` (the opening of `<mxGraphModel>`).
/// Anything else is treated as compressed — this is more robust than
/// looking at `compressed="..."` on `<mxfile>`, which some files omit.
pub fn is_compressed_body(body: &str) -> bool {
    !body.trim_start().starts_with('<')
}

/// Inflate a compressed `<diagram>` body into its original `<mxGraphModel>`
/// XML string.
///
/// The body is expected to be a base64 string of raw-deflated, URL-encoded
/// UTF-8. Leading/trailing whitespace is tolerated.
pub fn inflate_diagram_body(body: &str) -> Result<String, RenderError> {
    let trimmed = body.trim();
    let compressed = STANDARD.decode(trimmed)?;
    let mut inflater = DeflateDecoder::new(&compressed[..]);
    let mut url_encoded = String::new();
    inflater
        .read_to_string(&mut url_encoded)
        .map_err(RenderError::Inflate)?;
    let xml = percent_decode_str(&url_encoded)
        .decode_utf8()
        .map_err(|err| RenderError::UrlDecode(err.to_string()))?;
    Ok(xml.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verified payload from the issue brief: a 2-node API Gateway -> Lambda
    /// diagram. Stored as the literal base64-and-urlencoded string a
    /// drawio file would contain between `<diagram>...</diagram>`.
    const COMPRESSED_FIXTURE: &str = include_str!("../tests/fixtures/compressed-payload.txt");

    #[test]
    fn inflates_verified_payload() {
        // The fixture body is itself URL-encoded (it lives inside the
        // mxfile envelope). Decode the outer URL encoding first, then
        // hand the resulting base64 to the inflate pipeline.
        let outer = percent_decode_str(COMPRESSED_FIXTURE.trim())
            .decode_utf8()
            .unwrap();
        let xml = inflate_diagram_body(&outer).expect("inflate");
        assert!(
            xml.contains("<mxGraphModel"),
            "expected mxGraphModel root in inflated payload"
        );
        assert!(
            xml.contains("mxgraph.aws4.api_gateway"),
            "expected api_gateway shape in payload",
        );
        assert!(
            xml.contains("aws3.lambda_function") || xml.contains("aws4.lambda"),
            "expected a lambda shape in payload: {xml}"
        );
    }

    #[test]
    fn detects_uncompressed_body() {
        assert!(!is_compressed_body("<mxGraphModel><root/></mxGraphModel>"));
        assert!(!is_compressed_body("  \n  <mxGraphModel/>"));
    }

    #[test]
    fn detects_compressed_body() {
        assert!(is_compressed_body("tZZfj5swDMA="));
        assert!(is_compressed_body("  AAAA  "));
    }
}
