//! Arcs and circles in a sketch: `arc({ center, sweep })`, `circle({ center, r | d })`,
//! right-angle `{ fillet }` — and `cylinder()` as sugar for a circle sketch extruded.
//!
//! What is worth measuring: the volumes the arcs claim (`π` shows up exactly where a quarter
//! turn was drawn), the normal form the kernel applies (two fillets that meet are one half
//! circle — a rectangle becomes a slot), the cylinder primitive and a circle sketch agreeing bit
//! for bit, a backward extrude keeping the footprint, and every refusal being a sentence.

use nacre_kit::{
    CircleSize, Corner, CylAnchor, Dist, KitAxis, KitBool, KitError, Path, PenPath, Pivot,
    PlaneRef, SketchSeg, Step, ValueId, WorldPlane, build,
};

const PI: f64 = std::f64::consts::PI;

fn line_to(to: [f64; 2]) -> SketchSeg {
    SketchSeg::LineTo { to, corner: None }
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
        dist: Dist::One(dist),
    }
}

fn out_of(steps: &[Step]) -> nacre_kit::BuildOutput {
    match build(steps, None) {
        Ok(o) => o,
        Err(e) => panic!("build: {e}"),
    }
}

fn volume(out: &nacre_kit::BuildOutput, i: usize) -> f64 {
    out.values[i]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("a solid")
        .volume(&out.model)
}

fn err_val(steps: Vec<Step>) -> KitError {
    match build(&steps, None) {
        Err(e) => e,
        Ok(_) => panic!("expected a rejection"),
    }
}

fn err_of(steps: Vec<Step>) -> String {
    err_val(steps).to_string()
}

/// `rect(w, h, { fillet: r })` as a pen path with the fillet at every corner.
fn rounded_rect(w: f64, h: f64, r: f64) -> PenPath {
    rounded_rect_at(0.0, 0.0, w, h, r)
}

/// A horizontal slot from `(0, 0)` to `(len, 0)`, half-width `r`, drawn with the pen: the last
/// arc lands on the start, and `close()` draws nothing more.
fn pen_slot(len: f64, r: f64) -> PenPath {
    PenPath {
        start: [0.0, -r],
        segs: vec![
            line_to([len, -r]),
            SketchSeg::Arc {
                center: [len, 0.0],
                sweep: 180.0,
            },
            line_to([0.0, r]),
            SketchSeg::Arc {
                center: [0.0, 0.0],
                sweep: 180.0,
            },
        ],
        close_corner: None,
    }
}

/// ① A rounded rectangle is the rectangle minus four `(1 − π/4)·r²` corners — the fillet's
/// quarter arcs stand as cylinder walls, so `π` is in the volume exactly where it belongs.
#[test]
fn a_rounded_rectangle_loses_its_corners() {
    let out = out_of(&[
        sketch(vec![Path::Pen(rounded_rect(10.0, 8.0, 2.0))]),
        extrude(0, 3.0),
    ]);
    let want = (10.0 * 8.0 - (4.0 - PI) * 4.0) * 3.0;
    let got = volume(&out, 1);
    assert!((got - want).abs() < 1e-9, "got {got}, want {want}");
    assert!(
        nacre::validate::validate(&out.model).is_empty(),
        "{:?}",
        nacre::validate::validate(&out.model)
    );
}

/// ② Two fillets that meet exactly are one half circle: `rect(30 × 10, { fillet: 5 })` is a slot.
/// The kernel's normal form merges the two quarter arcs, so the solid has six faces — two caps,
/// two flat walls, two half cylinders — not eight.
#[test]
fn two_fillets_that_meet_make_a_slot() {
    let out = out_of(&[
        sketch(vec![Path::Pen(rounded_rect(30.0, 10.0, 5.0))]),
        extrude(0, 2.0),
    ]);
    let want = (20.0 * 10.0 + PI * 25.0) * 2.0;
    let got = volume(&out, 1);
    assert!((got - want).abs() < 1e-9, "got {got}, want {want}");
    let faces = out.faces_of(ValueId(1)).expect("faces");
    assert_eq!(faces.len(), 6, "caps 2 + flat walls 2 + half cylinders 2");
    // Two fillets that would overlap on their shared edge are refused, in words.
    let e = err_of(vec![sketch(vec![Path::Pen(rounded_rect(30.0, 10.0, 6.0))])]);
    assert!(e.contains("overlap"), "{e}");
}

/// ③ `close()` on a path whose last arc lands on the start draws nothing (the kernel refuses a
/// zero-length edge, so this rule is what makes the slot legal): the slot is `rect + πr²` and
/// its outline is one ring — two lines, two half circles — so the solid has six faces.
#[test]
fn a_pen_slot_closes_on_its_last_arc() {
    let pen = out_of(&[
        sketch(vec![Path::Pen(pen_slot(30.0, 5.0))]),
        extrude(0, 2.0),
    ]);
    let want = (30.0 * 10.0 + PI * 25.0) * 2.0;
    assert!((volume(&pen, 1) - want).abs() < 1e-9, "{}", volume(&pen, 1));
    assert_eq!(
        pen.faces_of(ValueId(1)).expect("faces").len(),
        6,
        "caps 2 + flat walls 2 + half cylinders 2"
    );
    assert!(
        nacre::validate::validate(&pen.model).is_empty(),
        "{:?}",
        nacre::validate::validate(&pen.model)
    );
}

/// ④ A negative extrude sweeps the slot against its plane's normal in the same frame — its arcs
/// and their sweeps stay as written — and the footprint, hence the volume, is the same.
#[test]
fn a_slot_extruded_backward_keeps_its_footprint() {
    let up = out_of(&[
        sketch(vec![Path::Pen(pen_slot(30.0, 5.0))]),
        extrude(0, 2.0),
    ]);
    let down = out_of(&[
        sketch(vec![Path::Pen(pen_slot(30.0, 5.0))]),
        extrude(0, -2.0),
    ]);
    assert!((volume(&up, 1) - volume(&down, 1)).abs() < 1e-9);
    let z = |o: &nacre_kit::BuildOutput| {
        let s = o.values[1]
            .as_ref()
            .and_then(|v| v.as_solid())
            .expect("solid");
        let b = nacre::props::bounds(&o.model, s.bodies[0]).expect("bounds");
        (b.0.as_array(), b.1.as_array())
    };
    let (lo, hi) = z(&down);
    assert_eq!((lo[2], hi[2]), (-2.0, 0.0), "below the plane");
    assert_eq!((lo[1], hi[1]), (-5.0, 5.0), "the same footprint in y");
    assert!(
        nacre::validate::validate(&down.model).is_empty(),
        "{:?}",
        nacre::validate::validate(&down.model)
    );
}

/// ⑤ `cylinder()` is sugar: a circle sketch extruded is the same solid, bit for bit — and so
/// is the circle said by diameter, halved in rationals. This is the lock that keeps the
/// primitive and the sketch on one road.
#[test]
fn a_circle_sketch_is_the_cylinder() {
    let circle = |size: CircleSize| {
        out_of(&[
            sketch(vec![Path::Circle {
                center: [2.0, 2.0],
                size,
            }]),
            extrude(0, 3.0),
        ])
    };
    let by_r = circle(CircleSize::Radius(0.5));
    let by_d = circle(CircleSize::Diameter(1.0));
    let sugar = out_of(&[Step::Cylinder {
        radius: 0.5,
        height: 3.0,
        at: CylAnchor::Base([2.0, 2.0, 0.0]),
        axis: KitAxis::Z,
    }]);
    let want = PI * 0.25 * 3.0;
    assert!((volume(&by_r, 1) - want).abs() < 1e-9);
    assert_eq!(volume(&by_r, 1).to_bits(), volume(&by_d, 1).to_bits());
    assert_eq!(volume(&by_r, 1).to_bits(), volume(&sugar, 0).to_bits());
    let bits = |o: &nacre_kit::BuildOutput, id: u32| {
        let mut v: Vec<[u64; 3]> = o
            .vertices_of(ValueId(id))
            .expect("vertices")
            .iter()
            .map(|p| p.at.map(f64::to_bits))
            .collect();
        v.sort_unstable();
        v
    };
    assert_eq!(bits(&by_r, 1), bits(&sugar, 0), "the same seam vertices");
    assert_eq!(
        by_r.faces_of(ValueId(1)).expect("faces").len(),
        sugar.faces_of(ValueId(0)).expect("faces").len()
    );
}

/// ⑥ A plate with a circular hole and a slot: the sketch nests the circle as a hole (no
/// declaration), and the prism removes `πr²·h`.
#[test]
fn a_circle_inside_a_rectangle_is_a_hole() {
    let plate = PenPath {
        start: [0.0, 0.0],
        segs: vec![
            line_to([40.0, 0.0]),
            line_to([40.0, 20.0]),
            line_to([0.0, 20.0]),
        ],
        close_corner: None,
    };
    let out = out_of(&[
        sketch(vec![
            Path::Pen(plate),
            Path::Circle {
                center: [10.0, 10.0],
                size: CircleSize::Radius(3.0),
            },
            Path::Circle {
                center: [30.0, 10.0],
                size: CircleSize::Diameter(6.0),
            },
        ]),
        extrude(0, 2.0),
    ]);
    let want = (40.0 * 20.0 - 2.0 * PI * 9.0) * 2.0;
    assert!((volume(&out, 1) - want).abs() < 1e-9, "{}", volume(&out, 1));
    assert_eq!(out.body_count_of(ValueId(1)), Some(1));
}

/// ⑦ Every refusal is a sentence: a corner beside an arc, a leaf of two arcs (the kernel's
/// own refusal, translated), a start off its circle's rational distance.
#[test]
fn the_refusals_say_what_is_wrong() {
    // A corner is declared at a lineTo's arrival; the one before an arc sits beside it.
    let beside = err_of(vec![sketch(vec![Path::Pen(PenPath {
        start: [0.0, 0.0],
        segs: vec![
            SketchSeg::LineTo {
                to: [4.0, 0.0],
                corner: Some(Corner::Fillet(1.0)),
            },
            SketchSeg::Arc {
                center: [4.0, 2.0],
                sweep: 90.0,
            },
            line_to([0.0, 4.0]),
        ],
        close_corner: None,
    })])]);
    assert!(beside.contains("beside an arc"), "{beside}");

    // A leaf: two quarter arcs of different circles between (0,0) and (5,5). The sketch court
    // accepts the region; the prism builder has no corner for it.
    let leaf = err_val(vec![
        sketch(vec![Path::Pen(PenPath {
            start: [0.0, 0.0],
            segs: vec![
                SketchSeg::Arc {
                    center: [5.0, 0.0],
                    sweep: -90.0,
                },
                SketchSeg::Arc {
                    center: [0.0, 5.0],
                    sweep: -90.0,
                },
            ],
            close_corner: None,
        })]),
        extrude(0, 1.0),
    ]);
    let said = leaf.to_string();
    assert!(said.contains("two arcs of different circles"), "{said}");
    // Its words moved into the table with the rest; the arm that carried them here is
    // gone, and the identifier came with them so this reads like every other refusal.
    assert!(
        said.contains("[ArcsMeetAtVertex]"),
        "the handle to search with: {said}"
    );
    // The step comes off the value, not the sentence: read out of a sentence it would be
    // loose — `starts_with("step 1")` also accepts `step 10`.
    assert_eq!(leaf.step(), 1, "refused at the extrude: {said}");

    // A start `√2` from the centre builds: the kernel's truth is the squared radius, so the
    // quarter turn about (1, 1) from the origin
    // — landing on (2, 0), closed by the chord — is a circular segment of area `π/2 − 1`.
    let out = out_of(&[
        sketch(vec![Path::Pen(PenPath {
            start: [0.0, 0.0], // √2 from the centre
            segs: vec![SketchSeg::Arc {
                center: [1.0, 1.0],
                sweep: 90.0,
            }],
            close_corner: None,
        })]),
        extrude(0, 2.0),
    ]);
    let want = (PI / 2.0 - 1.0) * 2.0;
    assert!((volume(&out, 1) - want).abs() < 1e-9, "{}", volume(&out, 1));
    assert!(
        nacre::validate::validate(&out.model).is_empty(),
        "{:?}",
        nacre::validate::validate(&out.model)
    );
}

/// ⑧ **A user's assembly folds, in script order.** A filleted, bored plate (XY), a
/// slot-windowed vertical plate standing on it (ZX, `y ∈ [3, 4]`) and two triangular gussets (YZ)
/// fuse into one body whose volume is the parts' algebra, `48 + (34 − π/4) + 2 · 7.5` — the parts
/// touch along faces and overlap nowhere. The hard places are faces that come close without
/// touching (the fillets' parallel pair, the slot's tangent rulings, the gusset's slanted plane
/// beside the bores) and a vertex where four faces meet (the gusset's apex on the slot plate's top
/// edge). The gussets are placed twice: at `x = 1.4` and `−2.4`, and at the script's own
/// `1.5`/`−2.5`, where a side plane runs through a fillet's axis and holds the fillet's tangent
/// ruling — one line shared by two planes and a cylinder. Both build, to the same volume.
#[test]
fn the_users_plate_slot_plate_and_gusset_fold_into_one_body() {
    let steps = |gusset_x: f64| {
        let fillet = Some(Corner::Fillet(2.0));
        let plate = PenPath {
            start: [-3.5, 4.0],
            segs: vec![
                SketchSeg::LineTo {
                    to: [-3.5, -4.0],
                    corner: fillet,
                },
                SketchSeg::LineTo {
                    to: [3.5, -4.0],
                    corner: fillet,
                },
                line_to([3.5, 4.0]),
            ],
            close_corner: None,
        };
        let bores = vec![
            Path::Circle {
                center: [-1.5, -2.0],
                size: CircleSize::Radius(1.0),
            },
            Path::Circle {
                center: [1.5, -2.0],
                size: CircleSize::Radius(1.0),
            },
        ];
        // ZX: u = +Z, v = +X. The window is a 2 × 1 slot along z.
        let wall = PenPath {
            start: [1.0, -3.5],
            segs: vec![
                line_to([6.0, -3.5]),
                line_to([6.0, 3.5]),
                line_to([1.0, 3.5]),
            ],
            close_corner: None,
        };
        let window = PenPath {
            start: [3.0, -0.5],
            segs: vec![
                line_to([4.0, -0.5]),
                SketchSeg::Arc {
                    center: [4.0, 0.0],
                    sweep: 180.0,
                },
                line_to([3.0, 0.5]),
                SketchSeg::Arc {
                    center: [3.0, 0.0],
                    sweep: 180.0,
                },
            ],
            close_corner: None,
        };
        // YZ: u = +Y, v = +Z.
        let gusset = PenPath {
            start: [0.0, 1.0],
            segs: vec![line_to([3.0, 1.0]), line_to([3.0, 6.0])],
            close_corner: None,
        };
        vec![
            Step::Sketch {
                plane: PlaneRef::World(WorldPlane::XY),
                paths: [vec![Path::Pen(plate)], bores].concat(),
            }, // 0
            extrude(0, 1.0), // 1
            Step::Sketch {
                plane: PlaneRef::World(WorldPlane::ZX),
                paths: vec![Path::Pen(wall), Path::Pen(window)],
            }, // 2
            extrude(2, 1.0), // 3
            Step::Translate {
                src: ValueId(3),
                offset: [0.0, 3.0, 0.0],
            }, // 4
            Step::Sketch {
                plane: PlaneRef::World(WorldPlane::YZ),
                paths: vec![Path::Pen(gusset)],
            }, // 5
            extrude(5, 1.0), // 6
            Step::Translate {
                src: ValueId(6),
                offset: [gusset_x, 0.0, 0.0],
            }, // 7
            extrude(5, 1.0), // 8
            Step::Translate {
                src: ValueId(8),
                offset: [-(gusset_x + 1.0), 0.0, 0.0],
            }, // 9
            Step::Boolean {
                kind: nacre_kit::KitBool::Fuse,
                args: vec![ValueId(1), ValueId(4), ValueId(7), ValueId(9)],
            }, // 10
        ]
    };
    let out = out_of(&steps(1.4));
    let bodies = out.values[10]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("a solid")
        .body_count();
    assert_eq!(bodies, 1, "one body");
    let want = 48.0 + (34.0 - PI / 4.0) + 2.0 * 7.5;
    let got = volume(&out, 10);
    assert!((got - want).abs() < 1e-9, "got {got}, want {want}");

    // The script's own gussets, on the fillets' axis planes: the same body.
    let out = out_of(&steps(1.5));
    let bodies = out.values[10]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("a solid")
        .body_count();
    assert_eq!(bodies, 1, "one body at the script's own dimensions");
    let got = volume(&out, 10);
    assert!((got - want).abs() < 1e-9, "got {got}, want {want}");
}

/// **A filleted profile extrudes across its own plane.**
///
/// A range is **one** sweep, from a plane stated at the range's start, so each fillet is stated
/// once. Two sweeps fused at the plane would state one fillet cylinder twice — same axis line,
/// same radius, same seam reference, opposite **sign** — and the kernel has no name for one
/// surface under two statements, so it would refuse the pair.
///
/// The volume is the oracle: a 20 × 20 square less four 2mm corners (each `r² − πr²/4`), ten deep.
#[test]
fn a_filleted_profile_extrudes_across_its_plane() {
    let steps = vec![
        sketch(vec![Path::Pen(rounded_rect(20.0, 20.0, 2.0))]),
        Step::Extrude {
            sketch: ValueId(0),
            dist: Dist::Both(-5.0, 5.0),
        },
    ];
    let out = out_of(&steps);
    let val = out.values[1]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("a solid");
    assert_eq!(val.bodies.len(), 1, "one prism, not two halves");
    let want = (400.0 - 4.0 * (4.0 - PI)) * 10.0;
    let got = val.volume(&out.model);
    assert!((got - want).abs() < 1e-9, "got {got}, want {want}");
    // It straddles the sketch plane, which is what the range asked for.
    let (a, b) = nacre::props::bounds(&out.model, val.bodies[0]).expect("bounds");
    assert!((a.as_array()[2] + 5.0).abs() < 1e-12, "{:?}", a.as_array());
    assert!((b.as_array()[2] - 5.0).abs() < 1e-12, "{:?}", b.as_array());
}

/// ⑨ **A user's rib plate**: a 90 × 50 plate with every corner filleted and four bores, and two
/// stepped ribs standing on it. The plate's cap ring has eight fillet tangencies and no
/// three-plane corner, so a ring-against-disk question needs witnesses beyond three-plane names.
///
/// **The whole script builds.** The fuse's volume is the parts' sum — the ribs stand on the
/// plate and overlap nothing — and the `cut` that follows takes two more bores out of it. The
/// bores' **caps** are disks on the plate's side plane, clearing a corner fillet's tangent line by
/// twice its radius, and the footprint reader has to read them as disks.
#[test]
fn the_users_rib_plate_and_the_bore_across_it_build() {
    let fillet = Some(Corner::Fillet(5.0));
    let plate = PenPath {
        start: [-45.0, -25.0],
        segs: vec![
            SketchSeg::LineTo {
                to: [45.0, -25.0],
                corner: fillet,
            },
            SketchSeg::LineTo {
                to: [45.0, 25.0],
                corner: fillet,
            },
            SketchSeg::LineTo {
                to: [-45.0, 25.0],
                corner: fillet,
            },
        ],
        close_corner: fillet,
    };
    let bores: Vec<Path> = [[38.0, 18.0], [38.0, -18.0], [-38.0, 18.0], [-38.0, -18.0]]
        .into_iter()
        .map(|center| Path::Circle {
            center,
            size: CircleSize::Diameter(7.0),
        })
        .collect();
    let rib = PenPath {
        start: [12.0, 0.0],
        segs: vec![
            line_to([12.0, 7.5]),
            line_to([27.0, 7.5]),
            line_to([27.0, 5.5]),
            line_to([62.0, 5.5]),
            line_to([62.0, -5.5]),
            line_to([27.0, -5.5]),
            line_to([27.0, -7.5]),
            line_to([12.0, -7.5]),
        ],
        close_corner: None,
    };
    let steps = |with_cut: bool| {
        let mut s = vec![
            Step::Sketch {
                plane: PlaneRef::World(WorldPlane::XY),
                paths: [vec![Path::Pen(plate.clone())], bores.clone()].concat(),
            },
            Step::Extrude {
                sketch: ValueId(0),
                dist: Dist::One(12.0),
            },
            Step::Sketch {
                plane: PlaneRef::World(WorldPlane::ZX),
                paths: vec![Path::Pen(rib.clone())],
            },
            Step::Extrude {
                sketch: ValueId(2),
                dist: Dist::Both(-20.0, 20.0),
            },
            Step::Translate {
                src: ValueId(3),
                offset: [17.5, 0.0, 0.0],
            },
            Step::Extrude {
                sketch: ValueId(2),
                dist: Dist::Both(-20.0, 20.0),
            },
            Step::Translate {
                src: ValueId(5),
                offset: [-17.5, 0.0, 0.0],
            },
            Step::Boolean {
                kind: nacre_kit::KitBool::Fuse,
                args: vec![ValueId(1), ValueId(4), ValueId(6)],
            },
        ];
        if with_cut {
            s.push(Step::Cylinder {
                radius: 10.0,
                height: 90.0,
                at: CylAnchor::Center([0.0, 0.0, 0.0]),
                axis: KitAxis::Z,
            });
            s.push(Step::Rotate {
                src: ValueId(8),
                axis: KitAxis::Y,
                deg: 90.0,
                pivot: Pivot::Origin,
            });
            s.push(Step::Translate {
                src: ValueId(9),
                offset: [0.0, 0.0, 47.0],
            });
            s.push(Step::Boolean {
                kind: nacre_kit::KitBool::Cut,
                args: vec![ValueId(7), ValueId(10)],
            });
        }
        s
    };
    let out = out_of(&steps(false));
    let body = out.values[7]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("a solid");
    assert_eq!(body.body_count(), 1, "one body");
    let pi = std::f64::consts::PI;
    let want = (90.0 * 50.0 - 4.0 * (25.0 - pi * 25.0 / 4.0)) * 12.0 - 4.0 * pi * 3.5 * 3.5 * 12.0
        + 2.0 * (15.0 * 15.0 + 35.0 * 11.0) * 40.0;
    let got = volume(&out, 7);
    assert!((got - want).abs() < 1e-9, "got {got}, want {want}");

    // **And the bore across the ribs builds.** The tool's own cap is a **disk** on the
    // plate's side plane, clearing the corner fillet's tangent line by twice its radius; read as
    // anything less, that clearance would fold to "did not clear", a tangency row would be written
    // for a contact that is not there, and the verdict would abstain on it.
    let bored = out_of(&steps(true));
    let body = bored.values[11]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("a solid");
    assert_eq!(body.body_count(), 1, "one body");
    let want_bored = want - 2.0 * pi * 100.0 * 11.0;
    let got_bored = volume(&bored, 11);
    assert!(
        (got_bored - want_bored).abs() < 1e-9,
        "got {got_bored}, want {want_bored}"
    );
}

/// A rounded rectangle placed by its min-XY corner — [`rounded_rect`] is this at the origin.
fn rounded_rect_at(x0: f64, y0: f64, w: f64, h: f64, r: f64) -> PenPath {
    let fillet = Some(Corner::Fillet(r));
    PenPath {
        start: [x0, y0],
        segs: vec![
            SketchSeg::LineTo {
                to: [x0 + w, y0],
                corner: fillet,
            },
            SketchSeg::LineTo {
                to: [x0 + w, y0 + h],
                corner: fillet,
            },
            SketchSeg::LineTo {
                to: [x0, y0 + h],
                corner: fillet,
            },
        ],
        close_corner: fillet,
    }
}

/// **A drill crosses a filleted plate at right angles.**
///
/// The user's script: a 20 × 20 plate 8 thick, every corner filleted `5`, and a `d 7` drill laid
/// along `x` through the middle. The fillets' axes stand at `(±5, ±5)` and the drill's axis runs
/// `5` away from each — closer than the radius sum `5 + 7/2`, so the *surfaces* are not clear —
/// while the **faces** never come near: a fillet's quarter arc reaches `y ∈ [5, 10]` and the drill
/// only `[−7/2, 7/2]`. Along the two axes alone neither separates them; the gate also asks along
/// the rulings' cross product, which does.
///
/// The volume is the oracle: the plate `(400 − 4(25 − 25π/4)) · 8` less the drill's `π(7/2)² · 20`
/// — the drill runs the plate's full width at `|y| ≤ 7/2`, where its walls are straight.
#[test]
fn a_drill_crosses_a_filleted_plate() {
    let steps = vec![
        sketch(vec![Path::Pen(rounded_rect_at(
            -10.0, -10.0, 20.0, 20.0, 5.0,
        ))]),
        Step::Extrude {
            sketch: ValueId(0),
            dist: Dist::Both(-4.0, 4.0),
        },
        Step::Cylinder {
            radius: 3.5,
            height: 30.0,
            at: CylAnchor::Center([0.0, 0.0, 0.0]),
            axis: KitAxis::Z,
        },
        Step::Rotate {
            src: ValueId(2),
            axis: KitAxis::Y,
            deg: 90.0,
            pivot: Pivot::Origin,
        },
        Step::Boolean {
            kind: nacre_kit::KitBool::Cut,
            args: vec![ValueId(1), ValueId(3)],
        },
    ];
    let out = out_of(&steps);
    let val = out.values[4]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("a solid");
    assert_eq!(val.bodies.len(), 1, "one plate with a hole through it");
    let want = (400.0 - 4.0 * (25.0 - 25.0 * PI / 4.0)) * 8.0 - PI * 3.5 * 3.5 * 20.0;
    let got = val.volume(&out.model);
    assert!((got - want).abs() < 1e-9, "got {got}, want {want}");
}

/// **The other half of that rule: a drill that really meets another cylinder is still refused.**
/// Two bores through one block, crossing at its centre — their axes meet, so along the common
/// perpendicular both reaches sit on the same base and no direction separates them. This is the
/// case the third direction must **not** wave through, and the name it wears is the one a
/// cylinder–cylinder road will one day retire.
#[test]
fn two_drills_that_really_cross_are_refused() {
    let steps = vec![
        Step::Cuboid {
            size: [20.0, 20.0, 20.0],
            at: nacre_kit::Anchor::Center([0.0, 0.0, 0.0]),
        },
        Step::Cylinder {
            radius: 3.0,
            height: 40.0,
            at: CylAnchor::Center([0.0, 0.0, 0.0]),
            axis: KitAxis::Z,
        },
        Step::Cylinder {
            radius: 3.0,
            height: 40.0,
            at: CylAnchor::Center([0.0, 0.0, 0.0]),
            axis: KitAxis::Z,
        },
        Step::Rotate {
            src: ValueId(2),
            axis: KitAxis::Y,
            deg: 90.0,
            pivot: Pivot::Origin,
        },
        Step::Boolean {
            kind: nacre_kit::KitBool::Cut,
            args: vec![ValueId(0), ValueId(1), ValueId(3)],
        },
    ];
    let msg = err_of(steps);
    assert!(msg.contains("cylinder_pair_contact"), "{msg}");
}

/// A rectangle with a fillet on **one** corner only — the rest square.
fn one_filleted_corner(x0: f64, y0: f64, w: f64, h: f64, r: f64) -> PenPath {
    PenPath {
        start: [x0, y0],
        segs: vec![
            SketchSeg::LineTo {
                to: [x0 + w, y0],
                corner: Some(Corner::Fillet(r)),
            },
            SketchSeg::LineTo {
                to: [x0 + w, y0 + h],
                corner: None,
            },
            SketchSeg::LineTo {
                to: [x0, y0 + h],
                corner: None,
            },
        ],
        close_corner: None,
    }
}

/// **A loop that mixes lines and arcs is read, so a thick drill goes through.**
///
/// A drill as thick as the plate makes the plate's cap planes run **within its radius**, so the
/// gate stops judging those planes and asks their **faces**. The footprint reader spells an arc as
/// the piece it is — the same circle, cut to the extent its two ends name — so on a filleted plate
/// the fillet's distance from the drill is not the question.
///
/// The volume is the oracle: a 20 × 20 plate 8 thick, less four `f`-corners, less the part of the
/// drill inside it — and that part is the drill's disk **clipped by the caps**, because a drill
/// this thick stands proud of them.
#[test]
fn a_thick_drill_crosses_a_filleted_plate() {
    let half = 4.0_f64;
    let r = 4.5_f64;
    for f in [1.0_f64, 2.0, 5.0] {
        let steps = vec![
            sketch(vec![Path::Pen(rounded_rect_at(
                -10.0, -10.0, 20.0, 20.0, f,
            ))]),
            Step::Extrude {
                sketch: ValueId(0),
                dist: Dist::Both(-half, half),
            },
            Step::Cylinder {
                radius: r,
                height: 30.0,
                at: CylAnchor::Center([0.0, 0.0, 0.0]),
                axis: KitAxis::Z,
            },
            Step::Rotate {
                src: ValueId(2),
                axis: KitAxis::Y,
                deg: 90.0,
                pivot: Pivot::Origin,
            },
            Step::Boolean {
                kind: nacre_kit::KitBool::Cut,
                args: vec![ValueId(1), ValueId(3)],
            },
        ];
        let out = out_of(&steps);
        let val = out.values[4]
            .as_ref()
            .and_then(|v| v.as_solid())
            .expect("a solid");
        assert_eq!(val.bodies.len(), 2, "the drill cuts the plate in two");
        // The plate, then the drill's disk clipped to `|z| ≤ half`, run the plate's full width.
        let cap = r * r * (half / r).acos() - half * (r * r - half * half).sqrt();
        let want =
            (400.0 - 4.0 * f * f + PI * f * f) * 2.0 * half - (PI * r * r - 2.0 * cap) * 20.0;
        let got = val.volume(&out.model);
        assert!(
            (got - want).abs() < 1e-9,
            "fillet {f}: got {got}, want {want}"
        );
    }
}

/// **And where it stops.**
///
/// A fillet `f` puts its corner arc at `y ∈ [10 − f, 10]`, and the drill's rulings sit at
/// `|y| = √(r² − h²)`, which is always **nearer the axis than `r`**. So an arc that reaches a
/// ruling is inside the drill's own reach, and the *pair* rule refuses the pair before any
/// arrangement runs: `f = 6, 7, 8` all stop there, and `f = 5` — whose arc stays at `y ≥ 5` —
/// builds (the test above). That is why the arrangement's own check is a **backstop**: with the
/// question asked about the arc rather than the whole circle, nothing end-to-end reaches it.
///
/// The square plate at exactly the half-thickness keeps its own true refusal: the drill touches
/// the caps, so the result touches itself.
#[test]
fn a_fillet_that_really_reaches_the_drill_is_refused_by_the_pair_rule() {
    let case = |profile: PenPath, r: f64| -> String {
        err_of(vec![
            sketch(vec![Path::Pen(profile)]),
            Step::Extrude {
                sketch: ValueId(0),
                dist: Dist::Both(-4.0, 4.0),
            },
            Step::Cylinder {
                radius: r,
                height: 30.0,
                at: CylAnchor::Center([0.0, 0.0, 0.0]),
                axis: KitAxis::Z,
            },
            Step::Rotate {
                src: ValueId(2),
                axis: KitAxis::Y,
                deg: 90.0,
                pivot: Pivot::Origin,
            },
            Step::Boolean {
                kind: nacre_kit::KitBool::Cut,
                args: vec![ValueId(1), ValueId(3)],
            },
        ])
    };
    for f in [6.0, 7.0, 8.0] {
        let msg = case(rounded_rect_at(-10.0, -10.0, 20.0, 20.0, f), 4.5);
        assert!(msg.contains("cylinder_pair_contact"), "fillet {f}: {msg}");
    }
    // One fillet, at the corner furthest from the drill: its arc reaches no further than the four
    // do, so the same rule speaks.
    let msg = case(one_filleted_corner(-10.0, -10.0, 20.0, 20.0, 8.0), 4.5);
    assert!(msg.contains("cylinder_pair_contact"), "{msg}");
    // No arc at all, and the drill exactly on the caps: a different and true refusal.
    let square = PenPath {
        start: [-10.0, -10.0],
        segs: vec![
            SketchSeg::LineTo {
                to: [10.0, -10.0],
                corner: None,
            },
            SketchSeg::LineTo {
                to: [10.0, 10.0],
                corner: None,
            },
            SketchSeg::LineTo {
                to: [-10.0, 10.0],
                corner: None,
            },
        ],
        close_corner: None,
    };
    let msg = case(square, 4.0);
    assert!(msg.contains("self_touching"), "{msg}");
}

/// **The far side of the pair rule.** A drill wide enough to reach the fillets is still refused
/// by the pair rule, as it must be — at a half-thickness of `6` an `r = 5.9` drill reaches
/// `y ∈ [−5.9, 5.9]` and the fillets start at `y = 5`, so nothing separates them and nothing may.
#[test]
fn a_drill_that_reaches_the_fillets_is_refused() {
    let steps = vec![
        sketch(vec![Path::Pen(rounded_rect_at(
            -10.0, -10.0, 20.0, 20.0, 5.0,
        ))]),
        Step::Extrude {
            sketch: ValueId(0),
            dist: Dist::Both(-6.0, 6.0),
        },
        Step::Cylinder {
            radius: 5.9,
            height: 30.0,
            at: CylAnchor::Center([0.0, 0.0, 0.0]),
            axis: KitAxis::Z,
        },
        Step::Rotate {
            src: ValueId(2),
            axis: KitAxis::Y,
            deg: 90.0,
            pivot: Pivot::Origin,
        },
        Step::Boolean {
            kind: nacre_kit::KitBool::Cut,
            args: vec![ValueId(1), ValueId(3)],
        },
    ];
    let msg = err_of(steps);
    assert!(msg.contains("cylinder_pair_contact"), "{msg}");
}

/// **A tangency is not a crossing, and a user's part says so.**
///
/// The script: a `40 × 46 × 46` block fused with a `d 30` cylinder up `z` and a `d 36` one along
/// `x`, then cut by a `24 × 100 × 30` slab and the two bores that follow it. The slab's own wall
/// stands at `x = ±12`, and there the `d 30` cylinder's rulings sit at `y = ±√(15² − 12²) = ±9` —
/// **exactly** the radius of the `d 18` bore whose circle lies on that wall. Curve touches curve,
/// at one point, tangentially.
///
/// What the arrangement cannot mint is a **crossing**; a tangency divides nothing, so the check
/// asks about a crossing and this builds — in debug too, where a check that read a *touch* as a
/// meeting would fire (and the app, running a release build, would never see it).
#[test]
fn a_tangency_at_the_slab_wall_is_not_a_crossing() {
    let c = |r: f64, h: f64| Step::Cylinder {
        radius: r,
        height: h,
        at: CylAnchor::Center([0.0, 0.0, 0.0]),
        axis: KitAxis::Z,
    };
    let turn = |src: u32| Step::Rotate {
        src: ValueId(src),
        axis: KitAxis::Y,
        deg: 90.0,
        pivot: Pivot::Origin,
    };
    let box_of = |size: [f64; 3]| Step::Cuboid {
        size,
        at: nacre_kit::Anchor::Center([0.0, 0.0, 0.0]),
    };
    let steps = vec![
        box_of([40.0, 46.0, 46.0]),  // 0  the block
        c(15.0, 66.0),               // 1  the boss up z
        c(7.5, 100.0),               // 2  its bore
        c(18.0, 84.0),               // 3
        turn(3),                     // 4  the boss along x
        c(9.0, 100.0),               // 5
        turn(5),                     // 6  its bore
        box_of([24.0, 100.0, 30.0]), // 7  the slab, whose wall carries the tangency
        Step::Boolean {
            kind: nacre_kit::KitBool::Fuse,
            args: vec![ValueId(0), ValueId(1), ValueId(4)],
        },
        Step::Boolean {
            kind: nacre_kit::KitBool::Cut,
            args: vec![ValueId(8), ValueId(7), ValueId(2), ValueId(6)],
        },
    ];
    let out = out_of(&steps);
    let val = out.values[9]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("a solid");
    assert_eq!(val.bodies.len(), 1, "one body");
    assert!(
        nacre::validate::validate(&out.model).is_empty(),
        "{:?}",
        nacre::validate::validate(&out.model)
    );
    // No closed form for a bicylinder minus a slab; the frozen volume is the oracle.
    let got = val.volume(&out.model);
    assert!((got - 88_813.846_390_764_46).abs() < 1e-9, "{got}");
}

/// **A circle names witnesses on its own rim.** Two filleted brackets, one on XY
/// and one on ZX, each with two bores — the shape a user reported as `no_clear_ray`.
///
/// **Measured**: the refusal's frame is a `Cell::Disk` asked against a ring, on the forward pass —
/// a cell whose whole supply was its centre. **Derived**: that centre's ray meets the fillet's
/// tangency, because the bore's centre *is* its fillet arc's centre (a corner at `(−55,−45)` with
/// `r 12` centres its arc at `(−43,−33)`) and the class chart's parity ray runs along `p_x`, so
/// the ray from `(−43,−33)` reaches `(−43,−45)` exactly. That the abstention is *that* tie rather
/// than another was not read — the ledger that names ties is test-only and this shape comes from
/// the kit. Either way the supply was one point, and a rim witness decides instead.
///
/// **The volume alone would not measure this.** What the fix changes is which coplanar region
/// owns a circular hole, and a hole moved between two faces of one plane leaves the divergence
/// integral **and** `validate` untouched. Hence the face count, the bounds and the body count.
///
/// The oracle is exactly rational because the bores are concentric with the fillet arcs at half
/// their radius: each fillet removes `r² − πr²/4` and each bore `π(r/2)²`, and the `π` cancels.
/// `(110·45 − 288)·12 + (62·110 − 288)·12 − 110·12·12 = 118488`.
#[test]
fn a_bracket_of_two_filleted_plates_fuses() {
    let p1 = PenPath {
        start: [-55.0, 0.0],
        segs: vec![
            SketchSeg::LineTo {
                to: [-55.0, -45.0],
                corner: Some(Corner::Fillet(12.0)),
            },
            SketchSeg::LineTo {
                to: [55.0, -45.0],
                corner: Some(Corner::Fillet(12.0)),
            },
            line_to([55.0, 0.0]),
        ],
        close_corner: None,
    };
    let p2 = PenPath {
        start: [0.0, -55.0],
        segs: vec![
            line_to([0.0, 55.0]),
            SketchSeg::LineTo {
                to: [62.0, 55.0],
                corner: Some(Corner::Fillet(12.0)),
            },
            SketchSeg::LineTo {
                to: [62.0, -55.0],
                corner: Some(Corner::Fillet(12.0)),
            },
        ],
        close_corner: None,
    };
    let bore = |c: [f64; 2]| Path::Circle {
        center: c,
        size: CircleSize::Diameter(12.0),
    };
    let out = out_of(&[
        Step::Sketch {
            plane: PlaneRef::World(WorldPlane::XY),
            paths: vec![Path::Pen(p1), bore([-43.0, -33.0]), bore([43.0, -33.0])],
        }, // 0
        extrude(0, 12.0), // 1
        Step::Sketch {
            plane: PlaneRef::World(WorldPlane::ZX),
            paths: vec![Path::Pen(p2), bore([50.0, 43.0]), bore([50.0, -43.0])],
        }, // 2
        extrude(2, -12.0), // 3
        Step::Boolean {
            kind: nacre_kit::KitBool::Fuse,
            args: vec![ValueId(1), ValueId(3)],
        }, // 4
    ]);
    assert_eq!(out.body_count_of(ValueId(4)), Some(1), "one body");
    let got = volume(&out, 4);
    assert!((got - 118488.0).abs() < 1e-6, "got {got}, want 118488");
    assert!(
        nacre::validate::validate(&out.model).is_empty(),
        "{:?}",
        nacre::validate::validate(&out.model)
    );
    // The four bores survive as holes, and the fused body spans both plates.
    let solid = out.values[4]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("solid");
    let (lo, hi) = nacre::props::bounds(&out.model, solid.bodies[0]).expect("bounds");
    assert_eq!(lo.as_array(), [-55.0, -45.0, 0.0]);
    assert_eq!(hi.as_array(), [55.0, 0.0, 62.0]);
    // Curved 8 — four bores, four fillets, none of them in the overlap. Planar 8 — `z = 0` and
    // `z = 12` from p1, `y = −45` and `z = 62` (each plate's outer wall), `y = 0` and `y = −12`
    // (p2's caps, the first swallowing p1's top wall), and `x = ±55`, where the two plates'
    // coplanar walls merge into one L each.
    let faces = out.faces_of(ValueId(4)).expect("faces");
    assert_eq!(faces.len(), 16, "the fused bracket's faces");
}

/// A filleted profile swept both ways off its plane, then fused: the two fillet walls lie on one
/// cylinder, and the kernel refuses a cylinder shared by two solids (`cylinder_pair_contact`).
/// Today's answer — measured the same whether the backward sweep restates the plane flipped (its
/// fillet axis then runs the other way: two statements of one cylinder) or sweeps against the
/// plane's own normal (one statement). A ranged extrude (`Dist::Both`) is the one sweep that
/// builds this shape.
#[test]
fn a_fillet_swept_both_ways_and_fused_meets_itself_on_a_cylinder() {
    let fillet = Some(Corner::Fillet(1.0));
    let sketch = Step::Sketch {
        plane: PlaneRef::World(WorldPlane::XY),
        paths: vec![Path::Pen(PenPath {
            start: [0.0, 0.0],
            segs: vec![
                SketchSeg::LineTo {
                    to: [4.0, 0.0],
                    corner: fillet,
                },
                SketchSeg::LineTo {
                    to: [4.0, 3.0],
                    corner: fillet,
                },
                line_to([0.0, 3.0]),
            ],
            close_corner: None,
        })],
    };
    let both = |dist| {
        vec![
            sketch.clone(),
            Step::Extrude {
                sketch: ValueId(0),
                dist,
            },
        ]
    };
    let mut steps = both(Dist::One(2.0));
    steps.push(Step::Extrude {
        sketch: ValueId(0),
        dist: Dist::One(-2.0),
    });
    steps.push(Step::Boolean {
        kind: KitBool::Fuse,
        args: vec![ValueId(1), ValueId(2)],
    });
    match build(&steps, None) {
        Err(e) => assert!(e.to_string().contains("[cylinder_pair_contact]"), "{e}"),
        Ok(_) => panic!("the fused halves build today — the cylinder-pair gate moved"),
    }
    let out = build(&both(Dist::Both(-2.0, 2.0)), None).expect("the ranged sweep builds");
    let v = out.values[1]
        .as_ref()
        .and_then(|v| v.as_solid())
        .expect("solid");
    assert_eq!(v.bodies.len(), 1);
}
