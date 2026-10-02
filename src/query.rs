//! The query surface — what a script's selectors *look at* before they pick.
//!
//! The division of labor: **selection reads f64, statements use names.** These
//! queries hand back report coordinates (`f64` — good enough to *choose* a vertex or
//! face by appearance) together with the model index that *names* the chosen cell; the
//! script then records the name, never the coordinate. The TS bridge and the Rust
//! tests go through this same door.
//!
//! Enumeration is deterministic: bodies in value order, shells outer-then-cavities,
//! each shell's faces in order (the kernel's own walk order — `transform_solid` calls
//! it out), vertices deduped and sorted by index. The same build enumerates the same
//! lists, which is what makes a recorded index reproducible within one step log.

use std::collections::HashSet;

use crate::build::BuildOutput;
use crate::step::ValueId;
use nacre::store::Handle;
use nacre::topo::{Model, Solid};

/// Every vertex index reachable from `bodies`, deduped and sorted — one walk for the
/// query below **and** for the build's membership check (a `through` statement may
/// only name vertices of the value it points at).
pub(crate) fn solid_vertex_indices(model: &Model, bodies: &[Handle<Solid>]) -> Vec<u32> {
    let mut seen: HashSet<u32> = HashSet::new();
    for &body in bodies {
        let solid = model.solid(body);
        for sh in std::iter::once(solid.outer).chain(solid.cavities.iter().copied()) {
            for &fh in &model.shell(sh).faces {
                let face = model.face(fh);
                for lp in std::iter::once(&face.outer).chain(face.inner.iter()) {
                    for he in &lp.half_edges {
                        for vh in model.edge(he.edge).vertices {
                            seen.insert(vh.index());
                        }
                    }
                }
            }
        }
    }
    let mut indices: Vec<u32> = seen.into_iter().collect();
    indices.sort_unstable();
    indices
}

/// One vertex of a solid value: its model index (the name a statement uses) and its
/// report coordinate (what a selector compares).
#[derive(Clone, Copy, Debug)]
pub struct VertexInfo {
    pub vertex: u32,
    pub at: [f64; 3],
}

/// One face of a solid value: its model index, and its report properties
/// (`face_props`). `normal` is `None` for a curved face — a "facing +Z" filter skips
/// those instead of failing.
#[derive(Clone, Copy, Debug)]
pub struct FaceInfo {
    pub face: u32,
    pub normal: Option<[f64; 3]>,
    pub center: [f64; 3],
    pub area: f64,
}

impl BuildOutput {
    /// A sketch's segments, in world coordinates on the plane it was drawn on.
    ///
    /// The frame is the kernel's to answer: a plane through three named vertices has a
    /// canonically derived basis, and reconstructing it from the vertices' *coordinates*
    /// would place the sketch on a plane slightly beside the real one — the very error
    /// naming the vertices exists to avoid. So the frame is realized by `frame_plane`
    /// and the sketch's `(u, v)` are read against it.
    ///
    /// What comes back is the outline **after** chamfers and fillets are resolved: what
    /// you see is what will be extruded. An arc is drawn as chords (eight a quarter turn)
    /// — a picture, not the truth. `None` for a value that is not a sketch, or for a plane
    /// whose frame cannot be realized at all — in which case a viewer should draw nothing
    /// rather than draw it somewhere wrong.
    pub fn sketch_lines(&self, id: ValueId) -> Option<Vec<[[f64; 3]; 2]>> {
        let sketch = self.values.get(id.0 as usize)?.as_ref()?.as_sketch()?;
        let frame = match sketch.plane {
            crate::step::PlaneRef::World(wp) => nacre::ops::SketchFrame::world(
                &self.model,
                match wp {
                    crate::step::WorldPlane::XY => nacre::exact::Axis::Z,
                    crate::step::WorldPlane::YZ => nacre::exact::Axis::X,
                    crate::step::WorldPlane::ZX => nacre::exact::Axis::Y,
                },
            ),
            crate::step::PlaneRef::Value(pid) => {
                self.values.get(pid.0 as usize)?.as_ref()?.as_plane()?.frame
            }
        };
        let plane = nacre::ops::frame_plane(&self.model, &frame)?;
        let (o, u, v) = (plane.origin(), plane.x_axis(), plane.y_axis());
        let at = |p: [f64; 2]| (o + u * p[0] + v * p[1]).as_array();
        // The statement validated at its own step; lowering it again cannot fail.
        let rings = crate::sketch::lower(id.0 as usize, &sketch.paths, false).ok()?;
        Some(
            crate::sketch::sample(&rings)
                .iter()
                .map(|[a, b]| [at(*a), at(*b)])
                .collect(),
        )
    }

    /// **How many bodies a solid value has.** `None` when the value is missing or not a
    /// solid — the same answer shape as the two below, so a caller reads them alike.
    ///
    /// A boolean can return several solids (two parts that only touch are two parts),
    /// and a script needs this before it can point at one of them. Counting asks the
    /// build nothing it has not already done: the value is holding the list.
    pub fn body_count_of(&self, id: ValueId) -> Option<usize> {
        Some(
            self.values
                .get(id.0 as usize)?
                .as_ref()?
                .as_solid()?
                .body_count(),
        )
    }

    /// The vertices of a solid value, deduped and index-sorted. `None` when the value
    /// is missing or not a solid.
    pub fn vertices_of(&self, id: ValueId) -> Option<Vec<VertexInfo>> {
        let v = self.values.get(id.0 as usize)?.as_ref()?.as_solid()?;
        Some(
            solid_vertex_indices(&self.model, &v.bodies)
                .into_iter()
                .map(|i| VertexInfo {
                    vertex: i,
                    at: self
                        .model
                        .vertex_point(self.model.vertex_handle_at(i).expect("walked index"))
                        .as_array(),
                })
                .collect(),
        )
    }

    /// **One vertex's coordinate, realized to `places` decimal places.**
    ///
    /// The odd one out on this surface: every other query hands back the f64 report cache,
    /// because selection only has to be good enough to *choose*. This one goes to the vertex's
    /// definition and realizes from it, so the digits are earned rather than a printout of the
    /// cache's rounding — a diagnostic, not a selector.
    ///
    /// `None` when the value is missing or not a solid, when `vertex` names no vertex of this
    /// model, or when the kernel declines to realize it (`nacre_ops::RealizeError`, which never
    /// falls back to the cache).
    ///
    /// **`handle_at` is a `?` here, not an `expect`.** `vertices_of` walks its own indices and
    /// so may assert; this index arrives from the caller — a stale one from an earlier run must
    /// be a `None`, not a panic.
    pub fn vertex_decimal(&self, id: ValueId, vertex: u32, places: usize) -> Option<[String; 3]> {
        let v = self.values.get(id.0 as usize)?.as_ref()?.as_solid()?;
        // The vertex has to belong to this value — the same membership rule a `through`
        // statement obeys, and for the same reason: an index is only a name within its value.
        solid_vertex_indices(&self.model, &v.bodies)
            .contains(&vertex)
            .then_some(())?;
        let vh = self.model.vertex_handle_at(vertex)?;
        nacre::ops::realize_vertex_decimal(&self.model, vh, places).ok()
    }

    /// The faces of a solid value, in the kernel's walk order. `None` when the value
    /// is missing or not a solid — or when a face refuses its properties, which no
    /// face the kit can build does today (the kernel documents the refusals as
    /// exotic boundaries with no producer).
    pub fn faces_of(&self, id: ValueId) -> Option<Vec<FaceInfo>> {
        let v = self.values.get(id.0 as usize)?.as_ref()?.as_solid()?;
        let mut out = Vec::new();
        for &body in &v.bodies {
            let solid = self.model.solid(body);
            for sh in std::iter::once(solid.outer).chain(solid.cavities.iter().copied()) {
                for &fh in &self.model.shell(sh).faces {
                    let props = nacre::props::face_props(&self.model, fh).ok()?;
                    out.push(FaceInfo {
                        face: fh.index(),
                        normal: props.normal.map(|n| n.as_array()),
                        center: props.centroid.as_array(),
                        area: props.area,
                    });
                }
            }
        }
        Some(out)
    }
}
