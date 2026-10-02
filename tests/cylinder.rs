//! The cylinder primitive: the step that lets a script make a round solid, and
//! with it, a hole.
//!
//! What is worth measuring here is not "a cylinder appears" but the three things the
//! step promises: the anchor arithmetic is exact, a drill standing through a plate
//! actually removes `πr²t`, and every refusal has a sentence.

use nacre_kit::{Anchor, CylAnchor, KitAxis, KitBool, Step, ValueId, build};

fn v(i: u32) -> ValueId {
    ValueId(i)
}

fn cuboid(corner: [f64; 3], size: [f64; 3]) -> Step {
    Step::Cuboid {
        size,
        at: Anchor::Corner(corner),
    }
}

fn run(steps: Vec<Step>) -> nacre_kit::BuildOutput {
    build(&steps, None).expect("build")
}

fn vol(out: &nacre_kit::BuildOutput, i: u32) -> f64 {
    out.values[i as usize]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("a solid value")
        .volume(&out.model)
}

const PI: f64 = std::f64::consts::PI;

/// The primitive says what it is: `πr²h`, and the two anchors place the same solid.
#[test]
fn a_cylinder_is_its_own_volume_under_either_anchor() {
    let out = run(vec![
        Step::Cylinder {
            radius: 1.0,
            height: 2.0,
            at: CylAnchor::Base([0.0, 0.0, 0.0]),
            axis: KitAxis::Z,
        },
        Step::Cylinder {
            radius: 1.0,
            height: 2.0,
            at: CylAnchor::Center([0.0, 0.0, 1.0]), // the same solid, said the other way
            axis: KitAxis::Z,
        },
    ]);
    assert!((vol(&out, 0) - 2.0 * PI).abs() < 1e-9, "πr²h");
    assert!((vol(&out, 1) - 2.0 * PI).abs() < 1e-9, "πr²h");
    // Same solid, so the two must occupy the same box.
    let z = |i: u32| {
        let s = out.values[i as usize]
            .as_ref()
            .and_then(|v| v.as_solid())
            .expect("a solid");
        let b = nacre::props::bounds(&out.model, s.bodies[0]).expect("bounds");
        (b.0.as_array()[2], b.1.as_array()[2])
    };
    let (a, b) = (z(0), z(1));
    assert!(
        (a.0 - b.0).abs() < 1e-12 && (a.1 - b.1).abs() < 1e-12,
        "base [0,0,0] and centre [0,0,1] are the same cylinder: {a:?} vs {b:?}"
    );
}

/// **The computed `z` never becomes a decimal.**
///
/// A centre anchor is the one place this step computes rather than repeats: the base
/// is `z − h/2`. That value rides `Isometry::translation`, which takes rationals — it
/// is never handed through the kernel's f64 door, where only *written* decimals
/// survive intact.
///
/// The lock is **two spellings of one cylinder**: `Center(z)` and `Base(z − h/2)`
/// name the same solid, so they must land on the same coordinates *bit for bit*.
///
/// The numbers are chosen so that an f64 road would actually differ — `0.1` and
/// `0.3`: exactly, `1/10 − 3/20 = −1/20`; in f64, `0.1 - 0.3/2` is
/// `-0.04999999999999999`, **a different f64** from `-0.05`. With tidier numbers
/// (`z = 1.0, h = 0.1`) both roads land on `0.95` and this test would measure nothing.
///
/// It does *not* ask whether the two share an interned plane with a world-stated
/// one: an anchored primitive is placed by `Transform`, which **records** its
/// translation as a motion node rather than rewriting the statement, so its planes are
/// keyed `(name, motion)` and cannot collide with a plane said in the world. The
/// exactness lives in the motion, and that is what this measures.
#[test]
fn a_centre_anchors_arithmetic_stays_exact() {
    let out = run(vec![
        Step::Cylinder {
            radius: 0.5,
            height: 0.3,
            at: CylAnchor::Center([2.0, 2.0, 0.1]),
            axis: KitAxis::Z,
        },
        Step::Cylinder {
            radius: 0.5,
            height: 0.3,
            at: CylAnchor::Base([2.0, 2.0, -0.05]),
            axis: KitAxis::Z,
        },
    ]);
    let bits = |i: u32| {
        let s = out.values[i as usize]
            .as_ref()
            .and_then(|v| v.as_solid())
            .expect("a solid");
        let (lo, hi) = nacre::props::bounds(&out.model, s.bodies[0]).expect("bounds");
        [lo.as_array()[2].to_bits(), hi.as_array()[2].to_bits()]
    };
    assert_eq!(
        bits(0),
        bits(1),
        "centre and base are two spellings of one cylinder; an f64 subtraction would \
         have put the first at -0.04999999999999999 and the second at -0.05"
    );
    // And the value the exact road produces is the one a reader would write down.
    let lo = f64::from_bits(bits(0)[0]);
    assert_eq!(lo, -0.05, "the base sits where the arithmetic says: {lo:?}");
}

/// **What the app will do**: a plate, a drill through it, and a cut — in the
/// script's own vocabulary.
///
/// The drill starts *below* the plate on purpose, and that is measuring something:
/// its anchor is not `z = 0`, so the cylinder goes through an exact translation. Had
/// that translation left a motion node behind, the boolean would decline the moved
/// cylinder by name (`cylinder_gate_undecided`) and **every kit cylinder not sitting
/// at the origin would be uncuttable**. It overshoots both faces the way a "through all"
/// hole is drawn; stopping flush on them works too (`the_refusals_say_what_is_wrong`).
#[test]
fn a_drill_through_a_plate_removes_its_own_bore() {
    let out = run(vec![
        cuboid([0.0, 0.0, 0.0], [4.0, 4.0, 2.0]),
        Step::Cylinder {
            radius: 0.5,
            height: 3.0,
            at: CylAnchor::Base([2.0, 2.0, -0.5]),
            axis: KitAxis::Z,
        },
        Step::Boolean {
            kind: KitBool::Cut,
            args: vec![v(0), v(1)],
        },
    ]);
    let want = 4.0 * 4.0 * 2.0 - PI * 0.25 * 2.0;
    assert!(
        (vol(&out, 2) - want).abs() < 1e-9,
        "a drilled plate is the plate minus the bore: {} vs {want}",
        vol(&out, 2)
    );
    assert!(
        nacre::validate::validate(&out.model).is_empty(),
        "{:?}",
        nacre::validate::validate(&out.model)
    );
    // The drill was consumed, so the screen shows the drilled plate alone.
    assert_eq!(out.rendered, vec![v(2)]);
}

/// A cylinder is a value like any other: reusable, and the reuse is invisible.
#[test]
fn a_cylinder_value_is_freely_reusable() {
    let out = run(vec![
        cuboid([0.0, 0.0, 0.0], [4.0, 4.0, 2.0]),
        Step::Cylinder {
            radius: 0.5,
            height: 3.0,
            at: CylAnchor::Base([1.0, 1.0, -0.5]),
            axis: KitAxis::Z,
        },
        Step::Boolean {
            kind: KitBool::Cut,
            args: vec![v(0), v(1)],
        },
        Step::Translate {
            src: v(1), // the drill again — the reuse
            offset: [2.0, 2.0, 0.0],
        },
    ]);
    assert!(
        (vol(&out, 3) - PI * 0.25 * 3.0).abs() < 1e-9,
        "the reused drill is intact"
    );
    assert!(
        !out.auto_copies.is_empty(),
        "a copy was inserted, invisibly"
    );
}

/// **Every refusal is a sentence.** The one a user meets by accident is the boss that
/// hangs over the edge it stands on — and the sentence is the only place they learn what
/// about it the kernel cannot do.
///
/// Its neighbour, a drill standing *exactly* on the plate's face, builds, and the
/// assertion below is that it does: a refusal there would also answer for this one, telling a
/// user who overhung a boss that their cap was flush. The sentence and the success are the
/// same fact seen from two sides.
#[test]
fn the_refusals_say_what_is_wrong() {
    let flush = build(
        &[
            cuboid([0.0, 0.0, 0.0], [4.0, 4.0, 2.0]),
            Step::Cylinder {
                radius: 0.5,
                height: 2.0,
                at: CylAnchor::Base([2.0, 2.0, 0.0]), // both caps flush with the plate
                axis: KitAxis::Z,
            },
            Step::Boolean {
                kind: KitBool::Cut,
                args: vec![v(0), v(1)],
            },
        ],
        None,
    );
    if let Err(e) = flush {
        panic!("a flush drill bores the plate: {e}");
    }

    // **The boss over the edge builds**: the rim's crossings become branch vertices, the
    // circle splits into arcs, and the band assembles. The witness pipe keeps its coverage on
    // the refusals that remain (the app's 45° fold and pinch fixtures assert the mark end to end).
    let overhang = build(
        &[
            cuboid([0.0, 0.0, 0.0], [4.0, 4.0, 2.0]),
            Step::Cylinder {
                radius: 0.5,
                height: 1.0,
                at: CylAnchor::Base([4.0, 2.0, 2.0]), // half of it hangs past x = 4
                axis: KitAxis::Z,
            },
            Step::Boolean {
                kind: KitBool::Fuse,
                args: vec![v(0), v(1)],
            },
        ],
        None,
    );
    if let Err(e) = overhang {
        panic!("a boss over the edge builds: {e}");
    }

    for (r, h) in [(0.0, 1.0), (-1.0, 1.0), (1.0, 0.0)] {
        let e = build(
            &[Step::Cylinder {
                radius: r,
                height: h,
                at: CylAnchor::Base([0.0, 0.0, 0.0]),
                axis: KitAxis::Z,
            }],
            None,
        );
        let e = match e {
            Err(e) => e,
            Ok(_) => panic!("r={r} h={h} is a program error, not a solid"),
        };
        assert!(
            e.to_string().contains("must be positive"),
            "r={r} h={h}: {e}"
        );
    }

    // The decimal window is the kit's own refusal, not the kernel's: `dec` names it
    // with the value in hand, so the kernel's own window errors are the
    // kernel API's net and cannot be reached through this door.
    let wide = build(
        &[Step::Cylinder {
            radius: 1e300,
            height: 1.0,
            at: CylAnchor::Base([0.0, 0.0, 0.0]),
            axis: KitAxis::Z,
        }],
        None,
    );
    let wide = match wide {
        Err(e) => e,
        Ok(_) => panic!("1e300 has no exact decimal"),
    };
    assert!(wide.to_string().contains("decimal window"), "{wide}");
}

/// **The axis names the plane, and the anchor follows it.**
///
/// A cylinder stands along any world axis, so a crosswise drill needs no hand-written YZ/ZX
/// sketch. The axis is sugar the kit may spend — `SketchFrame::world` already
/// takes one — but the *frames run cyclically*: `Z` sketches on XY with `(u, v) = (x, y)`, `X`
/// on YZ with `(y, z)`, `Y` on ZX with `(z, x)`. Get that shift backwards and every non-`Z`
/// cylinder lands somewhere plausible and wrong.
///
/// So the oracle is written from the shape, not from the frame: whatever the axis, the solid
/// spans `h` along it and `2r` across the other two.
#[test]
fn a_cylinder_stands_along_the_axis_it_names() {
    for (axis, w) in [(KitAxis::X, 0usize), (KitAxis::Y, 1), (KitAxis::Z, 2)] {
        let out = run(vec![Step::Cylinder {
            radius: 2.0,
            height: 10.0,
            at: CylAnchor::Base([0.0, 0.0, 0.0]),
            axis,
        }]);
        let s = out.values[0]
            .as_ref()
            .expect("value")
            .as_solid()
            .expect("a solid");
        let (lo, hi) = nacre::props::bounds(&out.model, s.bodies[0]).expect("bounds");
        let (lo, hi) = (lo.as_array(), hi.as_array());
        for k in 0..3 {
            let (want_lo, want_hi) = if k == w { (0.0, 10.0) } else { (-2.0, 2.0) };
            assert!(
                (lo[k] - want_lo).abs() < 1e-9 && (hi[k] - want_hi).abs() < 1e-9,
                "{axis:?} axis, coordinate {k}: [{}, {}] against [{want_lo}, {want_hi}]",
                lo[k],
                hi[k]
            );
        }
    }
}

/// **And the anchor is «along the axis», not «in z»** — measured on `X`, because `Z` would
/// pass with the generalisation only half done.
///
/// `Base` puts the low cap's centre at the point; `Center` puts the solid's middle there. Both
/// are read from the bounds rather than from the builder's own arithmetic.
#[test]
fn the_anchor_runs_along_the_axis_too() {
    let at_x = |at: CylAnchor| {
        let out = run(vec![Step::Cylinder {
            radius: 1.0,
            height: 6.0,
            at,
            axis: KitAxis::X,
        }]);
        let s = out.values[0]
            .as_ref()
            .expect("value")
            .as_solid()
            .expect("a solid");
        let (lo, hi) = nacre::props::bounds(&out.model, s.bodies[0]).expect("bounds");
        (lo.as_array(), hi.as_array())
    };
    // Base at (1, 2, 3): the low cap sits on x = 1, and the circle is centred on (y, z) = (2, 3).
    let (lo, hi) = at_x(CylAnchor::Base([1.0, 2.0, 3.0]));
    assert!(
        (lo[0] - 1.0).abs() < 1e-9 && (hi[0] - 7.0).abs() < 1e-9,
        "{lo:?} {hi:?}"
    );
    assert!(
        (lo[1] - 1.0).abs() < 1e-9 && (hi[1] - 3.0).abs() < 1e-9,
        "{lo:?} {hi:?}"
    );
    assert!(
        (lo[2] - 2.0).abs() < 1e-9 && (hi[2] - 4.0).abs() < 1e-9,
        "{lo:?} {hi:?}"
    );
    // Center at the same point: the solid straddles x = 1.
    let (lo, hi) = at_x(CylAnchor::Center([1.0, 2.0, 3.0]));
    assert!(
        (lo[0] + 2.0).abs() < 1e-9 && (hi[0] - 4.0).abs() < 1e-9,
        "{lo:?} {hi:?}"
    );
}
