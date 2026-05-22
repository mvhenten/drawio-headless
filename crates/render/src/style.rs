//! drawio style strings are a semicolon-separated DSL like:
//! `sketch=0;shape=mxgraph.aws4.resourceIcon;fillColor=#ED7100;`.
//!
//! This module parses such a string into a [`StyleMap`] for cheap lookup.

use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct StyleMap(pub HashMap<String, String>);

impl StyleMap {
    pub fn parse(s: &str) -> Self {
        let mut map = HashMap::new();
        for part in s.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some((k, v)) = part.split_once('=') {
                map.insert(k.trim().to_string(), v.trim().to_string());
            } else {
                // shape-only entries like "ellipse" (no '=') — store as bool flag.
                map.insert(part.to_string(), String::new());
            }
        }
        Self(map)
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    pub fn get_or<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.get(key).unwrap_or(default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_style() {
        let s = StyleMap::parse(
            "sketch=0;shape=mxgraph.aws4.resourceIcon;fillColor=#ED7100;resIcon=mxgraph.aws4.lambda;",
        );
        assert_eq!(s.get("shape"), Some("mxgraph.aws4.resourceIcon"));
        assert_eq!(s.get("fillColor"), Some("#ED7100"));
        assert_eq!(s.get("resIcon"), Some("mxgraph.aws4.lambda"));
        assert_eq!(s.get("missing"), None);
    }
}
