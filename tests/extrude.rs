//! Extrude on the three world planes, both directions, through the sketch value.
//! The kernel's `dist` is a positive thickness whose direction belongs to the frame; a
//! negative script `dist` is translated by re-stating the same plane with the opposite
//! normal (same handle, opposite frame — the kernel's own rule for choosing a side).
//!
//! The direction tests assert the **full bounds**, not just the normal axis — the
//! first version checked only the normal extent and let a mirrored in-plane footprint
//! ship (caught by the shared-plane fuse below going non-manifold on edge contact).

#[allow(unused_imports)]
use nacre_kit::KitError;
use nacre_kit::{KitBool, Path, PenPath, PlaneRef, SketchSeg, Step, ValueId, WorldPlane, build};

fn line_to(to: [f64; 2]) -> SketchSeg {
    SketchSeg::LineTo { to, corner: None }
}

fn tri_sketch(plane: WorldPlane) -> Step {
    Step::Sketch {
        plane: PlaneRef::World(plane),
        paths: vec![Path::Pen(PenPath {
            start: [0.0, 0.0],
            segs: vec![line_to([4.0, 0.0]), line_to([0.0, 3.0])],
            close_corner: None,
        })],
    }
}

fn bounds_of(plane: WorldPlane, dist: f64) -> ([f64; 3], [f64; 3]) {
    let steps = vec![
        tri_sketch(plane),
        Step::Extrude {
            sketch: ValueId(0),
            dist: nacre_kit::Dist::One(dist),
        },
    ];
    let out = build(&steps, None).expect("build");
    let val = out.values[1]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("a solid");
    assert!(
        (val.volume(&out.model) - 12.0).abs() < 1e-9,
        "a 4×3/2 triangle, 2 deep"
    );
    let (lo, hi) = nacre::props::bounds(&out.model, val.bodies[0]).expect("bounds");
    (lo.as_array(), hi.as_array())
}

fn assert_close(got: [f64; 3], want: [f64; 3], what: &str) {
    for k in 0..3 {
        assert!(
            (got[k] - want[k]).abs() < 1e-12,
            "{what}: axis {k} — got {got:?}, want {want:?}"
        );
    }
}

/// +dist grows along the +normal, with the caller's footprint on the named axes
/// (XY = (+X, +Y), YZ = (+Y, +Z), ZX = (+Z, +X)).
#[test]
fn each_world_plane_extrudes_along_its_normal() {
    let (lo, hi) = bounds_of(WorldPlane::XY, 2.0);
    assert_close(lo, [0.0, 0.0, 0.0], "XY lo");
    assert_close(hi, [4.0, 3.0, 2.0], "XY hi");

    let (lo, hi) = bounds_of(WorldPlane::YZ, 2.0);
    assert_close(lo, [0.0, 0.0, 0.0], "YZ lo");
    assert_close(hi, [2.0, 4.0, 3.0], "YZ hi");

    let (lo, hi) = bounds_of(WorldPlane::ZX, 2.0);
    assert_close(lo, [0.0, 0.0, 0.0], "ZX lo");
    assert_close(hi, [3.0, 2.0, 4.0], "ZX hi");
}

/// −dist grows along the −normal and keeps the same footprint — the whole point of
/// the axis-pinned flipped statement.
#[test]
fn a_negative_dist_extrudes_the_other_way() {
    let (lo, hi) = bounds_of(WorldPlane::XY, -2.0);
    assert_close(lo, [0.0, 0.0, -2.0], "XY lo");
    assert_close(hi, [4.0, 3.0, 0.0], "XY hi");

    let (lo, hi) = bounds_of(WorldPlane::YZ, -2.0);
    assert_close(lo, [-2.0, 0.0, 0.0], "YZ lo");
    assert_close(hi, [0.0, 4.0, 3.0], "YZ hi");

    let (lo, hi) = bounds_of(WorldPlane::ZX, -2.0);
    assert_close(lo, [0.0, -2.0, 0.0], "ZX lo");
    assert_close(hi, [3.0, 0.0, 4.0], "ZX hi");
}

/// One sketch, two directions, fused: the halves merge across the shared plane (one
/// handle, not a second plane an ulp away) — and the sketch is reused without a single
/// copy, because a sketch is pure data.
#[test]
fn both_directions_share_the_plane() {
    let steps = vec![
        Step::Sketch {
            plane: PlaneRef::World(WorldPlane::XY),
            paths: vec![Path::Pen(PenPath {
                start: [0.0, 0.0],
                segs: vec![
                    line_to([2.0, 0.0]),
                    line_to([2.0, 2.0]),
                    line_to([0.0, 2.0]),
                ],
                close_corner: None,
            })],
        },
        Step::Extrude {
            sketch: ValueId(0),
            dist: nacre_kit::Dist::One(1.5),
        },
        Step::Extrude {
            sketch: ValueId(0),
            dist: nacre_kit::Dist::One(-1.0),
        },
        Step::Boolean {
            kind: KitBool::Fuse,
            args: vec![ValueId(1), ValueId(2)],
        },
    ];
    let out = build(&steps, None).expect("build");
    let val = out.values[3]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("solid");
    assert_eq!(val.body_count(), 1, "the halves merge across z = 0");
    assert!((val.volume(&out.model) - 10.0).abs() < 1e-9, "4×1.5 + 4×1");
    // The sketch itself is reused with no copies at all — the only copies are the
    // fuse step's eager per-consumption ones (prefix-stability's flat price).
    assert_eq!(out.auto_copies, vec![3, 3]);
}

/// C3 — a two-sided extrude straddles the plane: one body per island, correct volume
/// and z-range, and no correspondence bookkeeping (every prism goes into one fold).
#[test]
fn a_two_sided_extrude_straddles_the_plane() {
    let steps = vec![
        tri_sketch(WorldPlane::XY),
        Step::Extrude {
            sketch: ValueId(0),
            dist: nacre_kit::Dist::Both(-1.0, 2.0),
        },
    ];
    let out = build(&steps, None).expect("build");
    let val = out.values[1]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("solid");
    assert_eq!(val.body_count(), 1, "the halves fuse across the plane");
    assert!((val.volume(&out.model) - 18.0).abs() < 1e-9, "6 × (1 + 2)");
    let (lo, hi) = nacre::props::bounds(&out.model, val.bodies[0]).expect("bounds");
    assert_close(lo.as_array(), [0.0, 0.0, -1.0], "lo");
    assert_close(hi.as_array(), [4.0, 3.0, 2.0], "hi");
}

/// C3 — islands stay separate bodies through a two-sided extrude, each one body.
#[test]
fn islands_survive_a_two_sided_extrude() {
    let sq = |a: f64, b: f64| PenPath {
        start: [a, a],
        segs: vec![line_to([b, a]), line_to([b, b]), line_to([a, b])],
        close_corner: None,
    };
    let steps = vec![
        Step::Sketch {
            plane: PlaneRef::World(WorldPlane::XY),
            paths: vec![
                Path::Pen(sq(0.0, 20.0)),
                Path::Pen(sq(4.0, 16.0)),
                Path::Pen(sq(8.0, 12.0)),
            ],
        },
        Step::Extrude {
            sketch: ValueId(0),
            dist: nacre_kit::Dist::Both(-1.0, 1.0),
        },
    ];
    let out = build(&steps, None).expect("build");
    let val = out.values[1]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("solid");
    assert_eq!(
        val.body_count(),
        2,
        "outer-with-hole and island, one body each"
    );
    let want = ((400.0 - 144.0) + 16.0) * 2.0;
    assert!((val.volume(&out.model) - want).abs() < 1e-9);
}

/// **A range says where the sketch starts and how far it sweeps.** It is **one** sweep
/// from a plane stated at the range's start, so it need not straddle the sketch plane and the
/// signs are the caller's business — `[1, 2]` needs no `translate` after it.
#[test]
fn a_range_need_not_straddle_its_plane() {
    for (lo, hi) in [
        (1.0, 2.0),
        (-2.0, -1.0),
        (0.0, 2.0),
        (-1.0, 0.0),
        (-1.5, 3.5),
    ] {
        let steps = vec![
            tri_sketch(WorldPlane::XY),
            Step::Extrude {
                sketch: ValueId(0),
                dist: nacre_kit::Dist::Both(lo, hi),
            },
        ];
        let out = match build(&steps, None) {
            Ok(o) => o,
            Err(e) => panic!("({lo}, {hi}): {e}"),
        };
        let val = out.values[1]
            .as_ref()
            .and_then(|v| v.as_solid())
            .expect("a solid");
        assert_eq!(val.bodies.len(), 1, "({lo}, {hi}): one island, one prism");
        let want = 6.0 * (hi - lo);
        assert!(
            (val.volume(&out.model) - want).abs() < 1e-9,
            "({lo}, {hi}): got {}, want {want}",
            val.volume(&out.model)
        );
        // The 4×3 triangle's footprint stays put; the range is where it stands in z.
        let (a, b) = nacre::props::bounds(&out.model, val.bodies[0]).expect("bounds");
        assert_close(a.as_array(), [0.0, 0.0, lo], "low corner");
        assert_close(b.as_array(), [4.0, 3.0, hi], "high corner");
    }
}

/// **And a range starts where it says on *every* plane.** The offset is taken along
/// the plane's own normal, which is a different component for each of the three; a wrong one would
/// put the prism somewhere else with the **same volume**, so the volume tests above cannot see it.
#[test]
fn a_range_starts_along_each_planes_own_normal() {
    let span = |plane: WorldPlane, lo: f64, hi: f64| {
        let steps = vec![
            tri_sketch(plane),
            Step::Extrude {
                sketch: ValueId(0),
                dist: nacre_kit::Dist::Both(lo, hi),
            },
        ];
        let out = build(&steps, None).expect("build");
        let val = out.values[1]
            .as_ref()
            .and_then(|v| v.as_solid())
            .expect("a solid");
        let (a, b) = nacre::props::bounds(&out.model, val.bodies[0]).expect("bounds");
        (a.as_array(), b.as_array())
    };
    // The footprint is the same as a one-sided extrude's; only the normal axis carries the range.
    let (lo, hi) = span(WorldPlane::XY, -1.0, 2.0);
    assert_close(lo, [0.0, 0.0, -1.0], "XY lo");
    assert_close(hi, [4.0, 3.0, 2.0], "XY hi");

    let (lo, hi) = span(WorldPlane::YZ, -1.0, 2.0);
    assert_close(lo, [-1.0, 0.0, 0.0], "YZ lo");
    assert_close(hi, [2.0, 4.0, 3.0], "YZ hi");

    let (lo, hi) = span(WorldPlane::ZX, -1.0, 2.0);
    assert_close(lo, [0.0, -1.0, 0.0], "ZX lo");
    assert_close(hi, [3.0, 2.0, 4.0], "ZX hi");

    // And a range that never touches the plane lands wholly on its far side.
    let (lo, hi) = span(WorldPlane::YZ, 1.0, 2.0);
    assert_close(lo, [1.0, 0.0, 0.0], "YZ offset lo");
    assert_close(hi, [2.0, 4.0, 3.0], "YZ offset hi");
}

/// The one thing a range still refuses: two distances that are not in order.
#[test]
fn an_unordered_range_is_refused() {
    for (lo, hi) in [(2.0, 1.0), (0.0, 0.0), (-1.0, -3.0)] {
        let steps = vec![
            tri_sketch(WorldPlane::XY),
            Step::Extrude {
                sketch: ValueId(0),
                dist: nacre_kit::Dist::Both(lo, hi),
            },
        ];
        match build(&steps, None) {
            Err(nacre_kit::KitError::Program { step: 1, .. }) => {}
            Err(other) => panic!("({lo}, {hi}): expected a program error, got {other:?}"),
            Ok(_) => panic!("({lo}, {hi}): expected a program error, got a build"),
        }
    }
}

/// A distance the exact window cannot hold says so in words.
///
/// Like its sibling refusal in the sketch layer ("is outside the exact decimal window"),
/// it says more than `DistOutsideDecimalWindow`, the Rust variant's name, which tells an author
/// nothing about their own number.
#[test]
fn a_distance_outside_the_window_says_what_a_window_is() {
    let steps = vec![
        tri_sketch(WorldPlane::XY),
        Step::Extrude {
            sketch: ValueId(0),
            dist: nacre_kit::Dist::One(1e300),
        },
    ];
    let msg = match build(&steps, None) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("expected a rejection"),
    };
    assert!(msg.contains("outside the exact decimal window"), "{msg}");
    assert!(
        msg.contains("[DistOutsideDecimalWindow]"),
        "the handle to search with: {msg}"
    );
}
