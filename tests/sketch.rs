//! Pen, chamfers, and the kernel's ring court through the kit surface.

use nacre_kit::{
    Corner, KitError, Path, PenPath, PlaneRef, SketchSeg, Step, ValueId, WorldPlane, build,
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

fn sketch(paths: Vec<Path>) -> Step {
    Step::Sketch {
        plane: PlaneRef::World(WorldPlane::XY),
        paths,
    }
}

fn extrude(sketch: u32, dist: f64) -> Step {
    Step::Extrude {
        sketch: ValueId(sketch),
        dist: nacre_kit::Dist::One(dist),
    }
}

fn volume_of(steps: Vec<Step>, i: usize) -> f64 {
    let out = build(&steps, None).expect("build");
    out.values[i]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("a solid")
        .volume(&out.model)
}

fn err_of(steps: Vec<Step>) -> String {
    match build(&steps, None) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("expected a rejection"),
    }
}

/// ② Holes and islands, undeclared: three nested squares — material, hole, island —
/// classified by the kernel's containment court. Islands become separate bodies.
#[test]
fn holes_and_islands_come_from_nesting() {
    let sq = |a: f64, b: f64| PenPath {
        start: [a, a],
        segs: vec![line_to([b, a]), line_to([b, b]), line_to([a, b])],
        close_corner: None,
    };
    let steps = vec![
        sketch(vec![
            Path::Pen(sq(0.0, 20.0)),
            Path::Pen(sq(4.0, 16.0)),
            Path::Pen(sq(8.0, 12.0)),
        ]),
        extrude(0, 2.0),
    ];
    let out = build(&steps, None).expect("build");
    let val = out.values[1]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("solid");
    assert_eq!(val.body_count(), 2, "outer-with-hole, and the island");
    let want = ((400.0 - 144.0) + 16.0) * 2.0;
    assert!(
        (val.volume(&out.model) - want).abs() < 1e-9,
        "got {}, want {want}",
        val.volume(&out.model)
    );
}

/// ④ The court's verdicts arrive with coordinates, in words.
#[test]
fn rejections_carry_positions() {
    // A bowtie: the path crosses itself.
    let bowtie = err_of(vec![sketch(vec![Path::Pen(PenPath {
        start: [0.0, 0.0],
        segs: vec![
            line_to([2.0, 2.0]),
            line_to([2.0, 0.0]),
            line_to([0.0, 2.0]),
        ],
        close_corner: None,
    })])]);
    assert!(bowtie.contains("crosses itself"), "{bowtie}");

    // Two rings crossing.
    let meet = err_of(vec![sketch(vec![
        Path::Pen(pen_rect(4.0, 3.0)),
        Path::Pen({
            let mut p = pen_rect(4.0, 3.0);
            p.start = [2.0, 1.0];
            p.segs = vec![
                line_to([6.0, 1.0]),
                line_to([6.0, 2.0]),
                line_to([2.0, 2.0]),
            ];
            p
        }),
    ])]);
    assert!(meet.contains("touch or cross"), "{meet}");
}

/// ⑤ Chamfers: declared at the corner, computed by the kit against the sharp ring.
/// A 45° cut removes exactly d²/2 per corner — and on axis-aligned edges the retreat
/// is rational, so the volume is exact.
#[test]
fn chamfers_cut_the_corners() {
    let all = Some(Corner::Chamfer(2.0));
    let steps = vec![
        sketch(vec![Path::Pen(PenPath {
            start: [0.0, 0.0],
            segs: vec![
                SketchSeg::LineTo {
                    to: [10.0, 0.0],
                    corner: all,
                },
                SketchSeg::LineTo {
                    to: [10.0, 8.0],
                    corner: all,
                },
                SketchSeg::LineTo {
                    to: [0.0, 8.0],
                    corner: all,
                },
            ],
            close_corner: all,
        })]),
        extrude(0, 3.0),
    ];
    let want = (10.0 * 8.0 - 4.0 * 2.0) * 3.0; // 4 corners × d²/2 = 8
    let got = volume_of(steps, 1);
    assert!(
        (got - want).abs() < 1e-12,
        "got {got}, want {want} — exactly"
    );
}

/// ⑤ The chamfer rejections say which corner and how much.
#[test]
fn chamfer_rejections_name_the_corner() {
    // Too big for its edge.
    let big = err_of(vec![sketch(vec![Path::Pen(PenPath {
        start: [0.0, 0.0],
        segs: vec![
            SketchSeg::LineTo {
                to: [10.0, 0.0],
                corner: Some(Corner::Chamfer(9.0)),
            },
            SketchSeg::LineTo {
                to: [10.0, 4.0],
                corner: None,
            },
            line_to([0.0, 4.0]),
        ],
        close_corner: None,
    })])]);
    assert!(big.contains("does not fit"), "{big}");
    assert!(big.contains("(10, 0)"), "the corner is named: {big}");

    // Each fits alone; together they overlap the shared edge.
    let overlap = err_of(vec![sketch(vec![Path::Pen(PenPath {
        start: [0.0, 0.0],
        segs: vec![
            SketchSeg::LineTo {
                to: [10.0, 0.0],
                corner: Some(Corner::Chamfer(6.0)),
            },
            SketchSeg::LineTo {
                to: [10.0, 8.0],
                corner: Some(Corner::Chamfer(6.0)),
            },
            line_to([0.0, 8.0]),
        ],
        close_corner: None,
    })])]);
    assert!(overlap.contains("shared edge"), "{overlap}");

    // A straight corner has nothing to chamfer.
    let straight = err_of(vec![sketch(vec![Path::Pen(PenPath {
        start: [0.0, 0.0],
        segs: vec![
            SketchSeg::LineTo {
                to: [5.0, 0.0],
                corner: Some(Corner::Chamfer(1.0)),
            },
            line_to([10.0, 0.0]),
            line_to([10.0, 4.0]),
            line_to([0.0, 4.0]),
        ],
        close_corner: None,
    })])]);
    assert!(straight.contains("straight"), "{straight}");
}

/// ⑥ What the vocabulary cannot state exactly is refused in words, and ambiguous pen
/// input is refused. A fillet off a right angle, an arc off the quarter grid; a repeated
/// point, an empty sketch. A `lineTo` back to the start is **not** ambiguous:
/// `close()` draws nothing when the pen is already home.
#[test]
fn inexact_and_ambiguous_input_is_refused() {
    let fillet = err_of(vec![sketch(vec![Path::Pen(PenPath {
        start: [0.0, 0.0],
        segs: vec![
            SketchSeg::LineTo {
                to: [4.0, 0.0],
                corner: Some(Corner::Fillet(1.0)),
            },
            line_to([0.0, 3.0]), // the corner at (4,0) is not a right angle
        ],
        close_corner: None,
    })])]);
    assert!(fillet.contains("right angle"), "{fillet}");
    assert!(fillet.contains("chamfer works"), "{fillet}");

    let arc = err_of(vec![sketch(vec![Path::Pen(PenPath {
        start: [0.0, 0.0],
        segs: vec![
            line_to([4.0, 0.0]),
            SketchSeg::Arc {
                center: [4.0, 3.0],
                sweep: 45.0,
            },
            line_to([0.0, 3.0]),
        ],
        close_corner: None,
    })])]);
    assert!(arc.contains("multiple of 90"), "{arc}");

    let home = vec![
        sketch(vec![Path::Pen(PenPath {
            start: [0.0, 0.0],
            segs: vec![
                line_to([4.0, 0.0]),
                line_to([4.0, 3.0]),
                line_to([0.0, 3.0]),
                line_to([0.0, 0.0]), // back at start — close() draws nothing more
            ],
            close_corner: None,
        })]),
        extrude(0, 2.0),
    ];
    let plain = vec![sketch(vec![Path::Pen(pen_rect(4.0, 3.0))]), extrude(0, 2.0)];
    assert_eq!(
        volume_of(home, 1).to_bits(),
        volume_of(plain, 1).to_bits(),
        "a path that walks home closes without a doubled edge"
    );

    let repeat = err_of(vec![sketch(vec![Path::Pen(PenPath {
        start: [0.0, 0.0],
        segs: vec![
            line_to([4.0, 0.0]),
            line_to([4.0, 0.0]),
            line_to([0.0, 3.0]),
        ],
        close_corner: None,
    })])]);
    assert!(repeat.contains("repeats the point"), "{repeat}");

    let empty = err_of(vec![sketch(vec![])]);
    assert!(empty.contains("empty sketch"), "{empty}");
}

/// ⑦ A sketch is pure data: extruding it twice consumes nothing and copies nothing.
#[test]
fn a_sketch_is_reused_without_copies() {
    let steps = vec![
        sketch(vec![Path::Pen(pen_rect(2.0, 2.0))]),
        extrude(0, 1.0),
        extrude(0, 3.0),
    ];
    let out = build(&steps, None).expect("build");
    assert!(out.auto_copies.is_empty());
    for (i, want) in [(1usize, 4.0), (2, 12.0)] {
        let v = out.values[i]
            .as_ref()
            .and_then(|v| v.as_solid())
            .expect("solid");
        assert!((v.volume(&out.model) - want).abs() < 1e-9);
    }
}

/// Kind mismatches are named before the kernel is asked.
#[test]
fn kind_mismatches_are_program_errors() {
    // A solid where a sketch is needed.
    let steps = vec![
        Step::Cuboid {
            size: [1.0, 1.0, 1.0],
            at: nacre_kit::Anchor::Corner([0.0, 0.0, 0.0]),
        },
        extrude(0, 1.0),
    ];
    assert!(matches!(
        build(&steps, None),
        Err(KitError::Program { step: 1, .. })
    ));
    // A sketch *can* be displayed — it is a thing you drew. A plane cannot: it has no
    // extent, only a statement about where things sit.
    let steps = vec![
        sketch(vec![Path::Pen(pen_rect(1.0, 1.0))]),
        Step::Display {
            targets: vec![ValueId(0)],
            style: None,
        },
    ];
    let out = build(&steps, None).expect("a sketch is displayable");
    assert_eq!(out.rendered, vec![ValueId(0)]);

    let steps = vec![
        Step::Plane {
            spec: nacre_kit::PlaneSpec::World(WorldPlane::XY),
        },
        Step::Display {
            targets: vec![ValueId(0)],
            style: None,
        },
    ];
    let msg = err_of(steps);
    assert!(msg.contains("no extent to draw"), "{msg}");
}
