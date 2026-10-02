//! Sketch lowering: pen paths and circles become the kernel's exact rings — a vertex list and
//! the edge leaving each vertex, a line or an arc about a stated centre — with corner treatments
//! resolved against the **sharp** ring, and the kernel's ring court (`Ring2d::new` per path,
//! `from_paths` over all of them) judging the result in one sitting.
//!
//! The kit's share is spelling. A chamfer's two
//! retreat points and a right-angle fillet's tangent points and centre are rational arithmetic on
//! the author's decimals (axis-aligned edges — the exact family), an
//! `arc({ center, sweep })` is a quarter-turn count, and every *computed* value reaches the kernel
//! through its `Rat` doors, never through f64 (`0.1 − 0.2` in f64 is not a decimal any
//! more). The pen is the kit's and the kernel does not know it: it takes rings, however they
//! were drawn. Simplicity, nesting — the judgments — are the kernel's,
//! and its rejections come back with coordinates.

use crate::error::KitError;
use crate::step::{CircleSize, Corner, Path, PenPath, SketchSeg};
use nacre::exact::{Rat, orient2d_rat};
use nacre::ops::{Edge2d, Profile2d, Ring2d, SketchError, arc_to_rat, arc_turns_rat, from_paths};

type P = [Rat; 2];

fn program(step: usize, what: String) -> KitError {
    KitError::Program { step, what }
}

fn lift(step: usize, p: [f64; 2]) -> Result<P, KitError> {
    match (Rat::from_decimal(p[0]), Rat::from_decimal(p[1])) {
        (Some(x), Some(y)) => Ok([x, y]),
        _ => Err(program(
            step,
            format!("({}, {}) is outside the exact decimal window", p[0], p[1]),
        )),
    }
}

fn f2(p: P) -> [f64; 2] {
    [p[0].to_f64(), p[1].to_f64()]
}

/// The quarter-turn count an `arc({ sweep })` states — a nonzero multiple of 90 short of a whole
/// turn, today. Any other angle ends at a point that is not rational; that end will be a *named*
/// point one day, not a rounded one.
fn quarter_turns(step: usize, sweep: f64) -> Result<i32, KitError> {
    let ok = sweep.is_finite() && sweep != 0.0 && sweep % 90.0 == 0.0 && sweep.abs() < 360.0;
    if !ok {
        return Err(program(
            step,
            format!(
                "an arc's sweep is a multiple of 90° between -270 and 270 today, got {sweep} — \
                 any other angle ends at a point that is not rational (a whole circle is \
                 `circle`)"
            ),
        ));
    }
    Ok((sweep / 90.0) as i32)
}

/// Which axis a straight step runs along, if either.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis2 {
    Horizontal,
    Vertical,
}

fn axis_of(a: P, b: P) -> Option<Axis2> {
    if a[1] == b[1] && a[0] != b[0] {
        Some(Axis2::Horizontal)
    } else if a[0] == b[0] && a[1] != b[1] {
        Some(Axis2::Vertical)
    } else {
        None
    }
}

/// The point `d` along the straight step `p → q` from `p`. Axis-aligned steps move exactly in
/// `Rat`; a slanted step is measured in f64 and lifted, which is exact only while the result is a
/// short decimal — the chamfer's standing behaviour.
fn retreat(step: usize, p: P, q: P, d: Rat) -> Result<P, KitError> {
    let toward = |from: Rat, to: Rat| -> Option<Rat> {
        if to > from {
            from.checked_add(d)
        } else {
            from.checked_sub(d)
        }
    };
    match axis_of(p, q) {
        Some(Axis2::Horizontal) => Ok([
            toward(p[0], q[0])
                .ok_or_else(|| program(step, "a corner's retreat overflows".into()))?,
            p[1],
        ]),
        Some(Axis2::Vertical) => Ok([
            p[0],
            toward(p[1], q[1])
                .ok_or_else(|| program(step, "a corner's retreat overflows".into()))?,
        ]),
        None => {
            let (pf, qf, df) = (f2(p), f2(q), d.to_f64());
            let (dx, dy) = (qf[0] - pf[0], qf[1] - pf[1]);
            let len = (dx * dx + dy * dy).sqrt();
            lift(step, [pf[0] + df * dx / len, pf[1] + df * dy / len])
        }
    }
}

fn dist2(a: P, b: P) -> Option<Rat> {
    let dx = a[0].checked_sub(b[0])?;
    let dy = a[1].checked_sub(b[1])?;
    dx.checked_mul(dx)?.checked_add(dy.checked_mul(dy)?)
}

fn kernel<T>(step: usize, r: Result<T, SketchError>) -> Result<T, KitError> {
    r.map_err(|e| translate(step, e))
}

/// One stop of the pen: a sharp vertex between two straight steps (with the corner treatment
/// declared there), or an arc already stated — leaving `from`; the pen's next vertex is where
/// it lands.
enum Item {
    Vertex { at: P, corner: Option<Corner> },
    Arc { from: P, edge: Edge2d },
}

/// Lower a sketch to the kernel's rings, one per path, in the order written.
pub(crate) fn lower(step: usize, paths: &[Path]) -> Result<Vec<Ring2d>, KitError> {
    if paths.is_empty() {
        return Err(program(
            step,
            "an empty sketch — draw at least one closed path".into(),
        ));
    }
    paths
        .iter()
        .map(|path| match path {
            Path::Pen(pen) => lower_path(step, pen),
            Path::Circle { center, size } => {
                let c = lift(step, *center)?;
                let dec = |x: f64| {
                    Rat::from_decimal(x).ok_or_else(|| {
                        program(step, format!("{x} is outside the exact decimal window"))
                    })
                };
                // A diameter is halved here, in rationals — `d / 2` in f64 is not the decimal
                // the author wrote once the digits run long.
                let radius =
                    match size {
                        CircleSize::Radius(r) => dec(*r)?,
                        CircleSize::Diameter(d) => dec(*d)?
                            .checked_mul(Rat::new(1, 2).expect("1/2"))
                            .ok_or_else(|| program(step, "a diameter overflows".into()))?,
                    };
                kernel(step, Ring2d::circle_rat(c, radius))
            }
        })
        .collect()
}

fn lower_path(step: usize, path: &PenPath) -> Result<Ring2d, KitError> {
    // 1. Walk the pen. Straight steps become sharp vertices; arcs are stated at once (the kernel
    //    computes their ends in `Rat`, and that end is where the pen stands next).
    let start = lift(step, path.start)?;
    let mut items: Vec<Item> = vec![Item::Vertex {
        at: start,
        corner: path.close_corner,
    }];
    let mut pos = start;
    for seg in &path.segs {
        match seg {
            SketchSeg::LineTo { to, corner } => {
                let to = lift(step, *to)?;
                if to == pos {
                    return Err(program(
                        step,
                        format!(
                            "a lineTo repeats the point ({}, {})",
                            f2(pos)[0],
                            f2(pos)[1]
                        ),
                    ));
                }
                items.push(Item::Vertex {
                    at: to,
                    corner: *corner,
                });
                pos = to;
            }
            SketchSeg::Arc { center, sweep } => {
                let c = lift(step, *center)?;
                let (edge, end) =
                    kernel(step, arc_turns_rat(c, pos, quarter_turns(step, *sweep)?))?;
                items.push(Item::Arc { from: pos, edge });
                pos = end;
                items.push(Item::Vertex {
                    at: pos,
                    corner: None,
                });
            }
        }
    }
    // 2. `close()`: the pen already home draws nothing; otherwise the closing line is implied by
    //    the ring wrapping round to `start`. A path that closed on an arc has its last vertex
    //    equal to the first — drop the duplicate so the wrap is the identity.
    if items.len() > 1 && matches!(items.last(), Some(Item::Vertex { at, .. }) if *at == start) {
        let dropped = items.pop();
        if let Some(Item::Vertex {
            corner: Some(_), ..
        }) = dropped
        {
            return Err(program(
                step,
                "a corner treatment on the point the path closes on belongs to close({ ... })"
                    .into(),
            ));
        }
    }
    let n = items.len();
    let vertex_count = items
        .iter()
        .filter(|it| matches!(it, Item::Vertex { .. }))
        .count();
    if vertex_count < 2
        || (vertex_count < 3 && !items.iter().any(|it| matches!(it, Item::Arc { .. })))
    {
        return Err(program(
            step,
            "a closed path needs at least three points".into(),
        ));
    }

    // 3. Corner treatments, resolved against the sharp ring, all at once.
    struct Treat {
        before: P,
        after: P,
        emit: Option<Edge2d>,
    }
    let mut treats: Vec<Option<Treat>> = Vec::with_capacity(n);
    for i in 0..n {
        let Item::Vertex { at: p, corner } = &items[i] else {
            treats.push(None);
            continue;
        };
        let Some(corner) = corner else {
            treats.push(None);
            continue;
        };
        let (prev, next) = (&items[(i + n - 1) % n], &items[(i + 1) % n]);
        let (Item::Vertex { at: a, .. }, Item::Vertex { at: b, .. }) = (prev, next) else {
            return Err(program(
                step,
                format!(
                    "the corner at ({}, {}) sits beside an arc — a chamfer or fillet rounds the \
                     corner between two straight lines",
                    f2(*p)[0],
                    f2(*p)[1]
                ),
            ));
        };
        let (a, b, p) = (*a, *b, *p);
        let d = match corner {
            Corner::Chamfer(x) | Corner::Fillet(x) => {
                if x.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
                    return Err(program(
                        step,
                        format!("a corner treatment must be positive, got {x}"),
                    ));
                }
                Rat::from_decimal(*x).ok_or_else(|| {
                    program(step, format!("{x} is outside the exact decimal window"))
                })?
            }
        };
        if orient2d_rat(a, p, b) == 0 {
            return Err(program(
                step,
                format!(
                    "the corner at ({}, {}) is straight — nothing to round or cut",
                    f2(p)[0],
                    f2(p)[1]
                ),
            ));
        }
        for q in [a, b] {
            let len2 =
                dist2(p, q).ok_or_else(|| program(step, "a corner's edge overflows".into()))?;
            let d2 = d
                .checked_mul(d)
                .ok_or_else(|| program(step, "a corner's edge overflows".into()))?;
            if d2 > len2 || (d2 == len2 && matches!(corner, Corner::Chamfer(_))) {
                return Err(program(
                    step,
                    format!(
                        "the {} {} at ({}, {}) does not fit — its edge is only {} long",
                        match corner {
                            Corner::Chamfer(_) => "chamfer",
                            Corner::Fillet(_) => "fillet",
                        },
                        d.to_f64(),
                        f2(p)[0],
                        f2(p)[1],
                        len2.to_f64().sqrt()
                    ),
                ));
            }
        }
        let before = retreat(step, p, a, d)?;
        let after = retreat(step, p, b, d)?;
        let emit = match corner {
            Corner::Chamfer(_) => Some(Edge2d::Line),
            Corner::Fillet(_) => {
                // Two straight edges at a right angle, both axis-aligned: the tangent points are
                // exactly `r` back along each edge and the centre is `P + r·(û + v̂)`. Any other
                // corner's tangent points are irrational (`r / tan(θ/2)`) — the family that opens
                // with named points, refused by name until then.
                let (ax_in, ax_out) = (axis_of(p, a), axis_of(p, b));
                let right_angle = matches!(
                    (ax_in, ax_out),
                    (Some(Axis2::Horizontal), Some(Axis2::Vertical))
                        | (Some(Axis2::Vertical), Some(Axis2::Horizontal))
                );
                if !right_angle {
                    return Err(program(
                        step,
                        format!(
                            "the fillet at ({}, {}) is not on a right angle between axis-aligned \
                             lines — that is the exact family today; other angles wait for named \
                             points (chamfer works on any corner)",
                            f2(p)[0],
                            f2(p)[1]
                        ),
                    ));
                }
                // The centre shares `before`'s coordinate along the incoming edge's normal and
                // `after`'s along the outgoing edge's: with one edge horizontal and one vertical
                // that is `(after.x, before.y)` or `(before.x, after.y)`.
                let center = match ax_in {
                    Some(Axis2::Horizontal) => [before[0], after[1]],
                    _ => [after[0], before[1]],
                };
                let ccw = orient2d_rat(a, p, b) > 0;
                Some(kernel(step, arc_to_rat(center, before, after, ccw))?)
            }
        };
        treats.push(Some(Treat {
            before,
            after,
            emit,
        }));
    }
    // Two treatments on one straight edge must not overlap; meeting exactly is allowed — the
    // straight piece between them vanishes (two equal fillets there are a half circle; the
    // kernel's normal form merges them).
    for i in 0..n {
        let j = (i + 1) % n;
        if let (Item::Vertex { at: p, .. }, Item::Vertex { at: q, .. }, Some(ti), Some(tj)) =
            (&items[i], &items[j], &treats[i], &treats[j])
        {
            // `ti.after` is on `p → q`, `tj.before` is on `q → p`; they must not cross.
            let along = |x: P| -> Rat {
                match axis_of(*p, *q) {
                    Some(Axis2::Horizontal) | None => {
                        if q[0] >= p[0] {
                            x[0]
                        } else {
                            Rat::from_int(0).checked_sub(x[0]).unwrap_or(x[0])
                        }
                    }
                    Some(Axis2::Vertical) => {
                        if q[1] >= p[1] {
                            x[1]
                        } else {
                            Rat::from_int(0).checked_sub(x[1]).unwrap_or(x[1])
                        }
                    }
                }
            };
            if along(ti.after) > along(tj.before) {
                return Err(program(
                    step,
                    format!(
                        "the corner treatments at ({}, {}) and ({}, {}) overlap on their shared \
                         edge — together they need less than {}",
                        f2(*p)[0],
                        f2(*p)[1],
                        f2(*q)[0],
                        f2(*q)[1],
                        dist2(*p, *q).map(|d| d.to_f64().sqrt()).unwrap_or(f64::NAN)
                    ),
                ));
            }
        }
    }

    // 4. Emit the ring: a vertex and the edge leaving it, in pen order — the straight edges
    //    between consecutive vertices (shortened by their treatments, dropped when a treatment
    //    ate them whole), the treatments' own edges, the arcs as stated. The ring closes on its
    //    first vertex, so the last edge needs no end of its own. The kernel's door re-checks
    //    every arc against its vertices and applies the normal form.
    let mut vertices: Vec<P> = Vec::with_capacity(n);
    let mut edges: Vec<Edge2d> = Vec::with_capacity(n);
    for i in 0..n {
        match &items[i] {
            Item::Arc { from, edge } => {
                vertices.push(*from);
                edges.push(*edge);
            }
            Item::Vertex { at: p, .. } => {
                if let Some(t) = &treats[i]
                    && let Some(e) = &t.emit
                {
                    vertices.push(t.before);
                    edges.push(*e);
                }
                let j = (i + 1) % n;
                let Item::Vertex { at: q, .. } = &items[j] else {
                    continue; // the next item is an arc that starts at `p`
                };
                let from = treats[i].as_ref().map(|t| t.after).unwrap_or(*p);
                let to = treats[j].as_ref().map(|t| t.before).unwrap_or(*q);
                if from != to {
                    vertices.push(from);
                    edges.push(Edge2d::Line);
                }
            }
        }
    }
    kernel(step, Ring2d::new(vertices, edges))
}

/// The kernel's profiles for a sketch — the ring court, sitting once.
pub(crate) fn classify(step: usize, paths: &[Path]) -> Result<Vec<Profile2d>, KitError> {
    let rings = lower(step, paths)?;
    from_paths(rings).map_err(|e| translate(step, e))
}

/// Display segments for a sketch: a line as itself, an arc as eight chords a quarter turn — a
/// picture, not the truth (the truth is the `Ring2d`).
pub(crate) fn sample(rings: &[Ring2d]) -> Vec<[[f64; 2]; 2]> {
    use std::f64::consts::{FRAC_PI_2, TAU};
    let mut out = Vec::new();
    for ring in rings {
        let (vs, es) = (ring.vertices(), ring.edges());
        let n = vs.len();
        for i in 0..n {
            let (s, t) = (f2(vs[i]), f2(vs[(i + 1) % n]));
            match &es[i] {
                Edge2d::Line => out.push([s, t]),
                Edge2d::Arc { center, ccw, .. } => {
                    // A ring of one vertex is a whole circle: the arc leaves the seam and lands
                    // on it.
                    let whole = n == 1;
                    let c = f2(*center);
                    let r = ((s[0] - c[0]).powi(2) + (s[1] - c[1]).powi(2)).sqrt();
                    let a0 = (s[1] - c[1]).atan2(s[0] - c[0]);
                    let mut sweep = if whole {
                        TAU
                    } else {
                        let a1 = (t[1] - c[1]).atan2(t[0] - c[0]);
                        (a1 - a0).rem_euclid(TAU)
                    };
                    if !*ccw {
                        sweep -= TAU;
                    }
                    let chords = ((sweep.abs() / FRAC_PI_2).ceil() as usize * 8).max(1);
                    let at = |k: usize| {
                        let a = a0 + sweep * (k as f64) / (chords as f64);
                        [c[0] + r * a.cos(), c[1] + r * a.sin()]
                    };
                    let mut prev = s;
                    for k in 1..=chords {
                        let p = if k == chords && !whole { t } else { at(k) };
                        out.push([prev, p]);
                        prev = p;
                    }
                }
            }
        }
    }
    out
}

fn translate(step: usize, e: SketchError) -> KitError {
    let at = |p: [f64; 2]| format!("({}, {})", p[0], p[1]);
    let what = match e {
        SketchError::RingSelfIntersects { at: [a, b], .. } => format!(
            "a path crosses itself — the segments near {} and {} meet",
            at(a),
            at(b)
        ),
        SketchError::RingsMeet { a, b } => format!(
            "two closed paths touch or cross (ring {a} and ring {b}, in drawing order) — \
             a hole must lie strictly inside, an island strictly outside its hole"
        ),
        SketchError::ZeroLengthEdge { .. } => "a line starts where it ends".to_string(),
        SketchError::DegenerateRing { .. } => "a ring with fewer than three points".to_string(),
        SketchError::OutsideDecimalWindow { at: p } => {
            format!("{} is outside the exact decimal window", at(p))
        }
        SketchError::ArcEndOffCircle { center, start, end } => format!(
            "the arc about {} from {} cannot end at {} — that point is not on its circle",
            at(center),
            at(start),
            at(end)
        ),
        SketchError::ArcTurnsOutOfRange { turns } => format!(
            "an arc turns 1 to 3 quarters either way, got {turns} (a whole circle is `circle`)"
        ),
        SketchError::NonPositiveRadius { center, radius } => {
            format!(
                "a circle about {} needs a positive radius, got {radius}",
                at(center)
            )
        }
        SketchError::ZeroLengthArc { at: p } => {
            format!(
                "an arc from {} back to itself is a whole circle — say `circle`",
                at(p)
            )
        }
        SketchError::Undecidable => {
            "the sketch's numbers overflow the exact arithmetic — undecidable, not guessed"
                .to_string()
        }
        other => format!("{other:?}"),
    };
    KitError::Kernel {
        step,
        what,
        class: None,
        blame: None,
        mark: None,
    }
}
