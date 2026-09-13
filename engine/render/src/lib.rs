//! `magnat-render` — renderer voxelowy (M1, właściciel `engine/render`).
#![forbid(unsafe_code)]

pub mod camera;
pub mod gpu;
pub mod graph;
pub mod renderer;
pub mod sky;

pub use camera::{CameraMode, CameraState};
pub use gpu::GpuContext;
pub use graph::{
    GraphError, GraphSlot, PassDecl, PassId, RenderGraph, RenderPass, ResourceDesc, ResourceRef,
};
pub use renderer::{frustum_planes, sphere_in_frustum, FrameStats, Renderer, PASS_NAMES};
pub use sky::{sample_sky, sky_lut, sun_state, SkySample, SunState};
