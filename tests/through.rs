//! `plane({through})`: the plane through three **named** vertices, picked by
//! appearance from a query, stated by name. The test plays the TS selector's role:
//! read `vertices_of`, choose by coordinates, record the indices.

use nacre_kit::{
    Anchor, Dist, KitAxis, KitBool, Path, PenPath, Pivot, PlaneRef, PlaneSpec, SketchSeg, Step,
    ValueId, VertexRef, build,
};

fn line_to(to: [f64; 2]) -> SketchSeg {
    SketchSeg::LineTo { to, corner: None }
}

fn pen_rect(w: f64, h: f64) -> PenPath {
    PenPath {
        start: [0.0, 0.0],
        segs: vec![line_to([w, 0.0]), line_to([w, h]), line_to([0.0, h])],
        close_corner: None,
    }
}

fn sketch_on(plane: PlaneRef, w: f64, h: f64) -> Step {
    Step::Sketch {
        plane,
        paths: vec![Path::Pen(pen_rect(w, h))],
    }
}

fn extrude(sketch: u32, dist: f64) -> Step {
    Step::Extrude {
        sketch: ValueId(sketch),
        dist: Dist::One(dist),
    }
}

fn cube4() -> Step {
    Step::Cuboid {
        size: [4.0, 4.0, 4.0],
        at: Anchor::Corner([0.0, 0.0, 0.0]),
    }
}

/// Pick the index of the vertex at `at` (exact report match — a box's corners are
/// exact decimals).
fn pick(out: &nacre_kit::BuildOutput, of: u32, at: [f64; 3]) -> VertexRef {
    let v = out
        .vertices_of(ValueId(of))
        .expect("solid")
        .into_iter()
        .find(|v| v.at == at)
        .expect("the corner exists");
    VertexRef {
        of: ValueId(of),
        vertex: v.vertex,
    }
}

/// ① The plane through three cube corners carries a sketch, and the prism's volume is
/// area × dist regardless of the plane's tilt.
#[test]
fn a_through_plane_carries_a_sketch() {
    let prefix = vec![cube4()];
    let out = build(&prefix, None).expect("build");
    let vs = [
        pick(&out, 0, [4.0, 0.0, 0.0]),
        pick(&out, 0, [0.0, 4.0, 0.0]),
        pick(&out, 0, [0.0, 0.0, 4.0]),
    ];
    let steps = vec![
        cube4(),
        Step::Plane {
            spec: PlaneSpec::Through { vertices: vs },
        },
        sketch_on(PlaneRef::Value(ValueId(1)), 1.0, 1.0),
        extrude(2, 1.0),
    ];
    let out = build(&steps, None).expect("build");
    let val = out.values[3]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("solid");
    assert!(
        (val.volume(&out.model) - 1.0).abs() < 1e-9,
        "a 1×1×1 prism on the tilted plane, got {}",
        val.volume(&out.model)
    );
}

/// ② The order **is** the direction: the same three corners in swapped order are the
/// same interned surface (one handle) with the opposite frame — the two prisms leave
/// the plane on opposite sides.
#[test]
fn vertex_order_is_the_normal() {
    let prefix = vec![cube4()];
    let out = build(&prefix, None).expect("build");
    let (a, b, c) = (
        pick(&out, 0, [4.0, 0.0, 0.0]),
        pick(&out, 0, [0.0, 4.0, 0.0]),
        pick(&out, 0, [0.0, 0.0, 4.0]),
    );
    let steps = vec![
        cube4(),
        Step::Plane {
            spec: PlaneSpec::Through {
                vertices: [a, b, c],
            },
        },
        Step::Plane {
            spec: PlaneSpec::Through {
                vertices: [a, c, b], // two swapped — same plane, opposite normal
            },
        },
        sketch_on(PlaneRef::Value(ValueId(1)), 1.0, 1.0),
        sketch_on(PlaneRef::Value(ValueId(2)), 1.0, 1.0),
        extrude(3, 1.0),
        extrude(4, 1.0),
    ];
    let out = build(&steps, None).expect("build");
    let plane_of = |i: usize| {
        out.values[i]
            .as_ref()
            .and_then(|v| v.as_plane())
            .expect("plane")
            .plane
    };
    assert_eq!(plane_of(1), plane_of(2), "one plane, one handle");

    // The plane is x + y + z = 4; (a, b, c) faces away from the origin by the
    // right-hand rule, the swapped order faces toward it.
    let side = |i: usize| {
        let val = out.values[i]
            .as_ref()
            .and_then(|v| v.as_solid())
            .expect("solid");
        let c = nacre::props::centroid(&out.model, val.bodies[0]).expect("centroid");
        c.as_array().iter().sum::<f64>() - 4.0
    };
    assert!(side(5) > 0.0, "statement order leaves the origin's side");
    assert!(side(6) < 0.0, "swapped order returns toward the origin");
}

/// ③ The reason a plane through vertices exists: on a **rotated** solid the corners' report coordinates
/// are rounded, so a plane through the *coordinates* would be subtly wrong — the
/// plane through the *names* is exact. Rotation about Z keeps the top plane z = 1,
/// and the through-plane lands on it exactly (asserted to 1e-12); a boss extruded
/// there fuses onto the rotated body.
#[test]
fn named_vertices_beat_rounded_coordinates() {
    let prefix = vec![
        Step::Cuboid {
            size: [2.0, 2.0, 2.0],
            at: Anchor::Center([0.0, 0.0, 0.0]),
        },
        Step::Rotate {
            src: ValueId(0),
            axis: KitAxis::Z,
            deg: 30.0,
            pivot: Pivot::Origin,
        },
    ];
    let out = build(&prefix, None).expect("build");
    // Three top corners (z = 1 survives a rotation about Z), ordered so the
    // right-hand normal faces +Z — the selector's own little computation.
    let mut top: Vec<_> = out
        .vertices_of(ValueId(1))
        .expect("solid")
        .into_iter()
        .filter(|v| v.at[2] > 0.5)
        .collect();
    assert_eq!(top.len(), 4, "a box top has four corners");
    top.truncate(3);
    let (p, q, r) = (top[0].at, top[1].at, top[2].at);
    let cross_z = (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
    if cross_z < 0.0 {
        top.swap(1, 2);
    }
    let vs = [
        VertexRef {
            of: ValueId(1),
            vertex: top[0].vertex,
        },
        VertexRef {
            of: ValueId(1),
            vertex: top[1].vertex,
        },
        VertexRef {
            of: ValueId(1),
            vertex: top[2].vertex,
        },
    ];
    let steps = vec![
        prefix[0].clone(),
        prefix[1].clone(),
        Step::Plane {
            spec: PlaneSpec::Through { vertices: vs },
        },
        Step::Sketch {
            plane: PlaneRef::Value(ValueId(2)),
            paths: vec![Path::Pen(PenPath {
                start: [-0.25, -0.25],
                segs: vec![
                    line_to([0.25, -0.25]),
                    line_to([0.25, 0.25]),
                    line_to([-0.25, 0.25]),
                ],
                close_corner: None,
            })],
        },
        extrude(3, 1.0),
        Step::Boolean {
            kind: nacre_kit::KitBool::Fuse,
            args: vec![ValueId(1), ValueId(4)],
        },
    ];
    let out = build(&steps, None).expect("build");

    // The boss sits exactly on z = 1 — the named plane, not a measured one.
    let boss = out.values[4]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("solid");
    let (lo, hi) = nacre::props::bounds(&out.model, boss.bodies[0]).expect("bounds");
    assert!(
        (lo.as_array()[2] - 1.0).abs() < 1e-12 && (hi.as_array()[2] - 2.0).abs() < 1e-12,
        "the boss rides z in [{}, {}] — exactly on the named plane",
        lo.as_array()[2],
        hi.as_array()[2]
    );

    let fused = out.values[5]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("solid");
    assert_eq!(fused.body_count(), 1, "the boss merged onto the body");
    assert!(
        (fused.volume(&out.model) - 8.25).abs() < 1e-6,
        "cube 8 + boss 0.25, got {}",
        fused.volume(&out.model)
    );
}

/// ④ Degenerate and foreign statements reject honestly: a repeated vertex, a vertex
/// of some other solid, backward extrudes.
#[test]
fn through_rejections_are_honest() {
    let prefix = vec![cube4()];
    let out = build(&prefix, None).expect("build");
    let a = pick(&out, 0, [4.0, 0.0, 0.0]);
    let b = pick(&out, 0, [0.0, 4.0, 0.0]);
    let c = pick(&out, 0, [0.0, 0.0, 4.0]);

    // A repeated vertex names no plane — the kernel's own reject, translated.
    let msg = err_of(vec![
        cube4(),
        Step::Plane {
            spec: PlaneSpec::Through {
                vertices: [a, a, b],
            },
        },
    ]);
    assert!(msg.contains("two points do not fix a plane"), "{msg}");
    assert!(
        msg.contains("[DuplicateVertex]"),
        "the handle to search with: {msg}"
    );

    // An in-range index that is no vertex of the named value is a program error.
    let far = VertexRef {
        of: ValueId(1),
        vertex: a.vertex,
    };
    let msg = err_of(vec![
        cube4(),
        Step::Cuboid {
            size: [1.0, 1.0, 1.0],
            at: Anchor::Corner([10.0, 10.0, 10.0]),
        },
        Step::Plane {
            spec: PlaneSpec::Through {
                vertices: [far, b, c],
            },
        },
    ]);
    assert!(msg.contains("not a vertex of value 1"), "{msg}");
}

/// ②b A backward extrude off a through-plane sweeps against its normal in the same frame: the
/// forward and backward prisms meet only on their base, so their fusion holds both.
#[test]
fn a_through_plane_extrudes_backward_too() {
    let prefix = vec![cube4()];
    let out = build(&prefix, None).expect("build");
    let vs = [
        pick(&out, 0, [4.0, 0.0, 0.0]),
        pick(&out, 0, [0.0, 4.0, 0.0]),
        pick(&out, 0, [0.0, 0.0, 4.0]),
    ];
    let steps = vec![
        cube4(),
        Step::Plane {
            spec: PlaneSpec::Through { vertices: vs },
        },
        sketch_on(PlaneRef::Value(ValueId(1)), 1.0, 1.0),
        extrude(2, 1.0),
        extrude(2, -1.0),
        Step::Boolean {
            kind: KitBool::Fuse,
            args: vec![ValueId(3), ValueId(4)],
        },
    ];
    let out = build(&steps, None).expect("build");
    let vol = |i: usize| {
        let v = out.values[i]
            .as_ref()
            .and_then(|v| v.as_solid())
            .expect("solid");
        (v.bodies.len(), v.volume(&out.model))
    };
    for (i, want) in [(3, (1, 1.0)), (4, (1, 1.0)), (5, (1, 2.0))] {
        let (n, v) = vol(i);
        assert!(
            n == want.0 && (v - want.1).abs() < 1e-9,
            "value {i}: {n} bodies, volume {v}"
        );
    }
}

fn err_of(steps: Vec<Step>) -> String {
    match build(&steps, None) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("expected a rejection"),
    }
}
