//! Smoke test: the kernel is reachable through the facade, by the same road the kit
//! takes for `cuboid` — an extrude on a world frame, through the operations alone.

use nacre::prelude::*;

#[test]
fn a_cuboid_by_the_extrude_road_has_its_volume() {
    let mut m = Model::new();
    let frame = SketchFrame::world(&m, Axis::Z);
    let profile = Profile2d::polygon(vec![
        Point2::from_array([0.0, 0.0]),
        Point2::from_array([2.0, 0.0]),
        Point2::from_array([2.0, 3.0]),
        Point2::from_array([0.0, 3.0]),
    ])
    .expect("a rectangle is a simple profile");
    let out = apply(
        &mut m,
        &Operation::Extrude {
            frame,
            profile,
            dist: 4.0,
        },
    )
    .expect("a world-plane extrude");
    let OpOutput::Extrude { solid, .. } = out else {
        panic!("extrude output shape");
    };
    m.rebuild_adjacency();
    let v = nacre::props::mass_props(&m, solid)
        .expect("mass props")
        .volume;
    assert!((v - 24.0).abs() < 1e-9, "2×3×4 = 24, got {v}");
    assert!(validate(&m).is_empty());
}
