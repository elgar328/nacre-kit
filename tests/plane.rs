//! Planes as values (world, moved, three-point, offset), the sketch that
//! lives on one, and the queries that pick by appearance.

use nacre_kit::{
    Anchor, Dist, Path, PenPath, PlaneRef, PlaneSpec, SketchSeg, Step, ValueId, WorldPlane, build,
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

fn extrude(sketch: u32, dist: Dist) -> Step {
    Step::Extrude {
        sketch: ValueId(sketch),
        dist,
    }
}

fn volume_of(steps: &[Step], i: usize) -> f64 {
    let out = build(steps, None).expect("build");
    out.values[i]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("a solid")
        .volume(&out.model)
}

fn err_of(steps: &[Step]) -> String {
    match build(steps, None) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("expected a rejection"),
    }
}

/// ① A world plane **value** builds the same solid as the K2 world-plane sketch, bit
/// for bit — in both directions and two-sided. This is also the no-breakage proof for
/// folding K1's world-only flipped road into the general one.
#[test]
fn a_world_plane_value_is_the_world_plane() {
    for dist in [Dist::One(2.0), Dist::One(-2.0), Dist::Both(-1.0, 2.0)] {
        let direct = vec![
            sketch_on(PlaneRef::World(WorldPlane::ZX), 4.0, 3.0),
            extrude(0, dist),
        ];
        let valued = vec![
            Step::Plane {
                spec: PlaneSpec::World(WorldPlane::ZX),
            },
            sketch_on(PlaneRef::Value(ValueId(0)), 4.0, 3.0),
            extrude(1, dist),
        ];
        assert_eq!(
            volume_of(&direct, 1).to_bits(),
            volume_of(&valued, 2).to_bits(),
            "the two spellings build the same model ({dist:?})"
        );
    }
}

/// ② Sketches live on moved and tilted planes: a prism's volume is area × height
/// regardless of the plane's attitude, and a moved origin moves the solid.
#[test]
fn moved_and_tilted_planes_carry_sketches() {
    // plane(XY, { origin: [0, 0, 5] }) — same axes, lifted origin.
    let steps = vec![
        Step::Plane {
            spec: PlaneSpec::WorldAt {
                base: WorldPlane::XY,
                origin: [0.0, 0.0, 5.0],
            },
        },
        sketch_on(PlaneRef::Value(ValueId(0)), 4.0, 3.0),
        extrude(1, Dist::One(2.0)),
    ];
    let out = build(&steps, None).expect("build");
    let val = out.values[2]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("solid");
    assert!((val.volume(&out.model) - 24.0).abs() < 1e-9);
    let (lo, hi) = nacre::props::bounds(&out.model, val.bodies[0]).expect("bounds");
    assert!(
        (lo.as_array()[2] - 5.0).abs() < 1e-12 && (hi.as_array()[2] - 7.0).abs() < 1e-12,
        "the prism sits on z = 5, got z in [{}, {}]",
        lo.as_array()[2],
        hi.as_array()[2]
    );

    // plane({ origin, xPoint, yHint }) — a tilted plane; the prism volume is exact
    // area × dist no matter the tilt.
    let steps = vec![
        Step::Plane {
            spec: PlaneSpec::Points {
                origin: [0.0, 0.0, 0.0],
                x_point: [1.0, 0.0, 1.0],
                y_hint: [0.0, 1.0, 0.0],
            },
        },
        sketch_on(PlaneRef::Value(ValueId(0)), 2.0, 3.0),
        extrude(1, Dist::One(1.5)),
    ];
    assert!((volume_of(&steps, 2) - 9.0).abs() < 1e-9);
}

/// ③ `plane(p, { offset })` interns onto the very plane a box top already lies on —
/// the same kernel handle, not a second plane an ulp away — and a sketch on it sits
/// at the offset; a **negative** extrude there exercises the flipped-offset road.
#[test]
fn an_offset_plane_interns_onto_the_box_top() {
    let steps = vec![
        Step::Cuboid {
            size: [4.0, 4.0, 2.0],
            at: Anchor::Corner([0.0, 0.0, 0.0]),
        },
        Step::Plane {
            spec: PlaneSpec::World(WorldPlane::XY),
        },
        Step::Plane {
            spec: PlaneSpec::Offset {
                base: ValueId(1),
                dist: 2.0,
            },
        },
        sketch_on(PlaneRef::Value(ValueId(2)), 1.0, 1.0),
        extrude(3, Dist::One(1.0)),
        extrude(3, Dist::One(-1.0)),
    ];
    let out = build(&steps, None).expect("build");
    let plane = out.values[2]
        .as_ref()
        .and_then(|v| v.as_plane())
        .expect("a plane value")
        .plane;
    // The box top's surface, found by appearance (normal +Z, center z = 2).
    let top = out
        .faces_of(ValueId(0))
        .expect("solid faces")
        .into_iter()
        .find(|f| f.normal == Some([0.0, 0.0, 1.0]))
        .expect("a top face");
    let top_surface = out
        .model
        .face(out.model.face_handle_at(top.face).expect("walked index"))
        .surface;
    assert_eq!(plane, top_surface, "one plane, one handle");

    // The sketch on the offset plane extrudes from z = 2 — up and (flipped road) down.
    for (i, want) in [(4usize, (2.0, 3.0)), (5, (1.0, 2.0))] {
        let val = out.values[i]
            .as_ref()
            .and_then(|v| v.as_solid())
            .expect("solid");
        let (lo, hi) = nacre::props::bounds(&out.model, val.bodies[0]).expect("bounds");
        assert!(
            (lo.as_array()[2] - want.0).abs() < 1e-12 && (hi.as_array()[2] - want.1).abs() < 1e-12,
            "prism {i} sits at z in [{}, {}], want {want:?}",
            lo.as_array()[2],
            hi.as_array()[2]
        );
    }
}

/// ④ The queries: a box reports its eight vertices and six faces, with the report
/// values a selector would filter on.
#[test]
fn queries_report_what_selectors_pick() {
    let steps = vec![Step::Cuboid {
        size: [4.0, 4.0, 2.0],
        at: Anchor::Corner([0.0, 0.0, 0.0]),
    }];
    let out = build(&steps, None).expect("build");

    let vs = out.vertices_of(ValueId(0)).expect("vertices");
    assert_eq!(vs.len(), 8);
    let mut coords: Vec<[f64; 3]> = vs.iter().map(|v| v.at).collect();
    coords.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut want: Vec<[f64; 3]> = (0..8)
        .map(|k| {
            [
                if k & 1 == 0 { 0.0 } else { 4.0 },
                if k & 2 == 0 { 0.0 } else { 4.0 },
                if k & 4 == 0 { 0.0 } else { 2.0 },
            ]
        })
        .collect();
    want.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(coords, want);

    let fs = out.faces_of(ValueId(0)).expect("faces");
    assert_eq!(fs.len(), 6);
    let caps: Vec<_> = fs.iter().filter(|f| (f.area - 16.0).abs() < 1e-9).collect();
    let sides = fs.iter().filter(|f| (f.area - 8.0).abs() < 1e-9).count();
    assert_eq!((caps.len(), sides), (2, 4), "two 4×4 caps, four 4×2 sides");
    let top = fs
        .iter()
        .find(|f| f.normal == Some([0.0, 0.0, 1.0]))
        .expect("top face");
    assert_eq!(top.center, [2.0, 2.0, 2.0]);
}

/// ⑤ Queries answer only for solids — a sketch or plane value is `None`.
#[test]
fn queries_refuse_non_solids() {
    let steps = vec![
        sketch_on(PlaneRef::World(WorldPlane::XY), 1.0, 1.0),
        Step::Plane {
            spec: PlaneSpec::World(WorldPlane::XY),
        },
    ];
    let out = build(&steps, None).expect("build");
    for id in [ValueId(0), ValueId(1)] {
        assert!(out.vertices_of(id).is_none());
        assert!(out.faces_of(id).is_none());
    }
}

/// ⑥ A three-point plane's opposite frame is the user's statement to make — the
/// backward extrude rejects with the swap guidance, one-sided and two-sided alike.
#[test]
fn a_points_plane_extrudes_forward_only() {
    // The two spellings decline for **different** reasons, and each says its own.
    // A backward extrude wants the plane stated the other way round; a range wants it stated
    // where the range starts. Both are the user's statement to make, not the kit's to invent.
    for (dist, guidance) in [
        (Dist::One(-1.0), "xPoint and yHint swapped"),
        (Dist::Both(-1.0, 1.0), "where the range starts"),
    ] {
        let steps = vec![
            Step::Plane {
                spec: PlaneSpec::Points {
                    origin: [0.0, 0.0, 0.0],
                    x_point: [1.0, 0.0, 1.0],
                    y_hint: [0.0, 1.0, 0.0],
                },
            },
            sketch_on(PlaneRef::Value(ValueId(0)), 1.0, 1.0),
            extrude(1, dist),
        ];
        let msg = err_of(&steps);
        assert!(
            msg.contains(guidance),
            "the guidance is the statement itself: {msg}"
        );
    }
}

/// ⑥b An offset over a three-point plane has no exact re-statement to pin its frame —
/// honest reject at the statement, not a silently-unpinned direction later.
#[test]
fn an_offset_over_a_points_plane_is_refused() {
    let steps = vec![
        Step::Plane {
            spec: PlaneSpec::Points {
                origin: [0.0, 0.0, 0.0],
                x_point: [1.0, 0.0, 1.0],
                y_hint: [0.0, 1.0, 0.0],
            },
        },
        Step::Plane {
            spec: PlaneSpec::Offset {
                base: ValueId(0),
                dist: 2.0,
            },
        },
    ];
    let msg = err_of(&steps);
    assert!(msg.contains("irrational"), "{msg}");
}

/// ⑦ Degenerate point statements name no plane.
#[test]
fn collinear_points_name_no_plane() {
    let steps = vec![Step::Plane {
        spec: PlaneSpec::Points {
            origin: [0.0, 0.0, 0.0],
            x_point: [1.0, 0.0, 0.0],
            y_hint: [2.0, 0.0, 0.0],
        },
    }];
    let msg = err_of(&steps);
    assert!(msg.contains("name no plane"), "{msg}");
}
