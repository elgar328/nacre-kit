//! Prefix-stability: the ground resolved references stand on.
//!
//! A script's selectors query a build of the steps recorded *so far* and write model
//! indices into later steps. That is sound only if step `k`'s kernel operations are a
//! function of steps `0..k` alone — which the old lookahead copy scan broke: whether a
//! consumption was "final" depended on later steps, so adding a script line moved copy
//! insertions and every later cell index. These tests are the locks on the eager
//! scheme that replaced it.

use nacre_kit::{
    Anchor, Dist, FaceRef, Path, PenPath, PlaneRef, PlaneSpec, SketchSeg, Step, ValueId, VertexRef,
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

/// ① The counterexample that killed the lookahead scan, face edition: a reference
/// picked against a prefix survives a *later* re-consumption of an upstream value.
/// Under the old scheme the last line moved a copy to before value 1's production,
/// shifting its cell indices out from under the recorded reference.
#[test]
fn a_reference_survives_a_later_upstream_reuse() {
    let prefix = vec![
        box_4x4x2(),
        Step::Translate {
            src: ValueId(0),
            offset: [0.0, 0.0, 5.0],
        },
    ];
    let out = build(&prefix, None).expect("prefix build");
    let top = out
        .faces_of(ValueId(1))
        .expect("solid faces")
        .into_iter()
        .find(|f| f.normal == Some([0.0, 0.0, 1.0]))
        .expect("a top face");

    let steps = vec![
        prefix[0].clone(),
        prefix[1].clone(),
        unit_square(),
        Step::Pad {
            face: FaceRef {
                of: ValueId(1),
                face: top.face,
            },
            sketch: ValueId(2),
            dist: 1.0,
        },
        // The line a lookahead scheme would let rewrite history: value 0's second consumption.
        Step::Translate {
            src: ValueId(0),
            offset: [10.0, 0.0, 0.0],
        },
    ];
    let out = build(&steps, None).expect("full build");
    let padded = out.values[3]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("solid");
    assert!((padded.volume(&out.model) - 33.0).abs() < 1e-9, "32 + boss");
    let (_, hi) = nacre::props::bounds(&out.model, padded.bodies[0]).expect("bounds");
    assert!(
        (hi.as_array()[2] - 8.0).abs() < 1e-12,
        "the boss rides the face the query saw (z = 7 → 8), got top {}",
        hi.as_array()[2]
    );
    let moved = out.values[4]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("solid");
    assert!((moved.volume(&out.model) - 32.0).abs() < 1e-9, "unpadded");
}

/// ① The same counterexample, vertex edition: a `through` plane over production
/// vertices survives the value's consumption (the membership check walks every
/// generation, not just the current binding).
#[test]
fn a_through_plane_survives_the_owners_consumption() {
    let prefix = vec![box_4x4x2()];
    let out = build(&prefix, None).expect("prefix build");
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
        pick([0.0, 0.0, 2.0]),
    ];
    let steps = vec![
        box_4x4x2(),
        // Consume value 0 before the plane statement — the old membership check
        // (current binding only) would reject the production indices here.
        Step::Translate {
            src: ValueId(0),
            offset: [10.0, 0.0, 0.0],
        },
        Step::Plane {
            spec: PlaneSpec::Through { vertices: vs },
        },
        unit_square_on(PlaneRef::Value(ValueId(2))),
        Step::Extrude {
            sketch: ValueId(3),
            dist: Dist::One(1.0),
        },
    ];
    let out = build(&steps, None).expect("full build");
    let prism = out.values[4]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("solid");
    assert!(
        (prism.volume(&out.model) - 1.0).abs() < 1e-9,
        "a 1×1×1 prism on the named plane"
    );
}

fn unit_square_on(plane: PlaneRef) -> Step {
    Step::Sketch {
        plane,
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

/// ② The theorem itself, measured on the arena: cut the same script at q₁ < q₂ and
/// the shorter build's cell stores are a **literal prefix** of the longer's — every
/// cell equal, embedded handles included. (Comparing *query outputs* across different
/// cut points would measure the wrong proposition: a query reports the value's
/// current generation, which legitimately advances with its consumptions. What the
/// references need is exactly this arena prefixness.)
#[test]
fn prefix_builds_agree_on_shared_values() {
    let steps = [
        box_4x4x2(),
        Step::Cuboid {
            size: [2.0, 2.0, 2.0],
            at: Anchor::Corner([10.0, 0.0, 0.0]),
        },
        Step::Boolean {
            kind: nacre_kit::KitBool::Fuse,
            args: vec![ValueId(0), ValueId(1)],
        },
        Step::Translate {
            src: ValueId(2),
            offset: [0.0, 0.0, 5.0],
        },
        // A late second consumption of value 0 — the exact shape a lookahead scheme would let
        // move earlier copies.
        Step::Translate {
            src: ValueId(0),
            offset: [0.0, 10.0, 0.0],
        },
    ];
    let builds: Vec<_> = (1..=steps.len())
        .map(|q| build(&steps[..q], None).expect("prefix build"))
        .collect();
    for (qi, a) in builds.iter().enumerate() {
        for b in &builds[qi + 1..] {
            let (ma, mb) = (&a.model, &b.model);
            assert!(ma.vertex_count() <= mb.vertex_count());
            for (h, cell) in (0..ma.vertex_count() as u32)
                .filter_map(|i| ma.vertex_handle_at(i))
                .map(|h| (h, ma.vertex(h)))
            {
                assert_eq!(
                    Some(cell),
                    mb.vertex_handle_at(h.index()).map(|h| mb.vertex(h)),
                    "vertex {} is prefix-stable",
                    h.index()
                );
            }
            for (h, cell) in (0..ma.edge_count() as u32)
                .filter_map(|i| ma.edge_handle_at(i))
                .map(|h| (h, ma.edge(h)))
            {
                assert_eq!(
                    Some(cell),
                    mb.edge_handle_at(h.index()).map(|h| mb.edge(h)),
                    "edge {} is prefix-stable",
                    h.index()
                );
            }
            for (h, cell) in (0..ma.face_count() as u32)
                .filter_map(|i| ma.face_handle_at(i))
                .map(|h| (h, ma.face(h)))
            {
                assert_eq!(
                    Some(cell),
                    mb.face_handle_at(h.index()).map(|h| mb.face(h)),
                    "face {} is prefix-stable",
                    h.index()
                );
            }
            for (h, cell) in (0..ma.shell_count() as u32)
                .filter_map(|i| ma.shell_handle_at(i))
                .map(|h| (h, ma.shell(h)))
            {
                assert_eq!(
                    Some(cell),
                    mb.shell_handle_at(h.index()).map(|h| mb.shell(h)),
                    "shell {} is prefix-stable",
                    h.index()
                );
            }
            for (h, cell) in (0..ma.solid_count() as u32)
                .filter_map(|i| ma.solid_handle_at(i))
                .map(|h| (h, ma.solid(h)))
            {
                assert_eq!(
                    Some(cell),
                    mb.solid_handle_at(h.index()).map(|h| mb.solid(h)),
                    "solid {} is prefix-stable",
                    h.index()
                );
            }
        }
    }
}
