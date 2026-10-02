//! **What an author learns from a rejection, through the kit** — the kernel's classification
//! (`RejectClass`) rides the error value, and the display speaks facts, never advice.
//!
//! These are consumer tests in the same sense as the kernel's own `coverage/rejects.rs`: raise a
//! rejection through the public road a script takes, then read what came back. The three cases
//! are the three shapes of `class`: a capability limit, an impossibility, and an
//! operation-level error that carries no classification yet.

use nacre_kit::{
    Anchor, Dist, KitBool, KitError, Path, PenPath, Pivot, PlaneRef, RejectClass, SketchSeg, Step,
    ValueId, WorldPlane, build,
};

fn v(i: u32) -> ValueId {
    ValueId(i)
}

fn cuboid(corner: [f64; 3], size: [f64; 3]) -> Step {
    Step::Cuboid {
        size,
        at: Anchor::Corner(corner),
    }
}

fn fuse(args: Vec<ValueId>) -> Step {
    Step::Boolean {
        kind: KitBool::Fuse,
        args,
    }
}

fn pen(start: [f64; 2], pts: &[[f64; 2]]) -> PenPath {
    PenPath {
        start,
        segs: pts
            .iter()
            .map(|&to| SketchSeg::LineTo { to, corner: None })
            .collect(),
        close_corner: None,
    }
}

/// The kernel's classification arrives with the error, and the message states the class as a
/// fact plus the stable identifier. The 45° fold is a self-touch (`Impossible`), with the
/// touching edge itself as the mark. No kit-reachable end-to-end `NotSupported` input exists
/// (the class pipe itself is class-agnostic, and the sentence table below covers the class
/// wordings without building models).
#[test]
fn the_folds_self_touch_arrives_classified() {
    let plate = pen(
        [0.0, 0.0],
        &[
            [50.0, 0.0],
            [50.0, 25.0],
            [38.0, 25.0],
            [38.0, 50.0],
            [50.0, 50.0],
            [50.0, 75.0],
            [0.0, 75.0],
        ],
    );
    let bar = pen([20.0, 12.0], &[[75.0, 12.0], [75.0, 37.0], [55.0, 37.0]]);
    let mut steps = vec![
        Step::Sketch {
            plane: PlaneRef::World(WorldPlane::XY),
            paths: vec![Path::Pen(plate)],
        }, // 0
        Step::Extrude {
            sketch: v(0),
            dist: Dist::One(12.0),
        }, // 1
        Step::Sketch {
            plane: PlaneRef::World(WorldPlane::YZ),
            paths: vec![Path::Pen(bar)],
        }, // 2
        Step::Extrude {
            sketch: v(2),
            dist: Dist::One(25.0),
        }, // 3
        fuse(vec![v(1), v(3)]), // 4: the unit
    ];
    let mut part = 4u32;
    let mut next = 5u32;
    for deg in (45..360).step_by(45) {
        steps.push(Step::Rotate {
            src: v(4),
            axis: nacre_kit::KitAxis::Z,
            deg: deg as f64,
            pivot: Pivot::Origin,
        }); // next
        steps.push(fuse(vec![v(part), v(next)])); // next + 1
        part = next + 1;
        next += 2;
    }

    let err = match build(&steps, None) {
        Err(e) => e,
        Ok(_) => panic!("the 45° fold declines"),
    };
    let KitError::Kernel {
        what,
        class,
        blame,
        mark,
        ..
    } = &err
    else {
        panic!("a kernel rejection, got {err:?}");
    };
    assert_eq!(*class, Some(RejectClass::Impossible));
    assert_eq!(
        what, "self_touching_result",
        "the stable identifier travels"
    );
    assert!(blame.is_some(), "a fold failure blames its pair");
    // The kernel's witness location survives the hop. The geometric proposition (the touch
    // segment spans the fold's caps, vertically) is the kernel's own lock
    // (`rotation_sweep.rs`); here the claim is that the pipe carries it.
    let Some(nacre_kit::Mark::Segment([a, b])) = mark else {
        panic!("the touch mark did not survive the kit hop: {mark:?}");
    };
    assert!(
        a.iter().chain(b.iter()).all(|c| c.is_finite()),
        "the touch mark is not a finite segment: {a:?} {b:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains(
            "no valid solid exists for this input — the result's surface touches itself, \
             leaving material of no thickness [self_touching_result]"
        ),
        "{msg}"
    );
}

/// `Impossible` via the pinched fuse (the kernel's own `rejects.rs` fixture, spoken in steps):
/// A and B meet only at one vertex while two bridges join them around it, so no 2-manifold
/// contains the union. The display states the fact — no advice.
#[test]
fn an_impossibility_arrives_classified() {
    let steps = vec![
        cuboid([0.0, 0.0, 0.0], [2.0, 2.0, 1.0]), // 0: a
        cuboid([1.0, 0.3, 0.2], [3.5, 1.0, 0.6]), // 1: g1 (corner + size = kernel's 4.5,1.3,0.8)
        cuboid([3.5, 0.3, 0.2], [1.0, 3.5, 1.4]), // 2: g2 (→ 4.5, 3.8, 1.6)
        cuboid([2.0, 2.0, 1.0], [2.0, 2.0, 1.0]), // 3: b
        fuse(vec![v(0), v(1), v(2), v(3)]),       // 4: ((a∪g1)∪g2)∪b — the last pair pinches
    ];
    let err = match build(&steps, None) {
        Err(e) => e,
        Ok(_) => panic!("the pinched fuse declines"),
    };
    let KitError::Kernel {
        what, class, mark, ..
    } = &err
    else {
        panic!("a kernel rejection, got {err:?}");
    };
    assert_eq!(*class, Some(RejectClass::Impossible));
    assert_eq!(what, "non_manifold_vertex");
    // The witness is the pinch corner itself, `(2, 2, 1)` — where cuboid 0 and cuboid 3 meet,
    // read off the steps above. Approximate: the coordinates are a diagnostic realization.
    let Some(nacre_kit::Mark::Point(p)) = mark else {
        panic!("the pinch mark did not survive the kit hop: {mark:?}");
    };
    let d2 = (p[0] - 2.0).powi(2) + (p[1] - 2.0).powi(2) + (p[2] - 1.0).powi(2);
    assert!(d2.sqrt() < 1e-9, "mark off the pinch corner: {p:?}");
    let msg = err.to_string();
    assert!(
        msg.contains(
            "no valid solid exists for this input — the result pinches at a single vertex \
             [non_manifold_vertex]"
        ),
        "{msg}"
    );
}

/// An operation-level error carries no classification yet — `class` is `None` and the message
/// is the error's own words, unchanged. (Their taxonomy is a later step; the field stating
/// "none" is the honest form of that.)
#[test]
fn an_operation_error_carries_no_class() {
    // A pad aimed at a cylinder's side: `NonPlanarFace`, an `OpError`, not a boolean rejection.
    let base = Step::Cylinder {
        radius: 4.0,
        height: 4.0,
        at: nacre_kit::CylAnchor::Center([0.0, 0.0, 0.0]),
        axis: nacre_kit::KitAxis::Z,
    };
    let square = Step::Sketch {
        plane: PlaneRef::World(WorldPlane::XY),
        paths: vec![Path::Pen(pen(
            [0.5, 0.5],
            &[[1.5, 0.5], [1.5, 1.5], [0.5, 1.5]],
        ))],
    };
    let out = build(std::slice::from_ref(&base), None).expect("the base builds");
    let curved = out
        .faces_of(v(0))
        .expect("solid faces")
        .into_iter()
        .find(|f| f.normal.is_none())
        .expect("the lateral face");
    let steps = vec![
        base,
        square,
        Step::Pad {
            face: nacre_kit::FaceRef {
                of: v(0),
                face: curved.face,
            },
            sketch: v(1),
            dist: 1.0,
        },
    ];
    let err = match build(&steps, None) {
        Err(e) => e,
        Ok(_) => panic!("a pad on a curved face declines"),
    };
    let KitError::Kernel { class, what, .. } = &err else {
        panic!("a kernel rejection, got {err:?}");
    };
    assert_eq!(*class, None, "OpError carries no classification yet");
    assert!(what.contains("a sketch stands on a flat face"), "{what}");
    assert!(
        what.contains("[NonPlanarFace]"),
        "the handle to search with: {what}"
    );
}

/// **Every shipped sentence, pinned** — table-driven over all twelve rows of the sentence table,
/// plus the fallback for an unknown identifier and for a reason the table leaves out on purpose
/// (`cylinder_stages_disagree`, a defect: the class sentence is the fact). Constructed directly (the fields are public) so every row is pinned
/// even though only two have raise fixtures above; the kernel-side rule covers the rest (a reason
/// rename turns the census gate red, and that commit revisits this table).
///
/// Each is built at `step: 3` and **none of them says so**: the step rides the error value
/// ([`KitError::step`]), the way the classification does. The test below states that rule for
/// every variant.
#[test]
fn every_reason_sentence_is_pinned() {
    let render = |what: &str, class: RejectClass| {
        KitError::Kernel {
            step: 3,
            what: what.to_string(),
            class: Some(class),
            blame: None,
            mark: None,
        }
        .to_string()
    };
    let cases = [
        (
            "self_touching_result",
            RejectClass::Impossible,
            "no valid solid exists for this input — the result's surface touches itself, leaving material of no thickness [self_touching_result]",
        ),
        (
            "non_manifold_vertex",
            RejectClass::Impossible,
            "no valid solid exists for this input — the result pinches at a single vertex [non_manifold_vertex]",
        ),
        (
            "non_manifold_result_edge",
            RejectClass::Impossible,
            "no valid solid exists for this input — the result pinches along an edge — more than two faces share it [non_manifold_result_edge]",
        ),
        (
            "oblique_cylinder_cut",
            RejectClass::NotSupported,
            "the kernel does not build this — a flat face meets the cylinder at a slant — the elliptical crossing is not built yet [oblique_cylinder_cut]",
        ),
        (
            "cylinder_pair_contact",
            RejectClass::NotSupported,
            "the kernel does not build this — two cylinders touch or overlap — cylinder-with-cylinder booleans are not built yet [cylinder_pair_contact]",
        ),
        (
            "cylinder_gate_undecided",
            RejectClass::NotSupported,
            "the kernel does not build this — the cylinder's placement could not be checked exactly against the other body [cylinder_gate_undecided]",
        ),
        (
            "ruling_bound_not_yet",
            RejectClass::NotSupported,
            "the kernel does not build this — the cylinder's side is divided along its length in a way that is not built yet [ruling_bound_not_yet]",
        ),
        (
            "arc_bound_not_yet",
            RejectClass::NotSupported,
            "the kernel does not build this — a boundary on the cylinder's side — around its seam or along a rim — is not built yet [arc_bound_not_yet]",
        ),
        (
            "no_clear_ray",
            RejectClass::NotSupported,
            "the kernel does not build this — inside-or-outside could not be decided — every probe ray from the region's corners grazes a boundary [no_clear_ray]",
        ),
        (
            "witness_not_rational",
            RejectClass::NotSupported,
            "the kernel does not build this — an exact value this step needed does not fit the kernel's exact numbers — the shapes' coordinates or planes carry too many digits to combine exactly [witness_not_rational]",
        ),
        (
            "vertex_names_absent_surface",
            RejectClass::NotSupported,
            "the kernel does not build this — a corner of the result is defined by a surface the result keeps no face on — the corner cannot be re-derived from the solid itself [vertex_names_absent_surface]",
        ),
        (
            "precision_budget",
            RejectClass::NotSupported,
            "the kernel does not build this — the motion history needs more precision than the judging budget holds [precision_budget]",
        ),
    ];
    for (what, class, want) in cases {
        assert_eq!(render(what, class), want);
    }
    // The fallback: an identifier this table does not know renders verdict + identifier — when
    // the kernel grows or renames a reason, the display gets less rich, never wrong.
    assert_eq!(
        render("some_future_reason", RejectClass::NotSupported),
        "the kernel does not build this [some_future_reason]"
    );
    assert_eq!(
        render("cylinder_stages_disagree", RejectClass::SuspectedDefect),
        "an engine invariant broke [cylinder_stages_disagree]"
    );
}

/// Does a sentence open with a `step N: ` prefix?
fn opens_with_a_step(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("step ") else {
        return false;
    };
    let Some((n, _)) = rest.split_once(": ") else {
        return false;
    };
    !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())
}

/// **The step rides the value, not the sentence** — for every variant.
///
/// The index is a field on all three, so a consumer reads it there; writing it into the
/// sentence as well would make every consumer state one fact twice. The app is the
/// measured case: it says `Where line 2 — extrude` in its own words, and a step prefix in the
/// sentence would print `Message step 1: …` beside it.
///
/// Each case asserts three things at once. "Does not open with a step" passes for an
/// empty sentence too, so the words the error owes are pinned in the same breath.
#[test]
fn the_step_rides_the_value_not_the_sentence() {
    let kernel_classed = KitError::Kernel {
        step: 3,
        what: "no_clear_ray".to_string(),
        class: Some(RejectClass::NotSupported),
        blame: None,
        mark: None,
    };
    let kernel_plain = KitError::Kernel {
        step: 5,
        what: "a sketch stands on a flat face [NonPlanarFace]".to_string(),
        class: None,
        blame: None,
        mark: None,
    };
    let program = KitError::Program {
        step: 0,
        what: "cuboid size must be positive".to_string(),
    };
    let internal = KitError::Internal {
        step: 7,
        what: "a live solid was consumed twice".to_string(),
    };

    for (err, at, owed) in [
        (&kernel_classed, 3, "does not build this"),
        (&kernel_plain, 5, "NonPlanarFace"),
        (&program, 0, "must be positive"),
        (&internal, 7, "invariant broke"),
    ] {
        let said = err.to_string();
        assert_eq!(err.step(), at, "the value knows which step: {said}");
        assert!(said.contains(owed), "it still says its own words: {said}");
        assert!(
            !opens_with_a_step(&said),
            "and does not repeat the step: {said}"
        );
    }

    // The helper is not vacuous: it does recognise the shape it forbids.
    assert!(opens_with_a_step("step 12: something"));
    assert!(!opens_with_a_step("stepwise: something"));
}
