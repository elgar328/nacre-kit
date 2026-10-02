//! The Step log — what a script records and [`crate::build`] replays.
//!
//! **Every vocabulary type here is kit-local and serde-derivable.** The kernel's own
//! `Axis`/`BoolKind` deliberately carry no serde (a smaller trust base), and the wire
//! format the TS bridge writes must not wobble when kernel types evolve — so the log
//! speaks its own words and `build` translates them.
//!
//! Dimensions stay `f64` end to end: the kernel re-quantizes every dimension to the
//! shortest round-tripping decimal, which is what makes `1.1` then `6.6` land on the
//! same plane as `7.7`. The one thing the kit must therefore never do is *arithmetic*
//! on a dimension in f64 — where it needs derived dimensions (a cuboid's half-size),
//! it computes in rationals and descends to f64 exactly once.

use serde::{Deserialize, Serialize};

/// The name of the value a step produces — its index in the step list. A step that
/// produces nothing (e.g. [`Step::Display`]) still owns its index; nothing may
/// reference it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ValueId(pub u32);

/// Where a cuboid sits.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Anchor {
    /// The box's center.
    Center([f64; 3]),
    /// The box's min-XYZ corner.
    Corner([f64; 3]),
}

/// Where a cylinder sits — **its own two spellings, not the box's.**
///
/// A round thing has no corner, and offering one would be a word that means nothing
/// at the place it is written. What it has instead is a *base*: the centre of the
/// bottom circle, which is how a cylinder is placed on a drawing and how the kernel
/// states one (a centre in the sketch frame, swept along `ŵ`).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum CylAnchor {
    /// The solid's centre — half the height along the axis from the base circle.
    Center([f64; 3]),
    /// The centre of the cap at the **low end along the cylinder's axis** — the bottom one
    /// for a `Z` cylinder, and the `-X` one for an `X` cylinder.
    Base([f64; 3]),
}

/// The kit's spelling of the kernel's `BoolKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KitBool {
    Fuse,
    Cut,
    Common,
}

/// The kit's spelling of the kernel's `Axis`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KitAxis {
    X,
    Y,
    Z,
}

/// What a rotation turns about.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Pivot {
    /// The world origin.
    Origin,
    /// The value's own centroid (volume-weighted across bodies) — a *report* coordinate,
    /// lifted to an exact rational pivot; outside the decimal window it rejects honestly.
    Center,
    /// A stated point.
    At([f64; 3]),
}

/// A coordinate mirror plane, `normal-axis = offset` (e.g. `normal: X, offset: 0` is
/// the YZ plane).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MirrorPlane {
    pub normal: KitAxis,
    pub offset: f64,
}

/// One of the three world sketch planes (the kernel seeds them; no operation is
/// recorded for stating one).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldPlane {
    XY,
    YZ,
    ZX,
}

/// Where a sketch lives: a world plane directly (the K2 spelling), or a plane *value*
/// produced by [`Step::Plane`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaneRef {
    World(WorldPlane),
    Value(ValueId),
}

/// A resolved vertex reference: `vertex` is the model's vertex-store index, read off
/// [`crate::BuildOutput::vertices_of`] against the value `of`. Resolved references
/// are stable only **within one step log** — the log is one run's product, and the
/// same log replays the same model bit for bit; across script edits the *script*
/// re-runs its selectors and re-picks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VertexRef {
    pub of: ValueId,
    pub vertex: u32,
}

/// A resolved face reference — [`VertexRef`]'s twin for faces
/// ([`crate::BuildOutput::faces_of`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FaceRef {
    pub of: ValueId,
    pub face: u32,
}

/// What `plane(...)` states — the kit-local spelling of a datum-plane statement.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum PlaneSpec {
    /// `plane(XY)` — the world plane itself, as a value.
    World(WorldPlane),
    /// `plane(ZX, { origin })` — a world plane's axes at a stated origin.
    WorldAt { base: WorldPlane, origin: [f64; 3] },
    /// `plane({ origin, xPoint, yHint })` — the plane through three stated points;
    /// the written points are the definition (the kernel's `through_points`).
    Points {
        origin: [f64; 3],
        x_point: [f64; 3],
        y_hint: [f64; 3],
    },
    /// `plane(p, { offset })` — parallel to a plane value, `dist` along its normal.
    Offset { base: ValueId, dist: f64 },
    /// `plane({ through: [v1, v2, v3] })` — the plane through three vertices the model
    /// already holds, **named not measured** (a discovered vertex's coordinate is
    /// rounded; its name is exact). The order fixes the normal by the right-hand rule.
    Through { vertices: [VertexRef; 3] },
}

/// A corner modifier, declared at the vertex it modifies (coordinates stay sharp; the
/// modifier says what happens there).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Corner {
    /// A quarter arc tangent to both edges. Today between two **axis-aligned straight**
    /// edges at a right angle — the tangent points are exactly `r` back along each edge
    /// and the centre is `P + r·(û + v̂)`, all rational; any other angle's tangent points
    /// are irrational (`r / tan(θ/2)`) and wait for named points.
    Fillet(f64),
    /// Works on any corner — the kit computes the two retreat points itself (no new exact
    /// predicate is involved).
    Chamfer(f64),
}

/// One pen stroke. A corner modifier applies at a straight stroke's arrival vertex.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SketchSeg {
    LineTo {
        to: [f64; 2],
        corner: Option<Corner>,
    },
    /// An arc from where the pen stands, about `center`, turning `sweep` degrees —
    /// positive is `+x → +y` in the sketch's own frame (counter-clockwise seen from its
    /// normal side). A nonzero multiple of 90 short of a whole turn today; the end point is
    /// the kernel's to compute, in rationals.
    Arc { center: [f64; 2], sweep: f64 },
}

/// How a circle's size is written — the author's literal, kept as written so the kit
/// halves a diameter in rationals rather than in f64.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum CircleSize {
    Radius(f64),
    Diameter(f64),
}

/// A pen path — structurally closed (`close()` draws the segment back to `start`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PenPath {
    pub start: [f64; 2],
    pub segs: Vec<SketchSeg>,
    /// `close({ fillet/chamfer })` — the modifier of the corner at `start`.
    pub close_corner: Option<Corner>,
}

/// One closed path of a sketch — the only kind of thing a sketch holds. The pen is the
/// whole grammar: a path is always stated in order, and the kernel receives it as one
/// ring.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Path {
    Pen(PenPath),
    /// `circle({ center, r | d })` — its own closed path; a hole or an island by nesting.
    Circle {
        center: [f64; 2],
        size: CircleSize,
    },
}

/// How far an extrude goes. `One(d)`: `d` along the stated normal (`d < 0` goes the
/// other way, in the same frame). `Both(lo, hi)` with `lo < hi`: the range along the
/// normal — one sweep per island, from the plane re-stated where the range starts.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Dist {
    One(f64),
    Both(f64, f64),
}

/// How a value's edges are drawn. Absent fields fall back to the layer beneath —
/// the scene's own defaults, and then the app's.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EdgeStyle {
    pub color: Option<String>,
    pub opacity: Option<f64>,
    /// Screen pixels. Line width is a viewer's business, not the kernel's — nothing
    /// here is measured in the model's units.
    pub width: Option<f64>,
}

/// Whether a value shows its edges, and how.
///
/// `Off` is kept beside an opacity of zero deliberately: a transparent line is still
/// built, uploaded and blended away, while "no edges" is a statement about what the
/// picture contains and lets the viewer skip the work entirely.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Edges {
    Off,
    On(EdgeStyle),
}

/// Display styling, passed through to the app untouched — the kit never reads it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Style {
    pub color: Option<String>,
    pub opacity: Option<f64>,
    /// Line width in screen pixels — a **sketch's** own lines. A face has no width, and
    /// the width of a solid's edges lives in [`EdgeStyle`].
    pub width: Option<f64>,
    pub edges: Option<Edges>,
}

/// One recorded script action. A program is `Vec<Step>`; [`ValueId`]s are step indices.
///
/// Pre-1.0: the TS bridge does not exist yet, so this enum is free to evolve.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Step {
    /// An axis-aligned box. Translated through the extrude road — a rectangle extruded by its
    /// size — anchored by an exact rational translation.
    Cuboid {
        size: [f64; 3],
        at: Anchor,
    },
    /// A cylinder along one **world axis**, `radius` across and `height` tall. Any other
    /// direction is [`Step::Rotate`]'s job, as it is for a box.
    ///
    /// **Sugar, like [`Step::Cuboid`]**: a circle sketch on that axis' world plane,
    /// extruded `height`, then moved to its anchor by an exact translation — the same
    /// road a user's `circle({ center, r })` sketch takes.
    ///
    /// **The axis names the sketch plane, and the anchor follows it.** `Z` is the XY
    /// plane, `X` the YZ, `Y` the ZX — so the circle's centre is the anchor's two
    /// *in-plane* components and the extrude runs along `+axis`. `Base` is the cap at the
    /// low end **along that axis**, not "the bottom in z".
    ///
    /// A hole is `cut(plate, cylinder)`. The drill may stop exactly on the body's faces —
    /// a flush cap is an ordinary seating — or overshoot it, as a "through all" hole does.
    Cylinder {
        radius: f64,
        height: f64,
        at: CylAnchor,
        axis: KitAxis,
    },
    /// An n-ary boolean — the fold, and its pair-level blame, are `build`'s job.
    Boolean {
        kind: KitBool,
        args: Vec<ValueId>,
    },
    Translate {
        src: ValueId,
        offset: [f64; 3],
    },
    Rotate {
        src: ValueId,
        axis: KitAxis,
        deg: f64,
        pivot: Pivot,
    },
    Mirror {
        src: ValueId,
        plane: MirrorPlane,
    },
    /// Explicit copy. **Reads without consuming** — the kernel's `Copy` is the one
    /// operation that leaves its input live, and the kit mirrors that: `base.copy()`
    /// keeps `base` usable, which is the whole point of calling it.
    Copy {
        src: ValueId,
    },
    /// **One body of a multi-body value, as a value of its own.** A boolean can return
    /// several solids — two parts that only touch are two parts — and until this there
    /// was no way for a script to point at one of them.
    ///
    /// **Reads without consuming**, like [`Step::Copy`], and for a stronger reason: it
    /// *must* copy the chosen body. `Builder::take` consumes the **originals** and
    /// leaves the value holding fresh copies, so a body that shared its source's handle
    /// would retire that handle out from under `src` the first time it was used. Copying
    /// is what keeps the value-semantics promise — consumption stays invisible.
    ///
    /// `index` is into the value's own `bodies`, which is the order everything else
    /// already uses: the query surface enumerates "bodies in value order", and a
    /// rejection's `body k` counts the same way.
    Body {
        src: ValueId,
        index: usize,
    },
    /// A datum-plane statement — produces a plane *value* (pure data plus an interned
    /// kernel handle; never consumed, never copied).
    Plane {
        spec: PlaneSpec,
    },
    /// A sketch: closed paths — pen paths and circles — on a world plane or a plane value.
    /// Each path is lowered to one of the kernel's rings, and the kernel's ring court
    /// (`from_paths`) judges them in one sitting — simplicity, nesting (holes and islands)
    /// included.
    Sketch {
        plane: PlaneRef,
        paths: Vec<Path>,
    },
    /// Extrude a sketch. Does **not** consume the sketch — sketches are pure data,
    /// freely reused.
    Extrude {
        sketch: ValueId,
        dist: Dist,
    },
    /// Pad a boss on a face picked by appearance: the sketch's profile is drawn **in
    /// the face's own frame** — the kernel's canonical placement, whose origin is the
    /// world origin projected onto the face's plane, so on an axis-aligned face the
    /// sketch coordinates are in-plane world coordinates (draw near `f.center`; the
    /// sketch's `plane` field is not consulted here) — extruded
    /// `dist` outward and fused on. An outline that misses the face entirely is refused.
    /// Consumes `face.of`. Single-island sketches only.
    Pad {
        face: FaceRef,
        sketch: ValueId,
        dist: f64,
    },
    /// Carve a pocket — [`Step::Pad`]'s cut twin: same face frame, the tool swept `dist`
    /// inward and cut away. Deeper than the body cuts through; a cut that severs the body
    /// leaves every piece in the value.
    Pocket {
        face: FaceRef,
        sketch: ValueId,
        dist: f64,
    },
    /// Explicit display — turns the automatic DAG-leaf rendering off.
    Display {
        targets: Vec<ValueId>,
        style: Option<Style>,
    },
}

impl Step {
    /// The values this step **consumes** (supersedes in the kernel). [`Step::Display`]
    /// and [`Step::Copy`] observe without consuming — that distinction is what the
    /// automatic-copy scan counts.
    pub(crate) fn consumes(&self) -> Vec<ValueId> {
        match self {
            Step::Boolean { args, .. } => args.clone(),
            Step::Translate { src, .. } | Step::Rotate { src, .. } | Step::Mirror { src, .. } => {
                vec![*src]
            }
            Step::Pad { face, .. } | Step::Pocket { face, .. } => vec![face.of],
            Step::Copy { .. }
            | Step::Body { .. }
            | Step::Cuboid { .. }
            | Step::Cylinder { .. }
            | Step::Plane { .. }
            | Step::Sketch { .. }
            | Step::Extrude { .. }
            | Step::Display { .. } => vec![],
        }
    }

    /// The values this step **reads** in any way (consuming or not) — the list the
    /// backward-reference check validates (references point backward, at steps that
    /// produce values). The render set does **not** ask this question; it asks
    /// [`Step::consumes`], because what leaves the screen is what something new was
    /// made out of.
    ///
    /// `Plane`'s `through` deliberately does not list the solid whose vertices it
    /// names — borrowing a vertex is not a claim on the solid — and that spec is
    /// validated in the build instead. Rendering is safe either way now, but the
    /// build's check still needs every reference that appears here.
    pub(crate) fn reads(&self) -> Vec<ValueId> {
        match self {
            Step::Display { targets, .. } => targets.clone(),
            Step::Copy { src } | Step::Body { src, .. } => vec![*src],
            Step::Extrude { sketch, .. } => vec![*sketch],
            Step::Plane {
                spec: PlaneSpec::Offset { base, .. },
            } => vec![*base],
            Step::Plane { .. } => vec![],
            Step::Sketch {
                plane: PlaneRef::Value(id),
                ..
            } => vec![*id],
            Step::Pad { face, sketch, .. } | Step::Pocket { face, sketch, .. } => {
                vec![face.of, *sketch]
            }
            other => other.consumes(),
        }
    }

    /// Whether this step produces a solid value.
    pub(crate) fn produces_value(&self) -> bool {
        !matches!(self, Step::Display { .. })
    }
}
