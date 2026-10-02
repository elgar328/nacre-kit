//! Pad and pocket on a face picked by appearance. The `top()` helper plays
//! the TS selector: read `faces_of`, filter by normal, record the index.

use nacre_kit::{
    Anchor, BuildOutput, FaceRef, KitBool, Path, PenPath, PlaneRef, SketchSeg, Step, ValueId,
    WorldPlane, build,
};

fn line_to(to: [f64; 2]) -> SketchSeg {
    SketchSeg::LineTo { to, corner: None }
}

fn unit_square() -> Step {
    Step::Sketch {
        plane: PlaneRef::World(WorldPlane::XY),
        paths: vec![Path::Pen(PenPath {
            start: [0.0, 0.0],
            segs: vec![
                line_to([1.0, 0.0]),
                line_to([1.0, 1.0]),
                line_to([0.0, 1.0]),
            ],
            close_corner: None,
        })],
    }
}

fn box_4x4x2() -> Step {
    Step::Cuboid {
        size: [4.0, 4.0, 2.0],
        at: Anchor::Corner([0.0, 0.0, 0.0]),
    }
}

/// The face of `of` facing +Z, picked by appearance (and by `pred` when several do).
fn top(out: &BuildOutput, of: u32, pred: impl Fn(&nacre_kit::FaceInfo) -> bool) -> FaceRef {
    let f = out
        .faces_of(ValueId(of))
        .expect("solid faces")
        .into_iter()
        .find(|f| f.normal == Some([0.0, 0.0, 1.0]) && pred(f))
        .expect("a top face");
    FaceRef {
        of: ValueId(of),
        face: f.face,
    }
}

fn solid_at(out: &BuildOutput, i: usize) -> &nacre_kit::SolidValue {
    out.values[i]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("a solid")
}

fn err_of(steps: Vec<Step>) -> String {
    match build(&steps, None) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("expected a rejection"),
    }
}

/// ① A boss padded on the top face adds its volume, and rides exactly on the face's
/// plane; ⑥ a re-query sees the boss's new faces.
#[test]
fn a_pad_adds_a_boss_on_the_face() {
    let prefix = vec![box_4x4x2()];
    let out = build(&prefix, None).expect("build");
    let face = top(&out, 0, |_| true);
    let steps = vec![
        box_4x4x2(),
        unit_square(),
        Step::Pad {
            face,
            sketch: ValueId(1),
            dist: 1.0,
        },
    ];
    let out = build(&steps, None).expect("build");
    let val = solid_at(&out, 2);
    assert!((val.volume(&out.model) - 33.0).abs() < 1e-9, "32 + 1 boss");
    let (lo, hi) = nacre::props::bounds(&out.model, val.bodies[0]).expect("bounds");
    assert!(
        (hi.as_array()[2] - 3.0).abs() < 1e-12 && lo.as_array()[2] == 0.0,
        "the boss rises from z = 2 to 3"
    );
    // ⑥ The query reflects the padded solid: more faces than a box, and the boss top.
    let fs = out.faces_of(ValueId(2)).expect("faces");
    assert!(fs.len() > 6, "the boss's faces joined the list");
    assert!(
        fs.iter()
            .any(|f| f.normal == Some([0.0, 0.0, 1.0]) && (f.center[2] - 3.0).abs() < 1e-9),
        "the boss's own top is queryable"
    );
}

/// ② A pocket removes its volume; carving deeper than the body is not blind and
/// rejects with the kernel's own name.
#[test]
fn a_pocket_carves_and_stays_blind() {
    let prefix = vec![box_4x4x2()];
    let out = build(&prefix, None).expect("build");
    let face = top(&out, 0, |_| true);
    let steps = vec![
        box_4x4x2(),
        unit_square(),
        Step::Pocket {
            face,
            sketch: ValueId(1),
            dist: 1.0,
        },
    ];
    let out = build(&steps, None).expect("build");
    assert!(
        (solid_at(&out, 2).volume(&out.model) - 31.0).abs() < 1e-9,
        "32 − 1"
    );

    let msg = err_of(vec![
        box_4x4x2(),
        unit_square(),
        Step::Pocket {
            face,
            sketch: ValueId(1),
            dist: 3.0,
        },
    ]);
    // A sentence **and** the identifier, never the name alone — and this is the refusal an
    // author meets first: a pocket as deep as the plate is the obvious way to try to make a hole.
    assert!(msg.contains("has to stop inside the material"), "{msg}");
    assert!(
        msg.contains("[PocketNotBlind]"),
        "the handle to search with: {msg}"
    );
}

/// ⑤ A footprint that never reaches the face, and a face that is not flat — two premises
/// of the operation breaking, said in words rather than named.
///
/// These are ordinary mistakes: a sketch drawn in the wrong place, and a boss asked for
/// on a cylinder's side. Both answer in words, not with the Rust variant's name alone.
#[test]
fn a_pad_says_why_its_premise_broke() {
    let prefix = vec![box_4x4x2()];
    let out = build(&prefix, None).expect("build");
    let face = top(&out, 0, |_| true);

    // A square drawn far away from the 4×4 face: the fuse comes back severed.
    let away = Step::Sketch {
        plane: PlaneRef::World(WorldPlane::XY),
        paths: vec![Path::Pen(PenPath {
            start: [90.0, 90.0],
            segs: vec![
                line_to([91.0, 90.0]),
                line_to([91.0, 91.0]),
                line_to([90.0, 91.0]),
            ],
            close_corner: None,
        })],
    };
    let msg = err_of(vec![
        box_4x4x2(),
        away,
        Step::Pad {
            face,
            sketch: ValueId(1),
            dist: 1.0,
        },
    ]);
    assert!(msg.contains("does not meet the face"), "{msg}");
    assert!(
        msg.contains("[PadMissesFace]"),
        "the handle to search with: {msg}"
    );
}

/// …and the same for a face that carries no sketch frame.
#[test]
fn a_pad_on_a_curved_face_says_what_a_face_has_to_be() {
    let prefix = vec![Step::Cylinder {
        radius: 4.0,
        height: 4.0,
        at: nacre_kit::CylAnchor::Center([0.0, 0.0, 0.0]),
        axis: nacre_kit::KitAxis::Z,
    }];
    let out = build(&prefix, None).expect("build");
    // The lateral surface: the one face of a cylinder with no normal of its own.
    let curved = out
        .faces_of(ValueId(0))
        .expect("solid faces")
        .into_iter()
        .find(|f| f.normal.is_none())
        .expect("a curved face");
    let msg = err_of(vec![
        prefix[0].clone(),
        unit_square(),
        Step::Pad {
            face: FaceRef {
                of: ValueId(0),
                face: curved.face,
            },
            sketch: ValueId(1),
            dist: 1.0,
        },
    ]);
    assert!(msg.contains("a sketch stands on a flat face"), "{msg}");
    assert!(
        msg.contains("[NonPlanarFace]"),
        "the handle to search with: {msg}"
    );
}

/// ④ Multi-island sketches are one-per-pad, honestly.
#[test]
fn one_island_per_pad() {
    let prefix = vec![box_4x4x2()];
    let out = build(&prefix, None).expect("build");
    let face = top(&out, 0, |_| true);
    let two = Step::Sketch {
        plane: PlaneRef::World(WorldPlane::XY),
        paths: vec![
            Path::Pen(PenPath {
                start: [0.0, 0.0],
                segs: vec![
                    line_to([1.0, 0.0]),
                    line_to([1.0, 1.0]),
                    line_to([0.0, 1.0]),
                ],
                close_corner: None,
            }),
            Path::Pen(PenPath {
                start: [2.0, 0.0],
                segs: vec![
                    line_to([3.0, 0.0]),
                    line_to([3.0, 1.0]),
                    line_to([2.0, 1.0]),
                ],
                close_corner: None,
            }),
        ],
    };
    let msg = err_of(vec![
        box_4x4x2(),
        two,
        Step::Pad {
            face,
            sketch: ValueId(1),
            dist: 1.0,
        },
    ]);
    assert!(msg.contains("one island per pad"), "{msg}");
    assert!(msg.contains("has 2"), "{msg}");
}

/// ⑤ On a multi-body value the pad supersedes only the face's own body; the other
/// rides along unchanged. The profile speaks the face's **plane frame** (canonical —
/// for an axis-aligned face, world coordinates in-plane), so the far body's boss is
/// drawn at the far body's coordinates.
#[test]
fn a_pad_touches_only_its_body() {
    let far_box = Step::Cuboid {
        size: [2.0, 2.0, 2.0],
        at: Anchor::Corner([10.0, 0.0, 0.0]),
    };
    let near_box = Step::Cuboid {
        size: [2.0, 2.0, 2.0],
        at: Anchor::Corner([0.0, 0.0, 0.0]),
    };
    let prefix = vec![
        near_box.clone(),
        far_box.clone(),
        Step::Boolean {
            kind: KitBool::Fuse,
            args: vec![ValueId(0), ValueId(1)],
        },
    ];
    let out = build(&prefix, None).expect("build");
    let two = solid_at(&out, 2);
    assert_eq!(two.body_count(), 2, "proven disjoint — two bodies");
    // The far body's top, by appearance (center x ≈ 11).
    let face = top(&out, 2, |f| f.center[0] > 5.0);
    let steps = vec![
        near_box,
        far_box,
        Step::Boolean {
            kind: KitBool::Fuse,
            args: vec![ValueId(0), ValueId(1)],
        },
        Step::Sketch {
            plane: PlaneRef::World(WorldPlane::XY),
            paths: vec![Path::Pen(PenPath {
                start: [10.5, 0.5],
                segs: vec![
                    line_to([11.5, 0.5]),
                    line_to([11.5, 1.5]),
                    line_to([10.5, 1.5]),
                ],
                close_corner: None,
            })],
        },
        Step::Pad {
            face,
            sketch: ValueId(3),
            dist: 1.0,
        },
    ];
    let out = build(&steps, None).expect("build");
    let val = solid_at(&out, 4);
    assert_eq!(val.body_count(), 2, "the other body rode along");
    let mut vols: Vec<f64> = val
        .bodies
        .iter()
        .map(|&b| nacre::props::mass_props(&out.model, b).unwrap().volume)
        .collect();
    vols.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!(
        (vols[0] - 8.0).abs() < 1e-9,
        "untouched body, got {}",
        vols[0]
    );
    assert!((vols[1] - 9.0).abs() < 1e-9, "padded body, got {}", vols[1]);
}

/// ⑦ A pad whose value is consumed again later: the automatic copy steps in, and the
/// positional remap keeps the reference on the right face — the boss lands where the
/// query saw the face, and the later consumer gets the unpadded shape.
#[test]
fn a_pad_survives_the_automatic_copy() {
    let prefix = vec![box_4x4x2()];
    let out = build(&prefix, None).expect("build");
    let face = top(&out, 0, |_| true);
    let steps = vec![
        box_4x4x2(),
        unit_square(),
        Step::Pad {
            face,
            sketch: ValueId(1),
            dist: 1.0,
        },
        Step::Translate {
            src: ValueId(0),
            offset: [0.0, 0.0, 10.0],
        },
    ];
    let out = build(&steps, None).expect("build");
    assert_eq!(
        out.auto_copies.len(),
        2,
        "eager: one copy per consumption (pad, then translate)"
    );
    let padded = solid_at(&out, 2);
    assert!((padded.volume(&out.model) - 33.0).abs() < 1e-9);
    let (_, hi) = nacre::props::bounds(&out.model, padded.bodies[0]).expect("bounds");
    assert!((hi.as_array()[2] - 3.0).abs() < 1e-12, "the boss is on top");
    let moved = solid_at(&out, 3);
    assert!((moved.volume(&out.model) - 32.0).abs() < 1e-9, "unpadded");
    let (lo, _) = nacre::props::bounds(&out.model, moved.bodies[0]).expect("bounds");
    assert!((lo.as_array()[2] - 10.0).abs() < 1e-12, "and translated");
}

/// ⑧ Two pads on one value both work — value semantics: each result is the original
/// shape plus its own boss. The second pad receives a copy, and the remap carries the
/// production-time face index onto it.
#[test]
fn two_pads_on_one_value() {
    let prefix = vec![box_4x4x2()];
    let out = build(&prefix, None).expect("build");
    let face = top(&out, 0, |_| true);
    let steps = vec![
        box_4x4x2(),
        unit_square(),
        Step::Pad {
            face,
            sketch: ValueId(1),
            dist: 1.0,
        },
        Step::Pad {
            face,
            sketch: ValueId(1),
            dist: 2.0,
        },
    ];
    let out = build(&steps, None).expect("build");
    assert_eq!(out.auto_copies.len(), 2, "eager: one copy per pad");
    assert!((solid_at(&out, 2).volume(&out.model) - 33.0).abs() < 1e-9);
    assert!((solid_at(&out, 3).volume(&out.model) - 34.0).abs() < 1e-9);
    for (i, want_top) in [(2usize, 3.0), (3, 4.0)] {
        let (_, hi) =
            nacre::props::bounds(&out.model, solid_at(&out, i).bodies[0]).expect("bounds");
        assert!(
            (hi.as_array()[2] - want_top).abs() < 1e-12,
            "each result carries its own boss"
        );
    }
}

/// ⑨⑩ Foreign references and degenerate depths are program errors.
#[test]
fn pad_rejections_are_honest() {
    let two_solids = vec![
        box_4x4x2(),
        Step::Cuboid {
            size: [1.0, 1.0, 1.0],
            at: Anchor::Corner([10.0, 10.0, 10.0]),
        },
    ];
    let out = build(&two_solids, None).expect("build");
    let foreign = top(&out, 1, |_| true); // a face of value 1…
    let msg = err_of(vec![
        box_4x4x2(),
        Step::Cuboid {
            size: [1.0, 1.0, 1.0],
            at: Anchor::Corner([10.0, 10.0, 10.0]),
        },
        unit_square(),
        Step::Pad {
            face: FaceRef {
                of: ValueId(0), // …claimed as value 0's
                face: foreign.face,
            },
            sketch: ValueId(2),
            dist: 1.0,
        },
    ]);
    assert!(msg.contains("not a face of value 0"), "{msg}");

    let face = top(&out, 0, |_| true);
    for bad in [0.0, -1.0, f64::NAN] {
        let msg = err_of(vec![
            box_4x4x2(),
            unit_square(),
            Step::Pad {
                face,
                sketch: ValueId(1),
                dist: bad,
            },
        ]);
        assert!(msg.contains("positive depth"), "{msg}");
    }
}
