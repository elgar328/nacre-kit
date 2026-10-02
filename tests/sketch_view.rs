//! A sketch is a thing you can look at.
//!
//! Two propositions, and neither is about coordinates being pretty: a sketch is drawn
//! until something is made out of it, and where it is drawn is *on its own plane* —
//! which for a plane through three named vertices only the kernel can say.

use nacre_kit::{
    Anchor, Dist, Path, PenPath, PlaneRef, PlaneSpec, SketchSeg, Step, ValueId, VertexRef,
    WorldPlane, build,
};

fn line_to(to: [f64; 2]) -> SketchSeg {
    SketchSeg::LineTo { to, corner: None }
}

fn rect(w: f64, h: f64) -> PenPath {
    PenPath {
        start: [0.0, 0.0],
        segs: vec![line_to([w, 0.0]), line_to([w, h]), line_to([0.0, h])],
        close_corner: None,
    }
}

fn sketch_on(plane: PlaneRef) -> Step {
    Step::Sketch {
        plane,
        paths: vec![Path::Pen(rect(4.0, 3.0))],
    }
}

/// ① A sketch nobody has used yet is a result: it is drawn.
#[test]
fn an_unused_sketch_is_drawn() {
    let out = build(&[sketch_on(PlaneRef::World(WorldPlane::XY))], None).expect("build");
    assert_eq!(out.rendered, vec![ValueId(0)]);
}

/// ② Extruding hands the screen to the solid — and note this cannot be "was it
/// consumed", because an extrude does not consume its sketch (they are pure data,
/// reused freely). The question is whether anything was made out of it.
#[test]
fn extruding_a_sketch_takes_the_screen_from_it() {
    let steps = vec![
        sketch_on(PlaneRef::World(WorldPlane::XY)),
        Step::Extrude {
            sketch: ValueId(0),
            dist: Dist::One(2.0),
        },
    ];
    let out = build(&steps, None).expect("build");
    assert_eq!(out.rendered, vec![ValueId(1)], "the solid, not the sketch");

    // ③ Cut the same program before the extrude and the sketch is what there is —
    // stepping through a script shows the sketch, then what became of it.
    let out = build(&steps, Some(1)).expect("prefix build");
    assert_eq!(out.rendered, vec![ValueId(0)]);
}

/// ④ Where the lines are: on the plane the sketch was drawn on. Measured against the
/// plane rather than against expected numbers — for a plane through three named
/// vertices the axes are the kernel's to choose, and predicting them from the
/// vertices' coordinates is the mistake `through` exists to prevent.
#[test]
fn a_sketch_is_drawn_on_its_own_plane() {
    // Three corners of a box, giving a plane at a slant.
    let cube = Step::Cuboid {
        size: [4.0, 4.0, 4.0],
        at: Anchor::Corner([0.0, 0.0, 0.0]),
    };
    let out = build(std::slice::from_ref(&cube), None).expect("build");
    let pick = |at: [f64; 3]| {
        let v = out
            .vertices_of(ValueId(0))
            .expect("solid")
            .into_iter()
            .find(|v| v.at == at)
            .expect("corner");
        VertexRef {
            of: ValueId(0),
            vertex: v.vertex,
        }
    };
    let vs = [
        pick([4.0, 0.0, 0.0]),
        pick([0.0, 4.0, 0.0]),
        pick([0.0, 0.0, 4.0]),
    ];
    let steps = vec![
        cube,
        Step::Plane {
            spec: PlaneSpec::Through { vertices: vs },
        },
        sketch_on(PlaneRef::Value(ValueId(1))),
    ];
    let out = build(&steps, None).expect("build");
    let lines = out.sketch_lines(ValueId(2)).expect("a sketch has lines");
    assert_eq!(lines.len(), 4, "a rectangle is four segments");

    // The plane through those three corners is x + y + z = 4; every drawn point is on it.
    for [a, b] in &lines {
        for p in [a, b] {
            let sum = p[0] + p[1] + p[2];
            assert!(
                (sum - 4.0).abs() < 1e-9,
                "{p:?} is off the plane (x+y+z={sum})"
            );
        }
    }
    // And the drawn rectangle is the size it was drawn: 4 by 3.
    let len = |[a, b]: &[[f64; 3]; 2]| {
        ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt()
    };
    let mut sides: Vec<f64> = lines.iter().map(len).collect();
    sides.sort_by(|x, y| x.partial_cmp(y).unwrap());
    assert!((sides[0] - 3.0).abs() < 1e-9 && (sides[3] - 4.0).abs() < 1e-9);
}

/// ⑤ Chamfers are already resolved in what comes back: what you see is what will be
/// extruded.
#[test]
fn the_lines_are_the_ring_that_will_be_extruded() {
    let steps = vec![Step::Sketch {
        plane: PlaneRef::World(WorldPlane::XY),
        paths: vec![Path::Pen(PenPath {
            start: [0.0, 0.0],
            segs: vec![
                SketchSeg::LineTo {
                    to: [10.0, 0.0],
                    corner: Some(nacre_kit::Corner::Chamfer(2.0)),
                },
                line_to([10.0, 8.0]),
                line_to([0.0, 8.0]),
            ],
            close_corner: None,
        })],
    }];
    let out = build(&steps, None).expect("build");
    let lines = out.sketch_lines(ValueId(0)).expect("lines");
    assert_eq!(
        lines.len(),
        5,
        "the chamfer replaced a corner with a segment"
    );
    assert!(
        !lines
            .iter()
            .flatten()
            .any(|p| (p[0] - 10.0).abs() < 1e-12 && p[1].abs() < 1e-12),
        "the sharp corner is gone — the chamfer is what will be built"
    );
}

/// ⑥ Only sketches have sketch lines.
#[test]
fn other_values_have_no_sketch_lines() {
    let steps = vec![Step::Cuboid {
        size: [1.0, 1.0, 1.0],
        at: Anchor::Corner([0.0, 0.0, 0.0]),
    }];
    let out = build(&steps, None).expect("build");
    assert!(out.sketch_lines(ValueId(0)).is_none());
    assert!(out.sketch_lines(ValueId(9)).is_none());
}
