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

/// Per-edge connection-point overrides.
///
/// drawio lets an edge fix its source/target attachment point on the cell's
/// bounding box via `exitX/exitY` (source side) and `entryX/entryY` (target
/// side). Each value is a normalised float in `[0.0, 1.0]`. When present,
/// these override any `points=` constraint declared on the cell itself.
///
/// A side is treated as "set" only when *both* coordinates of the pair are
/// present. This matches drawio's own behaviour: `exitX` alone has no effect
/// because the renderer has no y-coordinate to combine it with.
///
/// Values outside `[0.0, 1.0]` are clamped on read — drawio clamps too, and
/// the result is well-defined (the attachment lands on the cell's perimeter
/// at most, not floating in the void).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct EdgeEndpoints {
    pub exit: Option<(f32, f32)>,
    pub entry: Option<(f32, f32)>,
}

impl EdgeEndpoints {
    /// Read the four optional style keys from `style` and pair them up.
    pub fn from_style(style: &StyleMap) -> Self {
        let exit_x = parse_unit(style.get("exitX"));
        let exit_y = parse_unit(style.get("exitY"));
        let entry_x = parse_unit(style.get("entryX"));
        let entry_y = parse_unit(style.get("entryY"));
        Self {
            exit: pair(exit_x, exit_y),
            entry: pair(entry_x, entry_y),
        }
    }
}

/// Parse a `[0.0, 1.0]`-normalised attachment coordinate. Values outside the
/// range are clamped (drawio does the same). Returns `None` when the key is
/// missing or unparseable so the caller can fall back to the cell's picker.
fn parse_unit(s: Option<&str>) -> Option<f32> {
    let v: f32 = s?.parse().ok()?;
    if !v.is_finite() {
        return None;
    }
    Some(v.clamp(0.0, 1.0))
}

fn pair(x: Option<f32>, y: Option<f32>) -> Option<(f32, f32)> {
    match (x, y) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => None,
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

    #[test]
    fn parses_edge_endpoints_from_style() {
        let s =
            StyleMap::parse("edgeStyle=orthogonalEdgeStyle;exitX=1;exitY=0.5;entryX=0;entryY=0.5;");
        let ep = EdgeEndpoints::from_style(&s);
        assert_eq!(ep.exit, Some((1.0, 0.5)));
        assert_eq!(ep.entry, Some((0.0, 0.5)));
    }

    #[test]
    fn edge_endpoints_missing_pair_member_treated_as_unset() {
        // exitX without exitY: drawio ignores the half-spec; we do too.
        let s = StyleMap::parse("exitX=1;entryY=0.5;");
        let ep = EdgeEndpoints::from_style(&s);
        assert_eq!(ep.exit, None);
        assert_eq!(ep.entry, None);
    }

    #[test]
    fn edge_endpoints_default_when_absent() {
        let s = StyleMap::parse("edgeStyle=orthogonalEdgeStyle;endArrow=open;");
        let ep = EdgeEndpoints::from_style(&s);
        assert_eq!(ep.exit, None);
        assert_eq!(ep.entry, None);
    }

    #[test]
    fn edge_endpoints_clamp_out_of_range() {
        // drawio clamps values outside [0, 1] to the perimeter. Match that.
        let s = StyleMap::parse("exitX=1.5;exitY=-0.2;entryX=2;entryY=0;");
        let ep = EdgeEndpoints::from_style(&s);
        assert_eq!(ep.exit, Some((1.0, 0.0)));
        assert_eq!(ep.entry, Some((1.0, 0.0)));
    }

    #[test]
    fn edge_endpoints_ignore_unparseable_values() {
        let s = StyleMap::parse("exitX=nope;exitY=0.5;entryX=0;entryY=junk;");
        let ep = EdgeEndpoints::from_style(&s);
        assert_eq!(ep.exit, None);
        assert_eq!(ep.entry, None);
    }
}
