//! The value-semantics core: free reuse, the liveness invariant, the n-ary fold
//! algebra, pair blame, multi-body propagation, determinism.

use nacre_kit::{Anchor, KitAxis, KitBool, Pivot, Step, ValueId, build};

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

fn bodies(out: &nacre_kit::BuildOutput, i: u32) -> usize {
    out.values[i as usize]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("a solid value")
        .body_count()
}

/// ① Free reuse: the same value feeds two consumers and the user never says copy.
/// The kernel's consuming semantics stay invisible.
#[test]
fn a_value_is_freely_reusable() {
    let out = run(vec![
        cuboid([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]), // 0: a
        cuboid([1.0, 0.0, 0.0], [2.0, 2.0, 2.0]), // 1: b (overlaps a)
        Step::Boolean {
            kind: KitBool::Cut,
            args: vec![v(0), v(1)],
        }, // 2: a − b
        Step::Translate {
            src: v(0),
            offset: [10.0, 0.0, 0.0],
        }, // 3: a again — the reuse
    ]);
    assert!((vol(&out, 2) - 4.0).abs() < 1e-9, "a−b is a 1×2×2 slab");
    assert!((vol(&out, 3) - 8.0).abs() < 1e-9, "the reused a is intact");
    assert!(
        !out.auto_copies.is_empty(),
        "a copy was inserted, invisibly"
    );
}

/// ② The liveness invariant: however a program reuses values — across steps, twice in
/// one boolean, as a shared tool — the kernel's `SolidNotLive` never surfaces.
/// (It would come back as `KitError::Internal`, and `build` succeeding at all is the
/// assertion.)
#[test]
fn no_liveness_error_ever_reaches_the_user() {
    // Same value twice in one boolean: A ∪ A = A.
    let out = run(vec![
        cuboid([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]),
        Step::Boolean {
            kind: KitBool::Fuse,
            args: vec![v(0), v(0)],
        },
    ]);
    assert!((vol(&out, 1) - 8.0).abs() < 1e-9, "A ∪ A = A");

    // One tool cutting two different parts, then the tool reused once more.
    let out = run(vec![
        cuboid([0.0, 0.0, 0.0], [3.0, 1.0, 1.0]),   // 0: part1
        cuboid([0.0, 2.0, 0.0], [3.0, 1.0, 1.0]),   // 1: part2
        cuboid([1.0, -1.0, -1.0], [1.0, 5.0, 3.0]), // 2: tool crossing both
        Step::Boolean {
            kind: KitBool::Cut,
            args: vec![v(0), v(2)],
        },
        Step::Boolean {
            kind: KitBool::Cut,
            args: vec![v(1), v(2)],
        },
        Step::Translate {
            src: v(2),
            offset: [0.0, 0.0, 10.0],
        },
    ]);
    for i in [3, 4] {
        assert_eq!(bodies(&out, i), 2, "the tool severs each part");
        assert!((vol(&out, i) - 2.0).abs() < 1e-9);
    }
}

/// ③ The fuse fold is a real union — transitive merging through a bridge, honest
/// disjoint termination (the three-mutually-disjoint case is exactly where a naive
/// mark scheme cycles forever), and the mixed case.
#[test]
fn fuse_folds_to_a_true_union() {
    // Chain: a and c touch only through b — one body, union volume.
    let out = run(vec![
        cuboid([0.0, 0.0, 0.0], [2.0, 1.0, 1.0]),
        cuboid([1.5, 0.0, 0.0], [2.0, 1.0, 1.0]),
        cuboid([3.0, 0.0, 0.0], [2.0, 1.0, 1.0]),
        Step::Boolean {
            kind: KitBool::Fuse,
            args: vec![v(0), v(1), v(2)],
        },
    ]);
    assert_eq!(bodies(&out, 3), 1, "the chain is one body");
    assert!((vol(&out, 3) - 5.0).abs() < 1e-9, "x spans [0,5]");

    // Three mutually disjoint boxes: the closure terminates and keeps all three.
    let out = run(vec![
        cuboid([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
        cuboid([3.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
        cuboid([6.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
        Step::Boolean {
            kind: KitBool::Fuse,
            args: vec![v(0), v(1), v(2)],
        },
    ]);
    assert_eq!(bodies(&out, 3), 3);
    assert!((vol(&out, 3) - 3.0).abs() < 1e-9);

    // Mixed: two overlapping, one apart.
    let out = run(vec![
        cuboid([0.0, 0.0, 0.0], [2.0, 1.0, 1.0]),
        cuboid([1.0, 0.0, 0.0], [2.0, 1.0, 1.0]),
        cuboid([10.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
        Step::Boolean {
            kind: KitBool::Fuse,
            args: vec![v(0), v(1), v(2)],
        },
    ]);
    assert_eq!(bodies(&out, 3), 2);
    assert!((vol(&out, 3) - 4.0).abs() < 1e-9, "3 (merged) + 1 (apart)");
}

/// ③ (cut) — a cut that splits a piece keeps cutting the new pieces with the
/// remaining tools.
#[test]
fn cut_keeps_cutting_what_it_severs() {
    let out = run(vec![
        cuboid([0.0, 0.0, 0.0], [5.0, 1.0, 1.0]), // 0: bar, volume 5
        cuboid([2.0, -1.0, -1.0], [1.0, 3.0, 3.0]), // 1: knife at x[2,3]
        cuboid([0.5, -1.0, -1.0], [0.5, 3.0, 3.0]), // 2: knife at x[0.5,1]
        Step::Boolean {
            kind: KitBool::Cut,
            args: vec![v(0), v(1), v(2)],
        },
    ]);
    assert_eq!(bodies(&out, 3), 3, "two knives leave three pieces");
    assert!((vol(&out, 3) - 3.5).abs() < 1e-9, "5 − 1 − 0.5");
}

/// ③ (common) — the distributed pairwise intersection, and the empty result as a
/// normal value.
#[test]
fn common_distributes_over_bodies() {
    let out = run(vec![
        cuboid([0.0, 0.0, 0.0], [5.0, 1.0, 1.0]),   // 0: bar
        cuboid([2.0, -1.0, -1.0], [1.0, 3.0, 3.0]), // 1: knife
        Step::Boolean {
            kind: KitBool::Cut,
            args: vec![v(0), v(1)],
        }, // 2: two pieces
        cuboid([0.0, 0.0, 0.0], [10.0, 1.0, 0.5]),  // 3: lower half slab
        Step::Boolean {
            kind: KitBool::Common,
            args: vec![v(2), v(3)],
        }, // 4: both pieces, halved
    ]);
    assert_eq!(bodies(&out, 4), 2);
    assert!((vol(&out, 4) - 2.0).abs() < 1e-9, "(2 + 2) × ½");

    // Disjoint common: zero bodies, and the empty value flows on harmlessly.
    let out = run(vec![
        cuboid([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
        cuboid([5.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
        Step::Boolean {
            kind: KitBool::Common,
            args: vec![v(0), v(1)],
        }, // 2: empty
        Step::Translate {
            src: v(2),
            offset: [1.0, 0.0, 0.0],
        }, // 3: still empty, no error
        cuboid([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]), // 4
        Step::Boolean {
            kind: KitBool::Fuse,
            args: vec![v(3), v(4)],
        }, // 5: empty ∪ box = box
    ]);
    assert_eq!(bodies(&out, 2), 0, "disjoint common is empty, not an error");
    assert_eq!(bodies(&out, 3), 0);
    assert_eq!(bodies(&out, 5), 1);
    assert!((vol(&out, 5) - 1.0).abs() < 1e-9);
}

/// ④ Pair blame: when the kernel refuses a combination, the error names which two
/// values.
///
/// Two cubes sharing exactly an edge are not this case: nothing joins there, so the kernel
/// returns the two bodies it was handed. What it refuses is a body that touches **itself**: value 2 is a block with a bridge, and value 3 meets it only along the line
/// `x = 2, y = 2` while the bridge takes the material around that contact, so cutting the pinch
/// leaves one piece and there is no pair of solids to return.
#[test]
fn a_failing_pair_is_named() {
    let steps = vec![
        cuboid([0.0, 0.0, 0.0], [2.0, 2.0, 1.0]), // 0
        cuboid([1.0, 0.3, 0.2], [2.0, 2.7, 0.6]), // 1: the bridge, overlapping 0
        Step::Boolean {
            kind: KitBool::Fuse,
            args: vec![v(0), v(1)],
        }, // 2: one body, bridged
        cuboid([5.0, 5.0, 0.0], [1.0, 1.0, 1.0]), // 3: far away — fine
        cuboid([2.0, 2.0, 0.0], [2.0, 2.0, 1.0]), // 4: meets 2 only along a line
        Step::Boolean {
            kind: KitBool::Fuse,
            args: vec![v(2), v(3), v(4)],
        },
    ];
    match build(&steps, None) {
        Err(nacre_kit::KitError::Kernel {
            step,
            blame: Some(b),
            ..
        }) => {
            assert_eq!(step, 5);
            let pair = (b.a.0.min(b.b.0), b.a.0.max(b.b.0));
            assert_eq!(pair, (2, 4), "the blame names the pinching pair");
        }
        Err(other) => panic!("expected a blamed kernel rejection, got {other:?}"),
        Ok(_) => panic!("expected a blamed kernel rejection, got a successful build"),
    }
}

/// ⑤ Multi-body propagation: a two-body value moves as one, volumes never double
/// count.
#[test]
fn a_multi_body_value_moves_as_one() {
    let out = run(vec![
        cuboid([0.0, 0.0, 0.0], [5.0, 1.0, 1.0]),
        cuboid([2.0, -1.0, -1.0], [1.0, 3.0, 3.0]),
        Step::Boolean {
            kind: KitBool::Cut,
            args: vec![v(0), v(1)],
        }, // 2: two pieces, volume 4
        Step::Translate {
            src: v(2),
            offset: [0.0, 0.0, 7.0],
        }, // 3
        Step::Rotate {
            src: v(3),
            axis: KitAxis::Z,
            deg: 90.0,
            pivot: Pivot::Origin,
        }, // 4
    ]);
    for i in [2, 3, 4] {
        assert_eq!(bodies(&out, i), 2);
        assert!((vol(&out, i) - 4.0).abs() < 1e-9, "volume rides along");
    }
}

/// Determinism: the same steps build the same model — the kit adds no nondeterminism
/// on top of kernel replay.
#[test]
fn the_same_steps_build_the_same_model() {
    let steps = vec![
        cuboid([0.0, 0.0, 0.0], [2.0, 1.0, 1.0]),
        cuboid([1.5, 0.0, 0.0], [2.0, 1.0, 1.0]),
        cuboid([6.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
        Step::Boolean {
            kind: KitBool::Fuse,
            args: vec![v(0), v(1), v(2)],
        },
        Step::Rotate {
            src: v(3),
            axis: KitAxis::Z,
            deg: 30.0,
            pivot: Pivot::At([1.0, 1.0, 0.0]),
        },
    ];
    let (a, b) = (run(steps.clone()), run(steps));
    for i in 0..a.values.len() {
        match (&a.values[i], &b.values[i]) {
            (Some(x), Some(y)) => {
                let (x, y) = (x.as_solid().expect("solid"), y.as_solid().expect("solid"));
                assert_eq!(x.body_count(), y.body_count(), "value {i}");
                assert_eq!(
                    x.volume(&a.model).to_bits(),
                    y.volume(&b.model).to_bits(),
                    "value {i} volume, bit for bit"
                );
            }
            (None, None) => {}
            _ => panic!("value {i} presence differs"),
        }
    }
    assert_eq!(a.rendered, b.rendered);
}

/// Malformed programs are caught before the kernel is asked.
#[test]
fn a_malformed_program_is_named_early() {
    let steps = vec![Step::Translate {
        src: v(0),
        offset: [1.0, 0.0, 0.0],
    }];
    assert!(matches!(
        build(&steps, None),
        Err(nacre_kit::KitError::Program { step: 0, .. })
    ));
}

// ---- Body: one piece of a multi-body value, as a value ---------------------------

/// Two unit cubes sharing exactly an edge. Nothing joins there, so the fuse is the two
/// bodies it was handed — the population this whole feature exists for.
fn touching_pair() -> Vec<Step> {
    vec![
        cuboid([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
        cuboid([1.0, 1.0, 0.0], [1.0, 1.0, 1.0]),
        Step::Boolean {
            kind: KitBool::Fuse,
            args: vec![v(0), v(1)],
        },
    ]
}

/// ⑨ Picking: each index gives *its own* body, not just "a" body. Volumes alone would
/// pass on an implementation that ignores the index (both cubes are 1.0), so the two
/// bodies are given different sizes and each is scored against the one it should be.
#[test]
fn each_index_gives_its_own_body() {
    let mut steps = vec![
        cuboid([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]), // volume 1
        cuboid([1.0, 1.0, 0.0], [2.0, 2.0, 1.0]), // volume 4 — touches the first at an edge
        Step::Boolean {
            kind: KitBool::Fuse,
            args: vec![v(0), v(1)],
        },
    ];
    steps.push(Step::Body {
        src: v(2),
        index: 0,
    });
    steps.push(Step::Body {
        src: v(2),
        index: 1,
    });
    let out = run(steps);
    assert_eq!(bodies(&out, 2), 2, "the pair only touches, so it stays two");
    assert_eq!(bodies(&out, 3), 1);
    assert_eq!(bodies(&out, 4), 1);
    let (a, b) = (vol(&out, 3), vol(&out, 4));
    let mut got = [a, b];
    got.sort_by(|x, y| x.partial_cmp(y).unwrap());
    assert!(
        (got[0] - 1.0).abs() < 1e-9,
        "one body is the unit cube: {got:?}"
    );
    assert!(
        (got[1] - 4.0).abs() < 1e-9,
        "the other is the bigger one: {got:?}"
    );
    assert!(
        (a - b).abs() > 1e-9,
        "the index decides which — they must differ"
    );
}

/// ⑩ **The source survives its body being used.** This is the whole reason `Body`
/// copies: `take` consumes the *originals* and leaves a value holding fresh ones, so a
/// body that shared its source's handle would retire that handle out from under it.
/// Drop the copy in `Step::Body` and this is what goes red.
#[test]
fn taking_a_body_leaves_the_source_whole() {
    let mut steps = touching_pair();
    steps.push(Step::Body {
        src: v(2),
        index: 0,
    });
    // Consume the body — a translate supersedes what it moves.
    steps.push(Step::Translate {
        src: v(3),
        offset: [10.0, 0.0, 0.0],
    });
    // …and only then ask the source to do something. It must still be there.
    steps.push(Step::Translate {
        src: v(2),
        offset: [0.0, 10.0, 0.0],
    });
    let out = run(steps);
    assert_eq!(
        bodies(&out, 5),
        2,
        "the source still has both of its bodies"
    );
    assert!((vol(&out, 5) - 2.0).abs() < 1e-9, "and all of its material");
}

/// ⑪ Asking for a body that is not there is named, in the script's own terms.
#[test]
fn a_body_index_past_the_end_is_named() {
    let mut steps = touching_pair();
    steps.push(Step::Body {
        src: v(2),
        index: 5,
    });
    let Err(err) = build(&steps, None) else {
        panic!("there is no body 5")
    };
    let text = format!("{err:?}");
    assert!(
        text.contains("has 2 bod"),
        "says how many there are: {text}"
    );
    assert!(text.contains('5'), "and which was asked for: {text}");
}

/// ⑫ `auto_copies` counts copies inserted **before a consumption** — inspector
/// information about what value semantics cost. `Body`'s copy is the operation itself,
/// so counting it would inflate a number whose whole meaning is "copies you did not
/// ask for".
#[test]
fn taking_a_body_is_not_an_automatic_copy() {
    let mut steps = touching_pair();
    steps.push(Step::Body {
        src: v(2),
        index: 0,
    });
    let out = run(steps);
    // Not "the list is empty" — the fuse at step 2 consumed two values and those copies
    // belong there. The claim is narrower: **`Body` adds nothing of its own.**
    assert!(
        out.auto_copies.iter().all(|&s| s != 3),
        "step 3 is the Body; its copy is the operation, not an inserted one: {:?}",
        out.auto_copies
    );
    assert!(
        !out.auto_copies.is_empty(),
        "the fuse's own copies are still counted"
    );
}

/// ⑬ The number a rejection quotes and the number `Body` takes are the same number.
/// A blamed `body k` the script cannot then point at would make the message a lie.
#[test]
fn the_blamed_body_index_is_the_one_body_takes() {
    // A pinch that cannot part: A and B meet along one line while a bridge runs the
    // material around the contact, so the fuse is refused and blames a pair.
    let steps = vec![
        cuboid([0.0, 0.0, 0.0], [2.0, 2.0, 1.0]),
        cuboid([1.0, 0.3, 0.2], [2.0, 2.7, 0.6]),
        Step::Boolean {
            kind: KitBool::Fuse,
            args: vec![v(0), v(1)],
        },
        cuboid([2.0, 2.0, 0.0], [2.0, 2.0, 1.0]),
        Step::Boolean {
            kind: KitBool::Fuse,
            args: vec![v(2), v(3)],
        },
    ];
    let Err(err) = build(&steps, None) else {
        panic!("a pinch cannot part")
    };
    let nacre_kit::KitError::Kernel { blame: Some(b), .. } = err else {
        panic!("a refused pair carries blame");
    };
    // The label counts into the value's own `bodies`, which is what `Body` indexes.
    assert!(
        b.detail.contains("body "),
        "detail names a body: {}",
        b.detail
    );
    let taken = {
        let mut s = steps[..4].to_vec();
        s.push(Step::Body { src: b.a, index: 0 });
        run(s)
    };
    assert_eq!(
        bodies(&taken, 4),
        1,
        "body 0 of the blamed value is a value"
    );
}
