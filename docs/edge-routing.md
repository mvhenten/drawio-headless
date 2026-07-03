# Edge routing

How orthogonal edges pick their departure point, arrival point, and route.
The goal is a diagram a reader can follow without guessing which arrow goes
where. This is not a full layout engine: the rules below are local
(per-node, per-edge), deterministic, and aim to handle the common 99% of
architecture diagrams. Rare pathological layouts may still render imperfect
routes, and that is acceptable.

The side-selection and distribution ideas follow what libavoid, draw.io's
own orthogonal connector, and ELK all converge on; see the FAQ for why we
don't adopt their full machinery.

## Invariants — never violated

1. **Head-on arrival.** The final segment of an edge is perpendicular to
   the side it enters, so the arrowhead points into the box. An edge
   travelling horizontally arrives at a left/right side; travelling
   vertically it arrives at a top/bottom side.
2. **No shared seams.** Two edge segments never lie on top of each other.
   Edges may only cross transversally — one horizontal, one vertical, a
   clean X. Edges that run together and then split are illegible.
3. **Edges never pass through nodes.** A segment never enters a box other
   than its own source or target.
4. **Determinism.** The same input always renders the same picture.

Explicit `exit`/`entry` overrides are honoured verbatim and may violate
any of these; the invariants govern defaults only.

## Rules

**R1 — Jetty.** Every default endpoint gets a straight stub of at least
10 px perpendicular to its side before the route may turn. This keeps
arrowheads and departures visually attached to their box and creates the
room that anchor distribution and lane separation need.

**R2 — Departure side.** The departure side faces the target, with a
horizontal bias: depart Left/Right whenever the target has meaningful
horizontal offset; depart Top/Bottom only when the target's x-extent
overlaps the source's (a straight-ish vertical run exists). A horizontal
departure yields a vertical-side arrival at a diagonal target, which is
what a reader expects (issue #52).

**R3 — Arrival side.** Determined by the incoming direction under
invariant 1: horizontal travel lands on the nearest left/right side,
vertical travel on the nearest top/bottom side. Colinear boxes connect
facing sides with a straight, bend-free line.

**R4 — Anchor distribution.** When k edges share one side of a node they
get k distinct anchors at fractions 1/(k+1) … k/(k+1) of the side.
Anchors are ordered by continuation: sort the edges by their far
endpoint's coordinate along the side's axis, so each edge leaves already
heading where it is going and edges never cross at the box edge. Ties
break by edge declaration order. (Issue #51.)

**R5 — Lane separation.** Distinct anchors give the L-bend case distinct
seams for free. Where two same-axis middle segments would still be
colinear, nudge them apart into parallel lanes with a fixed 8 px gutter,
ordered by continuation like R4.

**R6 — Bend budget.** Prefer the legal route with fewest bends: straight
when colinear, L otherwise, Z only when R1/R5 or node avoidance demands
it. Never more than three bends.

**R7 — Node avoidance.** An L-route has two candidate corners
(horizontal-first, vertical-first). If one candidate intersects a third
node, take the other; if both do, take a Z-route through the widest free
channel between the obstacles; if that also fails, accept the overlap.

Among routes the invariants and rules leave open, prefer fewest bends,
then shortest, then fewest crossings.

## FAQ

**Why not use a real routing engine (libavoid, ELK)?**
They solve global optimization with visibility graphs and A*. Our inputs
are small LLM-authored diagrams where local rules already produce legible
output; a dependency and a solver aren't worth it until the gallery proves
otherwise.

**Can two edges ever share an anchor on purpose?**
Not today. Bus-style edge grouping (many edges deliberately sharing a
path) is a known knob in yFiles; add it as an explicit opt-in if a diagram
needs it, never as default behaviour.

**What happens when a box has more edges than fit on one side?**
Anchors just get denser — k edges always fit at k/(k+1) fractions. Past
~5 edges per side the diagram is the problem, not the router.

**Does this change existing diagrams?**
Yes — any diagram relying on default endpoints may shift by design.
Explicit exit/entry overrides render exactly as before. Closed-loop
snapshots are regenerated in the same change.

**Why is a crossing acceptable but an overlap not?**
A perpendicular crossing is unambiguous — both paths remain traceable. An
overlap destroys the information of which arrow went where.

**What does "99%" mean concretely?**
The edge-case gallery in the closed-loop tests: fan-out, fan-in, diagonal
targets, forced crossings, seam-risk pairs, and a dense mixed layout must
all render with zero invariant violations. New failure shapes become new
gallery cases first, then rule tweaks.
