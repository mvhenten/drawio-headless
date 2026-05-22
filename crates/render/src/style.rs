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

/// Parse a drawio `points=[[x,y,perim],[x,y,perim],...]` value into a list
/// of normalised `(x, y)` constraint points. Tolerates whitespace and the
/// shorter `[x,y]` form (no `perim`). Returns an empty vec on malformed
/// input rather than erroring — connector attachment is a hint, not a
/// correctness invariant.
pub fn parse_points(value: &str) -> Vec<(f32, f32)> {
    let trimmed = value.trim();
    let inner = match trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        Some(s) => s.trim(),
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut start: Option<usize> = None;
    for (i, ch) in inner.char_indices() {
        match ch {
            '[' => {
                if depth == 0 {
                    start = Some(i + ch.len_utf8());
                }
                depth += 1;
            }
            ']' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start.take() {
                        let body = &inner[s..i];
                        if let Some(pt) = parse_one_point(body) {
                            out.push(pt);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn parse_one_point(body: &str) -> Option<(f32, f32)> {
    let parts: Vec<&str> = body.split(',').map(str::trim).collect();
    if parts.len() < 2 {
        return None;
    }
    let x: f32 = parts[0].parse().ok()?;
    let y: f32 = parts[1].parse().ok()?;
    Some((x, y))
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

    #[test]
    fn parses_aws_corner_points() {
        let pts = parse_points("[[0,0,0],[1,0,0],[0,1,0],[1,1,0]]");
        assert_eq!(pts, vec![(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)],);
    }

    #[test]
    fn parses_two_tuple_points_and_whitespace() {
        let pts = parse_points(" [ [0.5, 0] , [1, 0.5] , [0.5, 1] , [0, 0.5] ] ");
        assert_eq!(pts, vec![(0.5, 0.0), (1.0, 0.5), (0.5, 1.0), (0.0, 0.5)],);
    }

    #[test]
    fn malformed_points_yields_empty() {
        assert!(parse_points("garbage").is_empty());
        assert!(parse_points("[notanumber]").is_empty());
    }
}
