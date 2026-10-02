//! The render set (a script-layer notion, not the kernel live set) and the
//! report surface.

use nacre_kit::{Anchor, KitBool, Step, ValueId, build};

fn v(i: u32) -> ValueId {
    ValueId(i)
}

fn cuboid(corner: [f64; 3], size: [f64; 3]) -> Step {
    Step::Cuboid {
        size,
        at: Anchor::Corner(corner),
    }
}

/// Default rendering: DAG leaves only — consumed ingredients disappear, and a value
/// that is merely *read* (copied) is an ingredient too.
#[test]
fn only_dag_leaves_render_by_default() {
    let out = build(
        &[
            cuboid([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]), // 0: consumed by 2
            cuboid([1.0, 1.0, -1.0], [1.0, 1.0, 4.0]), // 1: consumed by 2
            Step::Boolean {
                kind: KitBool::Cut,
                args: vec![v(0), v(1)],
            }, // 2: leaf
            cuboid([10.0, 0.0, 0.0], [1.0, 1.0, 1.0]), // 3: leaf
        ],
        None,
    )
    .expect("build");
    assert_eq!(out.rendered, vec![v(2), v(3)], "ingredients are not drawn");
}

/// An explicit display turns the default off entirely — what is displayed is the whole
/// set, even if other leaves exist.
#[test]
fn display_overrides_the_default() {
    let out = build(
        &[
            cuboid([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]),  // 0
            cuboid([10.0, 0.0, 0.0], [1.0, 1.0, 1.0]), // 1: a leaf — but not displayed
            Step::Display {
                targets: vec![v(0)],
                style: None,
            },
        ],
        None,
    )
    .expect("build");
    assert_eq!(
        out.rendered,
        vec![v(0)],
        "explicit display is the whole set"
    );
}

/// What `copy` is *for*: reuse copies by itself, so writing `.copy()` says one thing
/// only — "I want a second one" — and the original stays on screen beside it. The
/// screen shows what no later step **consumed**, and a copy consumes nothing.
#[test]
fn a_copy_keeps_the_original_on_screen() {
    let steps = vec![
        cuboid([0.0, 0.0, 0.0], [2.0, 2.0, 1.0]), // 0: base
        Step::Copy { src: v(0) },                 // 1: lid-to-be
        Step::Translate {
            src: v(1),
            offset: [0.0, 0.0, 5.0],
        }, // 2: lid
    ];
    let out = build(&steps, None).expect("build");
    assert_eq!(
        out.rendered,
        vec![v(0), v(2)],
        "the base and its moved copy"
    );
    // Display remains the explicit control above the default.
    let mut with_display = steps.clone();
    with_display.push(Step::Display {
        targets: vec![v(2)],
        style: None,
    });
    let out = build(&with_display, None).expect("build");
    assert_eq!(out.rendered, vec![v(2)]);
}

/// The negative control, and the reason the rule is "consumed" rather than "live":
/// transforming a value hands the screen to what was made, so a chain leaves no ghost.
#[test]
fn a_transform_takes_the_screen_from_its_source() {
    let steps = vec![
        cuboid([0.0, 0.0, 0.0], [2.0, 2.0, 1.0]), // 0
        Step::Translate {
            src: v(0),
            offset: [5.0, 0.0, 0.0],
        }, // 1
        Step::Rotate {
            src: v(1),
            axis: nacre_kit::KitAxis::Z,
            deg: 30.0,
            pivot: nacre_kit::Pivot::Origin,
        }, // 2
    ];
    let out = build(&steps, None).expect("build");
    assert_eq!(out.rendered, vec![v(2)], "only the end of the chain");
}

/// The trap this change had to avoid: the render set stopped asking `reads()`, but the
/// backward-reference check still does — a copy of a value that does not exist is
/// still a program error, not a silent nothing.
#[test]
fn a_copy_of_nothing_is_still_caught() {
    let steps = vec![
        cuboid([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
        Step::Copy { src: v(7) },
    ];
    assert!(matches!(
        build(&steps, None),
        Err(nacre_kit::KitError::Program { step: 1, .. })
    ));
}

/// The report surface: boolean steps carry a step-level aggregate; on an axis-aligned
/// model **nothing is assumed** — zero merges, zero coincidences, no closest call.
/// (That is the negative direction only: a fixture whose report is *nonempty* needs
/// rotated coplanar contact, which K1's vocabulary cannot state yet — an honest gap
/// for K2/K3, not a covered case.)
#[test]
fn axis_aligned_booleans_assume_nothing() {
    let out = build(
        &[
            cuboid([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]),
            cuboid([1.0, 0.0, 0.0], [2.0, 2.0, 2.0]),
            Step::Boolean {
                kind: KitBool::Fuse,
                args: vec![v(0), v(1)],
            },
        ],
        None,
    )
    .expect("build");
    assert!(out.reports[0].is_none(), "a cuboid step runs no boolean");
    let r = out.reports[2].as_ref().expect("a boolean step reports");
    assert_eq!(r.merges, 0, "nothing was assumed");
    assert_eq!(r.coincidences, 0);
    assert!(r.closest_calls.is_empty());
}

/// `upto` builds a prefix: the same program, cut earlier, renders the state as of that
/// step — the inspector's question.
#[test]
fn upto_builds_a_prefix() {
    let steps = vec![
        cuboid([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]),
        cuboid([1.0, 0.0, 0.0], [2.0, 2.0, 2.0]),
        Step::Boolean {
            kind: KitBool::Cut,
            args: vec![v(0), v(1)],
        },
    ];
    let out = build(&steps, Some(2)).expect("build");
    assert_eq!(out.values.len(), 2);
    assert_eq!(
        out.rendered,
        vec![v(0), v(1)],
        "before the cut, both boxes are leaves"
    );
    // And the prefix does not copy for consumptions that lie beyond the cutoff.
    assert!(out.auto_copies.is_empty());
}
