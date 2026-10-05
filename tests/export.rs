//! Files out of a build: what is shown is what is written, the export door raises what a long
//! history left behind, and a file is a function of its steps and its time stamp.

use nacre_kit::{Anchor, KitAxis, KitBool, Pivot, Step, ValueId, build};

const STAMP: &str = "2026-10-05T00:00:00Z";

fn v(i: u32) -> ValueId {
    ValueId(i)
}

fn cuboid(corner: [f64; 3], size: [f64; 3]) -> Step {
    Step::Cuboid {
        size,
        at: Anchor::Corner(corner),
    }
}

/// How many solids a STEP text holds, read back by step-io's reader — not by counting entity
/// names: a hollow body is a `BREP_WITH_VOIDS`, a subtype of `MANIFOLD_SOLID_BREP`, and a text
/// count could take one body for two.
fn solids_in(text: &str) -> usize {
    let (model, report) = step_io::read(text.as_bytes()).expect("the file reads back");
    assert!(report.dropped.is_empty(), "drops: {:?}", report.dropped);
    model.scene().all_solids().count()
}

/// ★★ **The file holds the bodies the build shows, and only those.** The model keeps more live
/// solids than it draws — here a box the display set leaves out, and a hollow box (one body with
/// a cavity) that is shown — so writing the live set would write a solid nobody sees.
#[test]
fn the_step_file_holds_the_shown_bodies_only() {
    let steps = vec![
        cuboid([0.0; 3], [4.0; 3]),
        cuboid([1.0; 3], [2.0; 3]),
        Step::Boolean {
            kind: KitBool::Cut,
            args: vec![v(0), v(1)],
        },
        cuboid([10.0, 0.0, 0.0], [1.0; 3]),
        Step::Display {
            targets: vec![v(2)],
            style: None,
        },
    ];
    let mut out = build(&steps, None).expect("build");
    let shown = out.rendered_bodies();
    assert_eq!(shown.len(), 1, "the hollow box is one body");
    let live = out.model.live_solids().to_vec();
    assert!(
        live.len() > shown.len(),
        "the premise: the model holds a solid the build does not show ({live:?})"
    );
    assert!(
        shown.iter().all(|h| live.contains(h)),
        "a shown body is not live: {shown:?} of {live:?}"
    );

    let file = out.export_step(STAMP).expect("export");
    assert!(
        file.text.contains("BREP_WITH_VOIDS"),
        "the cavity is written"
    );
    assert_eq!(solids_in(&file.text), shown.len());
}

/// ★★ **The export door raises what a long history left at its construction figure.** A box turned
/// 7° and carried 300 recorded translations is past what the caches pay for at build time (the
/// kernel's `translated_chain(300)`, said in steps), so its caches stand at the producer's
/// figures; the door raises the shown box's, and leaves nothing behind.
#[test]
fn the_export_door_raises_a_long_history() {
    let mut steps = vec![
        cuboid([0.0; 3], [2.0, 3.0, 4.0]),
        Step::Rotate {
            src: v(0),
            axis: KitAxis::Z,
            deg: 7.0,
            pivot: Pivot::Origin,
        },
    ];
    for i in 1..=300u32 {
        steps.push(Step::Translate {
            src: v(i),
            offset: [0.125, 0.0, 0.0],
        });
    }
    let mut out = build(&steps, None).expect("build");
    let file = out.export_step(STAMP).expect("export");
    let r = file.refine;
    let raised = r.vertices.refined + r.surfaces.refined + r.edges.refined;
    let left = r.vertices.left_undecided
        + r.vertices.left_unrealized
        + r.surfaces.left_undecided
        + r.surfaces.left_unrealized
        + r.edges.left_undecided
        + r.edges.left_unrealized;
    assert!(raised > 0, "the door raised nothing: {r:?}");
    // Every intermediate is live (301 hidden boxes): the door pays for the shown box's 8 corners
    // and no other's.
    assert_eq!(r.vertices.refined, 8, "{r:?}");
    assert_eq!(left, 0, "{r:?}");
    assert_eq!(solids_in(&file.text), 1);
}

/// ★ **Same steps, same stamp, same bytes** — from two separate builds, so the agreement is the
/// build's and the door's determinism rather than one model answering twice. The stamp is the
/// caller's and reaches the header verbatim.
#[test]
fn the_same_steps_export_the_same_bytes() {
    let steps = vec![
        cuboid([0.0; 3], [3.0, 2.0, 1.0]),
        Step::Rotate {
            src: v(0),
            axis: KitAxis::Z,
            deg: 30.0,
            pivot: Pivot::Origin,
        },
    ];
    let a = build(&steps, None)
        .expect("build")
        .export_step(STAMP)
        .expect("export");
    let b = build(&steps, None)
        .expect("build")
        .export_step(STAMP)
        .expect("export");
    assert_eq!(a.text, b.text);
    assert!(a.text.contains(&format!("'{STAMP}'")));
}
