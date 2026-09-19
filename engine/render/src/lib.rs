//! `magnat-render` — renderer voxelowy (M1, właściciel `engine/render`).
#![forbid(unsafe_code)]

pub mod camera;
pub mod clusters;
pub mod gpu;
pub mod graph;
pub mod instancing;
pub mod interiors;
pub mod pick;
pub mod renderer;
pub mod shadow;
pub mod signs;
pub mod sky;
pub mod ui;
pub mod weather;

pub use camera::{CameraMode, CameraState};
pub use clusters::{CLUSTER_CAPACITY, CLUSTER_COUNT, CLUSTER_X, CLUSTER_Y, CLUSTER_Z};
pub use gpu::GpuContext;
pub use graph::{
    GraphError, GraphSlot, PassDecl, PassId, RenderGraph, RenderPass, ResourceDesc, ResourceRef,
};
pub use instancing::{
    build_instances, Batch, GpuInstance, InstanceRenderer, InstanceScratch, LodBands, MeshSlot,
    ModelTable, PickHit, PickKind,
};
pub use interiors::{
    generate_interior, BuildingCut, CapGeometry, CapRenderer, CapVertex, CutMode, CutPlane,
    FloorUse, InteriorKit, InteriorSpec, PropModels, PropPlacement,
};
pub use renderer::{
    frustum_planes, sphere_in_frustum, FrameStats, Renderer, TerrainOverlay, PASS_NAMES,
};
pub use signs::{SignAtlas, SignGeometry, SignId, SignQuad, SignRenderer};
pub use sky::{daylight, sample_sky, sky_lut, sun_state, SkySample, SunState};
pub use weather::{fog_density, plume_density, terrain_params, WeatherRenderer};
