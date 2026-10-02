//! The script-level solid value: one `Solid` holding zero or more **bodies**.
//!
//! The invariant every constructor maintains: **a value's bodies are mutually
//! disjoint** — they are pieces of one union, so volumes add and display does not
//! self-intersect. The kit never *judges* disjointness itself (boundary rule); it
//! inherits the property from the kernel, whose boolean results are disjoint by
//! construction, and whose `Fuse` result count is the proof the n-ary fold leans on.

use nacre::store::Handle;
use nacre::topo::{Model, Solid};

/// A script value: zero or more mutually-disjoint kernel solids.
///
/// Empty (`bodies.is_empty()`) is a normal answer, not an error — `common` of disjoint
/// operands, a cut that consumes everything.
#[derive(Clone, Debug, Default)]
pub struct SolidValue {
    pub bodies: Vec<Handle<Solid>>,
}

impl SolidValue {
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// Total volume across bodies — a *report* number (f64 mass properties), meaningful
    /// because the bodies are disjoint.
    pub fn volume(&self, model: &Model) -> f64 {
        self.bodies
            .iter()
            .map(|&s| nacre::props::mass_props(model, s).map_or(0.0, |p| p.volume))
            .sum()
    }
}

/// The plane vocabulary — re-exported home for [`SketchValue`] and [`PlaneValue`]
/// (the step vocabulary's spellings are the serde twins).
pub use crate::step::{Path, PlaneRef, WorldPlane};
use nacre::ops::SketchFrame;
use nacre::topo::Surface;

/// A script value: a solid, a sketch, or a plane. Consumers state which kind they
/// need, and a mismatch is a program error caught before the kernel is asked.
#[derive(Clone, Debug)]
pub enum Value {
    Solid(SolidValue),
    /// Pure data — no kernel objects, so sketches are freely reusable with no copy
    /// machinery at all.
    Sketch(SketchValue),
    /// Pure data plus an *interned* kernel handle — datums intern by name, so a plane
    /// value is freely reusable with no copy machinery either. Boxed: a `SketchFrame`'s
    /// `Named` placement is six `Rat`s wide, and values accumulate one per step.
    Plane(Box<PlaneValue>),
}

impl Value {
    pub fn as_solid(&self) -> Option<&SolidValue> {
        match self {
            Value::Solid(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_sketch(&self) -> Option<&SketchValue> {
        match self {
            Value::Sketch(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_plane(&self) -> Option<&PlaneValue> {
        match self {
            Value::Plane(p) => Some(p),
            _ => None,
        }
    }
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Value::Solid(_) => "a solid",
            Value::Sketch(_) => "a sketch",
            Value::Plane(_) => "a plane",
        }
    }
}

/// A sketch value: the plane it lives on and the **statement** — the author's pen paths
/// and circles, literals untouched. Every consumer lowers the statement itself
/// (`sketch::classify`: corners resolved, arcs turned, all in rationals straight into the
/// kernel's `Rat` doors), so the kernel receives the literals the author wrote rather than
/// computed f64 points. Validation already happened at the sketch step, so an invalid sketch
/// is never stored.
#[derive(Clone, Debug)]
pub struct SketchValue {
    pub plane: PlaneRef,
    pub paths: Vec<Path>,
}

/// A built datum plane: the interned kernel handle, the frame a sketch on it extrudes
/// through, and the statement it came from — the raw material for the parallel
/// re-statement a ranged extrude starts on.
#[derive(Clone, Debug)]
pub struct PlaneValue {
    /// The interned surface — the same handle for every statement of the same plane
    /// (which is what makes `plane(XY, {offset: h})` land exactly on a box top at `h`).
    ///
    /// It names the kernel's *truth* for that plane (`nacre::topo::Surface`), not the f64
    /// realization beside it; the kit never reads either, only passes the handle back.
    pub plane: Handle<Surface>,
    pub frame: SketchFrame,
    pub src: PlaneSrc,
}

/// Where a plane value came from — enough to state the same plane with the opposite
/// frame, where that is possible at all (`WorldAxes` is; a `Points` plane's opposite
/// frame is the *user's* statement to make, by swapping the order — the axes of the
/// opposite frame are canonically derived, so no footprint mapping is the kit's to
/// invent).
///
/// There is no `Offset` variant: an offset over world axes **collapses** into
/// `WorldAxes` at the moved origin (the shift is exact — one decimal added to one
/// coordinate), and an offset over a `Points` plane is rejected at the statement
/// (its origin would be irrational, so no exact re-statement exists to pin the
/// frame's direction — and the kernel's own `Offset` datum deliberately returns a
/// canonical frame whose direction is *not* a contract).
#[derive(Clone, Debug)]
pub enum PlaneSrc {
    /// World axes at a (possibly moved) origin — flippable exactly: `v → −v`.
    WorldAxes { base: WorldPlane, origin: [f64; 3] },
    /// Three written points.
    Points {
        origin: [f64; 3],
        x_point: [f64; 3],
        y_hint: [f64; 3],
    },
    /// Three named vertices, in statement order (the order **is** the normal). The
    /// opposite frame is the user's statement — the same vertices in swapped order —
    /// so the backward extrude rejects with that guidance, like `Points`.
    Through { vertices: [u32; 3] },
}
