use lyon::tessellation::{
    self,
    geometry_builder::{BuffersBuilder, FillVertexConstructor, VertexBuffers},
    FillAttributes, FillTessellator, StrokeAttributes, StrokeTessellator, StrokeVertexConstructor,
};
use ruffle_core::backend::render::swf::{self, FillStyle};
use ruffle_core::backend::render::{
    srgb_to_linear, Bitmap, BitmapFormat, BitmapHandle, BitmapInfo, Color, Letterbox,
    RenderBackend, ShapeHandle, Transform,
};
use ruffle_core::shape_utils::{DistilledShape, DrawPath};
use std::borrow::Cow;
use swf::{CharacterId, DefineBitsLossless, Glyph, GradientInterpolation};

use bytemuck::{Pod, Zeroable};
use futures::executor::block_on;
use raw_window_handle::HasRawWindowHandle;

use crate::pipelines::Pipelines;
use crate::shapes::{Draw, DrawType, GradientUniforms, IncompleteDrawType, Mesh};
use crate::target::{RenderTarget, RenderTargetFrame, SwapChainTarget};
use crate::utils::{
    create_buffer_with_data, format_list, get_backend_names, gradient_spread_mode_index,
    ruffle_path_to_lyon_path, swf_bitmap_to_gl_matrix, swf_to_gl_matrix,
};
use enum_map::Enum;
use ruffle_core::color_transform::ColorTransform;

type Error = Box<dyn std::error::Error>;

#[macro_use]
mod utils;

mod bitmaps;
mod globals;
mod pipelines;
mod shapes;
pub mod target;

#[cfg(feature = "clap")]
pub mod clap;

use crate::bitmaps::BitmapSamplers;
use crate::globals::Globals;
use ruffle_core::swf::{Matrix, Twips};
use std::collections::HashMap;
use std::path::Path;
pub use wgpu;

pub struct Descriptors {
    pub device: wgpu::Device,
    queue: wgpu::Queue,
    globals: Globals,
    pipelines: Pipelines,
    bitmap_samplers: BitmapSamplers,
    msaa_sample_count: u32,
}

impl Descriptors {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Result<Self, Error> {
        // TODO: Allow this to be set from command line/settings file.
        let msaa_sample_count = 4;

        let bitmap_samplers = BitmapSamplers::new(&device);
        let globals = Globals::new(&device);
        let pipelines = Pipelines::new(
            &device,
            msaa_sample_count,
            bitmap_samplers.layout(),
            globals.layout(),
        )?;

        Ok(Self {
            device,
            queue,
            globals,
            pipelines,
            bitmap_samplers,
            msaa_sample_count,
        })
    }
}

pub struct WgpuRenderBackend<T: RenderTarget> {
    descriptors: Descriptors,
    target: T,
    frame_buffer_view: wgpu::TextureView,
    depth_texture_view: wgpu::TextureView,
    current_frame: Option<(T::Frame, wgpu::CommandEncoder)>,
    meshes: Vec<Mesh>,
    viewport_width: f32,
    viewport_height: f32,
    mask_state: MaskState,
    textures: Vec<(Option<swf::CharacterId>, Texture)>,
    num_masks: u32,
    quad_vbo: wgpu::Buffer,
    quad_ibo: wgpu::Buffer,
    quad_tex_transforms: wgpu::Buffer,
    bitmap_registry: HashMap<BitmapHandle, Bitmap>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
pub enum MaskState {
    NoMask,
    DrawMaskStencil,
    DrawMaskedContent,
    ClearMaskStencil,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct Transforms {
    world_matrix: [[f32; 4]; 4],
}

unsafe impl Pod for Transforms {}
unsafe impl Zeroable for Transforms {}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct TextureTransforms {
    u_matrix: [[f32; 4]; 4],
}

unsafe impl Pod for TextureTransforms {}
unsafe impl Zeroable for TextureTransforms {}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct ColorAdjustments {
    mult_color: [f32; 4],
    add_color: [f32; 4],
}

impl From<ColorTransform> for ColorAdjustments {
    fn from(transform: ColorTransform) -> Self {
        Self {
            mult_color: [
                transform.r_mult,
                transform.g_mult,
                transform.b_mult,
                transform.a_mult,
            ],
            add_color: [
                transform.r_add,
                transform.g_add,
                transform.b_add,
                transform.a_add,
            ],
        }
    }
}

unsafe impl Pod for ColorAdjustments {}
unsafe impl Zeroable for ColorAdjustments {}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct GPUVertex {
    position: [f32; 2],
    color: [f32; 4],
}

unsafe impl Pod for GPUVertex {}
unsafe impl Zeroable for GPUVertex {}

impl WgpuRenderBackend<SwapChainTarget> {
    pub fn for_window<W: HasRawWindowHandle>(
        window: &W,
        size: (u32, u32),
        backend: wgpu::BackendBit,
        power_preference: wgpu::PowerPreference,
        trace_path: Option<&Path>,
    ) -> Result<Self, Error> {
        if wgpu::BackendBit::SECONDARY.contains(backend) {
            log::warn!(
                "{} graphics backend support may not be fully supported.",
                format_list(&get_backend_names(backend), "and")
            );
        }

        let instance = wgpu::Instance::new(backend);

        let surface = unsafe { instance.create_surface(window) };

        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference,
            compatible_surface: Some(&surface),
        }))
        .ok_or_else(|| {
            let names = get_backend_names(backend);
            if names.is_empty() {
                "Ruffle requires hardware acceleration, but no compatible graphics device was found (no backend provided?)".to_string()
            } else {
                format!("Ruffle requires hardware acceleration, but no compatible graphics device was found supporting {}", format_list(&names, "or"))
            }
        })?;

        let (device, queue) = block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                features: Default::default(),
                limits: wgpu::Limits::default(),
            },
            trace_path,
        ))?;
        let descriptors = Descriptors::new(device, queue)?;

        let target = SwapChainTarget::new(surface, size, &descriptors.device);
        Self::new(descriptors, target)
    }
}

impl<T: RenderTarget> WgpuRenderBackend<T> {
    pub fn new(mut descriptors: Descriptors, target: T) -> Result<Self, Error> {
        let extent = wgpu::Extent3d {
            width: target.width(),
            height: target.height(),
            depth: 1,
        };

        let frame_buffer_label = create_debug_label!("Framebuffer texture");
        let frame_buffer = descriptors.device.create_texture(&wgpu::TextureDescriptor {
            label: frame_buffer_label.as_deref(),
            size: extent,
            mip_level_count: 1,
            sample_count: descriptors.msaa_sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: target.format(),
            usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
        });
        let frame_buffer_view = frame_buffer.create_view(&Default::default());

        let depth_label = create_debug_label!("Depth texture");
        let depth_texture = descriptors.device.create_texture(&wgpu::TextureDescriptor {
            label: depth_label.as_deref(),
            size: extent,
            mip_level_count: 1,
            sample_count: descriptors.msaa_sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
        });

        let depth_texture_view = depth_texture.create_view(&Default::default());

        let (quad_vbo, quad_ibo, quad_tex_transforms) = create_quad_buffers(&descriptors.device);

        let viewport_width = target.width() as f32;
        let viewport_height = target.height() as f32;

        descriptors
            .globals
            .set_resolution(target.width(), target.height());

        Ok(Self {
            descriptors,
            target,
            frame_buffer_view,
            depth_texture_view,
            current_frame: None,
            meshes: Vec::new(),
            viewport_width,
            viewport_height,
            textures: Vec::new(),

            num_masks: 0,
            mask_state: MaskState::NoMask,

            quad_vbo,
            quad_ibo,
            quad_tex_transforms,
            bitmap_registry: HashMap::new(),
        })
    }

    pub fn descriptors(self) -> Descriptors {
        self.descriptors
    }

    #[allow(clippy::cognitive_complexity)]
    fn register_shape_internal(&mut self, shape: DistilledShape) -> Mesh {
        use lyon::tessellation::{FillOptions, StrokeOptions};

        let transforms_label = create_debug_label!("Shape {} transforms ubo", shape.id);
        let transforms_ubo = self
            .descriptors
            .device
            .create_buffer(&wgpu::BufferDescriptor {
                label: transforms_label.as_deref(),
                size: std::mem::size_of::<Transforms>() as u64,
                usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
                mapped_at_creation: false,
            });

        let colors_ubo = create_buffer_with_data(
            &self.descriptors.device,
            bytemuck::cast_slice(&[ColorAdjustments {
                mult_color: [1.0, 1.0, 1.0, 1.0],
                add_color: [0.0, 0.0, 0.0, 0.0],
            }]),
            wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
            create_debug_label!("Shape {} colors ubo", shape.id),
        );

        let mut draws = Vec::new();

        let mut fill_tess = FillTessellator::new();
        let mut stroke_tess = StrokeTessellator::new();
        let mut lyon_mesh: VertexBuffers<_, u16> = VertexBuffers::new();

        #[allow(clippy::too_many_arguments)]
        fn flush_draw(
            shape_id: CharacterId,
            draw: IncompleteDrawType,
            draws: &mut Vec<Draw>,
            lyon_mesh: &mut VertexBuffers<GPUVertex, u16>,
            device: &wgpu::Device,
            pipelines: &Pipelines,
        ) {
            if lyon_mesh.vertices.is_empty() || lyon_mesh.indices.len() < 3 {
                return;
            }

            let vbo = create_buffer_with_data(
                device,
                bytemuck::cast_slice(&lyon_mesh.vertices),
                wgpu::BufferUsage::VERTEX,
                create_debug_label!("Shape {} ({}) vbo", shape_id, draw.name()),
            );

            let ibo = create_buffer_with_data(
                device,
                bytemuck::cast_slice(&lyon_mesh.indices),
                wgpu::BufferUsage::INDEX,
                create_debug_label!("Shape {} ({}) ibo", shape_id, draw.name()),
            );

            let draw_id = draws.len();

            draws.push(draw.build(
                device,
                vbo,
                ibo,
                lyon_mesh.indices.len() as u32,
                pipelines,
                shape_id,
                draw_id,
            ));

            *lyon_mesh = VertexBuffers::new();
        }

        for path in shape.paths {
            match path {
                DrawPath::Fill { style, commands } => match style {
                    FillStyle::Color(color) => {
                        let color = [
                            f32::from(color.r) / 255.0,
                            f32::from(color.g) / 255.0,
                            f32::from(color.b) / 255.0,
                            f32::from(color.a) / 255.0,
                        ];

                        let mut buffers_builder =
                            BuffersBuilder::new(&mut lyon_mesh, RuffleVertexCtor { color });

                        if let Err(e) = fill_tess.tessellate_path(
                            &ruffle_path_to_lyon_path(commands, true),
                            &FillOptions::even_odd(),
                            &mut buffers_builder,
                        ) {
                            // This may just be a degenerate path; skip it.
                            log::error!("Tessellation failure: {:?}", e);
                            continue;
                        }
                    }
                    FillStyle::LinearGradient(gradient) => {
                        flush_draw(
                            shape.id,
                            IncompleteDrawType::Color,
                            &mut draws,
                            &mut lyon_mesh,
                            &self.descriptors.device,
                            &self.descriptors.pipelines,
                        );

                        let mut buffers_builder = BuffersBuilder::new(
                            &mut lyon_mesh,
                            RuffleVertexCtor {
                                color: [1.0, 1.0, 1.0, 1.0],
                            },
                        );

                        if let Err(e) = fill_tess.tessellate_path(
                            &ruffle_path_to_lyon_path(commands, true),
                            &FillOptions::even_odd(),
                            &mut buffers_builder,
                        ) {
                            // This may just be a degenerate path; skip it.
                            log::error!("Tessellation failure: {:?}", e);
                            continue;
                        }

                        let uniforms = swf_gradient_to_uniforms(0, gradient, 0.0);
                        let matrix = swf_to_gl_matrix(gradient.matrix);

                        flush_draw(
                            shape.id,
                            IncompleteDrawType::Gradient {
                                texture_transform: matrix,
                                gradient: uniforms,
                            },
                            &mut draws,
                            &mut lyon_mesh,
                            &self.descriptors.device,
                            &self.descriptors.pipelines,
                        );
                    }
                    FillStyle::RadialGradient(gradient) => {
                        flush_draw(
                            shape.id,
                            IncompleteDrawType::Color,
                            &mut draws,
                            &mut lyon_mesh,
                            &self.descriptors.device,
                            &self.descriptors.pipelines,
                        );

                        let mut buffers_builder = BuffersBuilder::new(
                            &mut lyon_mesh,
                            RuffleVertexCtor {
                                color: [1.0, 1.0, 1.0, 1.0],
                            },
                        );

                        if let Err(e) = fill_tess.tessellate_path(
                            &ruffle_path_to_lyon_path(commands, true),
                            &FillOptions::even_odd(),
                            &mut buffers_builder,
                        ) {
                            // This may just be a degenerate path; skip it.
                            log::error!("Tessellation failure: {:?}", e);
                            continue;
                        }

                        let uniforms = swf_gradient_to_uniforms(1, gradient, 0.0);
                        let matrix = swf_to_gl_matrix(gradient.matrix);

                        flush_draw(
                            shape.id,
                            IncompleteDrawType::Gradient {
                                texture_transform: matrix,
                                gradient: uniforms,
                            },
                            &mut draws,
                            &mut lyon_mesh,
                            &self.descriptors.device,
                            &self.descriptors.pipelines,
                        );
                    }
                    FillStyle::FocalGradient {
                        gradient,
                        focal_point,
                    } => {
                        flush_draw(
                            shape.id,
                            IncompleteDrawType::Color,
                            &mut draws,
                            &mut lyon_mesh,
                            &self.descriptors.device,
                            &self.descriptors.pipelines,
                        );

                        let mut buffers_builder = BuffersBuilder::new(
                            &mut lyon_mesh,
                            RuffleVertexCtor {
                                color: [1.0, 1.0, 1.0, 1.0],
                            },
                        );

                        if let Err(e) = fill_tess.tessellate_path(
                            &ruffle_path_to_lyon_path(commands, true),
                            &FillOptions::even_odd(),
                            &mut buffers_builder,
                        ) {
                            // This may just be a degenerate path; skip it.
                            log::error!("Tessellation failure: {:?}", e);
                            continue;
                        }

                        let uniforms = swf_gradient_to_uniforms(2, gradient, *focal_point);
                        let matrix = swf_to_gl_matrix(gradient.matrix);

                        flush_draw(
                            shape.id,
                            IncompleteDrawType::Gradient {
                                texture_transform: matrix,
                                gradient: uniforms,
                            },
                            &mut draws,
                            &mut lyon_mesh,
                            &self.descriptors.device,
                            &self.descriptors.pipelines,
                        );
                    }
                    FillStyle::Bitmap {
                        id,
                        matrix,
                        is_smoothed,
                        is_repeating,
                    } => {
                        flush_draw(
                            shape.id,
                            IncompleteDrawType::Color,
                            &mut draws,
                            &mut lyon_mesh,
                            &self.descriptors.device,
                            &self.descriptors.pipelines,
                        );

                        let mut buffers_builder = BuffersBuilder::new(
                            &mut lyon_mesh,
                            RuffleVertexCtor {
                                color: [1.0, 1.0, 1.0, 1.0],
                            },
                        );

                        if let Err(e) = fill_tess.tessellate_path(
                            &ruffle_path_to_lyon_path(commands, true),
                            &FillOptions::even_odd(),
                            &mut buffers_builder,
                        ) {
                            // This may just be a degenerate path; skip it.
                            log::error!("Tessellation failure: {:?}", e);
                            continue;
                        }

                        let texture = match self
                            .textures
                            .iter()
                            .find(|(other_id, _tex)| *other_id == Some(*id))
                        {
                            None => {
                                log::error!("Couldn't fill shape with unknown bitmap {}", id);
                                continue;
                            }
                            Some(t) => &t.1,
                        };
                        let texture_view = texture.texture.create_view(&Default::default());

                        flush_draw(
                            shape.id,
                            IncompleteDrawType::Bitmap {
                                texture_transform: swf_bitmap_to_gl_matrix(
                                    *matrix,
                                    texture.width,
                                    texture.height,
                                ),
                                is_smoothed: *is_smoothed,
                                is_repeating: *is_repeating,
                                texture_view,
                                id: *id,
                            },
                            &mut draws,
                            &mut lyon_mesh,
                            &self.descriptors.device,
                            &self.descriptors.pipelines,
                        );
                    }
                },
                DrawPath::Stroke {
                    style,
                    commands,
                    is_closed,
                } => {
                    let color = [
                        f32::from(style.color.r) / 255.0,
                        f32::from(style.color.g) / 255.0,
                        f32::from(style.color.b) / 255.0,
                        f32::from(style.color.a) / 255.0,
                    ];

                    let mut buffers_builder =
                        BuffersBuilder::new(&mut lyon_mesh, RuffleVertexCtor { color });

                    // TODO(Herschel): 0 width indicates "hairline".
                    let width = if style.width.to_pixels() >= 1.0 {
                        style.width.to_pixels() as f32
                    } else {
                        1.0
                    };

                    let mut options = StrokeOptions::default()
                        .with_line_width(width)
                        .with_start_cap(match style.start_cap {
                            swf::LineCapStyle::None => tessellation::LineCap::Butt,
                            swf::LineCapStyle::Round => tessellation::LineCap::Round,
                            swf::LineCapStyle::Square => tessellation::LineCap::Square,
                        })
                        .with_end_cap(match style.end_cap {
                            swf::LineCapStyle::None => tessellation::LineCap::Butt,
                            swf::LineCapStyle::Round => tessellation::LineCap::Round,
                            swf::LineCapStyle::Square => tessellation::LineCap::Square,
                        });

                    let line_join = match style.join_style {
                        swf::LineJoinStyle::Round => tessellation::LineJoin::Round,
                        swf::LineJoinStyle::Bevel => tessellation::LineJoin::Bevel,
                        swf::LineJoinStyle::Miter(limit) => {
                            // Avoid lyon assert with small miter limits.
                            if limit >= StrokeOptions::MINIMUM_MITER_LIMIT {
                                options = options.with_miter_limit(limit);
                                tessellation::LineJoin::MiterClip
                            } else {
                                tessellation::LineJoin::Bevel
                            }
                        }
                    };
                    options = options.with_line_join(line_join);

                    if let Err(e) = stroke_tess.tessellate_path(
                        &ruffle_path_to_lyon_path(commands, is_closed),
                        &options,
                        &mut buffers_builder,
                    ) {
                        // This may just be a degenerate path; skip it.
                        log::error!("Tessellation failure: {:?}", e);
                        continue;
                    }
                }
            }
        }

        flush_draw(
            shape.id,
            IncompleteDrawType::Color,
            &mut draws,
            &mut lyon_mesh,
            &self.descriptors.device,
            &self.descriptors.pipelines,
        );

        let bind_group_label = create_debug_label!("Shape {} bindgroup", shape.id);
        let bind_group = self
            .descriptors
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &self.descriptors.pipelines.mesh_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer {
                            buffer: &transforms_ubo,
                            offset: 0,
                            size: wgpu::BufferSize::new(std::mem::size_of::<Transforms>() as u64),
                        },
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer {
                            buffer: &colors_ubo,
                            offset: 0,
                            size: wgpu::BufferSize::new(
                                std::mem::size_of::<ColorAdjustments>() as u64
                            ),
                        },
                    },
                ],
                label: bind_group_label.as_deref(),
            });

        Mesh {
            draws,
            transforms: transforms_ubo,
            colors_buffer: colors_ubo,
            colors_last: ColorTransform::default(),
            shape_id: shape.id,
            bind_group,
        }
    }

    fn register_bitmap(
        &mut self,
        id: Option<swf::CharacterId>,
        bitmap: Bitmap,
        debug_str: &str,
    ) -> BitmapInfo {
        let extent = wgpu::Extent3d {
            width: bitmap.width,
            height: bitmap.height,
            depth: 1,
        };

        let data: Cow<[u8]> = match &bitmap.data {
            BitmapFormat::Rgba(data) => Cow::Borrowed(data),
            BitmapFormat::Rgb(data) => {
                // Expand to RGBA.
                let mut as_rgba =
                    Vec::with_capacity(extent.width as usize * extent.height as usize * 4);
                for i in (0..data.len()).step_by(3) {
                    as_rgba.push(data[i]);
                    as_rgba.push(data[i + 1]);
                    as_rgba.push(data[i + 2]);
                    as_rgba.push(255);
                }
                Cow::Owned(as_rgba)
            }
        };

        let texture_label = create_debug_label!("{} Texture {:?}", debug_str, id);
        let texture = self
            .descriptors
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: texture_label.as_deref(),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsage::SAMPLED | wgpu::TextureUsage::COPY_DST,
            });

        self.descriptors.queue.write_texture(
            wgpu::TextureCopyView {
                texture: &texture,
                mip_level: 0,
                origin: Default::default(),
            },
            &data,
            wgpu::TextureDataLayout {
                offset: 0,
                bytes_per_row: 4 * extent.width,
                rows_per_image: 0,
            },
            extent,
        );

        let handle = BitmapHandle(self.textures.len());
        let width = bitmap.width;
        let height = bitmap.height;

        self.bitmap_registry.insert(handle, bitmap);
        self.textures.push((
            id,
            Texture {
                texture,
                width,
                height,
            },
        ));

        BitmapInfo {
            handle,
            width: width as u16,
            height: height as u16,
        }
    }

    pub fn target(&self) -> &T {
        &self.target
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.descriptors.device
    }
}

impl<T: RenderTarget + 'static> RenderBackend for WgpuRenderBackend<T> {
    fn set_viewport_dimensions(&mut self, width: u32, height: u32) {
        // Avoid panics from creating 0-sized framebuffers.
        let width = std::cmp::max(width, 1);
        let height = std::cmp::max(height, 1);

        self.target.resize(&self.descriptors.device, width, height);

        let label = create_debug_label!("Framebuffer texture");
        let frame_buffer = self
            .descriptors
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: label.as_deref(),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth: 1,
                },
                mip_level_count: 1,
                sample_count: self.descriptors.msaa_sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: self.target.format(),
                usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
            });
        self.frame_buffer_view = frame_buffer.create_view(&Default::default());

        let label = create_debug_label!("Depth texture");
        let depth_texture = self
            .descriptors
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: label.as_deref(),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth: 1,
                },
                mip_level_count: 1,
                sample_count: self.descriptors.msaa_sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
            });
        self.depth_texture_view = depth_texture.create_view(&Default::default());

        self.viewport_width = width as f32;
        self.viewport_height = height as f32;
        self.descriptors.globals.set_resolution(width, height);
    }

    fn register_shape(&mut self, shape: DistilledShape) -> ShapeHandle {
        let handle = ShapeHandle(self.meshes.len());
        let mesh = self.register_shape_internal(shape);
        self.meshes.push(mesh);
        handle
    }

    fn replace_shape(&mut self, shape: DistilledShape, handle: ShapeHandle) {
        let mesh = self.register_shape_internal(shape);
        self.meshes[handle.0] = mesh;
    }

    fn register_glyph_shape(&mut self, glyph: &Glyph) -> ShapeHandle {
        let shape = ruffle_core::shape_utils::swf_glyph_to_shape(glyph);
        let handle = ShapeHandle(self.meshes.len());
        let mesh = self.register_shape_internal((&shape).into());
        self.meshes.push(mesh);
        handle
    }

    fn register_bitmap_jpeg(
        &mut self,
        id: u16,
        data: &[u8],
        jpeg_tables: Option<&[u8]>,
    ) -> Result<BitmapInfo, Error> {
        let data = ruffle_core::backend::render::glue_tables_to_jpeg(data, jpeg_tables);
        self.register_bitmap_jpeg_2(id, &data[..])
    }

    fn register_bitmap_jpeg_2(&mut self, id: u16, data: &[u8]) -> Result<BitmapInfo, Error> {
        let bitmap = ruffle_core::backend::render::decode_define_bits_jpeg(data, None)?;
        Ok(self.register_bitmap(Some(id), bitmap, "JPEG2"))
    }

    fn register_bitmap_jpeg_3(
        &mut self,
        id: u16,
        jpeg_data: &[u8],
        alpha_data: &[u8],
    ) -> Result<BitmapInfo, Error> {
        let bitmap =
            ruffle_core::backend::render::decode_define_bits_jpeg(jpeg_data, Some(alpha_data))?;
        Ok(self.register_bitmap(Some(id), bitmap, "JPEG3"))
    }

    fn register_bitmap_png(&mut self, swf_tag: &DefineBitsLossless) -> Result<BitmapInfo, Error> {
        let bitmap = ruffle_core::backend::render::decode_define_bits_lossless(swf_tag)?;
        Ok(self.register_bitmap(Some(swf_tag.id), bitmap, "PNG"))
    }

    fn begin_frame(&mut self, clear: Color) {
        assert!(self.current_frame.is_none());
        self.current_frame = match self.target.get_next_texture() {
            Ok(frame) => {
                let label = create_debug_label!("Frame encoder");
                Some((
                    frame,
                    self.descriptors.device.create_command_encoder(
                        &wgpu::CommandEncoderDescriptor {
                            label: label.as_deref(),
                        },
                    ),
                ))
            }
            Err(e) => {
                log::warn!("Couldn't begin new render frame: {}", e);
                None
            }
        };

        self.mask_state = MaskState::NoMask;
        self.num_masks = 0;

        if let Some((frame_output, encoder)) = &mut self.current_frame {
            let color_attachment = if self.descriptors.msaa_sample_count >= 2 {
                &self.frame_buffer_view
            } else {
                frame_output.view()
            };
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: color_attachment,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(clear.r) / 255.0,
                            g: f64::from(clear.g) / 255.0,
                            b: f64::from(clear.b) / 255.0,
                            a: f64::from(clear.a) / 255.0,
                        }),
                        store: true,
                    },
                    resolve_target: None,
                }],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
                    attachment: &self.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: true,
                    }),
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0),
                        store: true,
                    }),
                }),
            });
            self.descriptors
                .globals
                .update_uniform(&self.descriptors.device, encoder);
        }
    }

    fn render_bitmap(&mut self, bitmap: BitmapHandle, transform: &Transform, smoothing: bool) {
        if let Some((_id, texture)) = self.textures.get(bitmap.0) {
            let (frame_output, encoder) =
                if let Some((frame_output, encoder)) = &mut self.current_frame {
                    (frame_output, encoder)
                } else {
                    return;
                };

            let transform = Transform {
                matrix: transform.matrix
                    * Matrix {
                        a: texture.width as f32,
                        d: texture.height as f32,
                        ..Default::default()
                    },
                ..*transform
            };

            let world_matrix = [
                [transform.matrix.a, transform.matrix.b, 0.0, 0.0],
                [transform.matrix.c, transform.matrix.d, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [
                    transform.matrix.tx.to_pixels() as f32,
                    transform.matrix.ty.to_pixels() as f32,
                    0.0,
                    1.0,
                ],
            ];

            let transforms_ubo = create_buffer_with_data(
                &self.descriptors.device,
                bytemuck::cast_slice(&[Transforms { world_matrix }]),
                wgpu::BufferUsage::UNIFORM,
                create_debug_label!("Bitmap {} transforms transfer buffer", bitmap.0),
            );

            let colors_ubo = create_buffer_with_data(
                &self.descriptors.device,
                bytemuck::cast_slice(&[ColorAdjustments::from(transform.color_transform)]),
                wgpu::BufferUsage::UNIFORM,
                create_debug_label!("Bitmap {} colors transfer buffer", bitmap.0),
            );

            let texture_view = texture.texture.create_view(&Default::default());

            let mesh_bind_group_label = create_debug_label!("Bitmap {} mesh bind group", bitmap.0);
            let mesh_bind_group =
                self.descriptors
                    .device
                    .create_bind_group(&wgpu::BindGroupDescriptor {
                        layout: &self.descriptors.pipelines.mesh_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::Buffer {
                                    buffer: &transforms_ubo,
                                    offset: 0,
                                    size: wgpu::BufferSize::new(
                                        std::mem::size_of::<Transforms>() as u64
                                    ),
                                },
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Buffer {
                                    buffer: &colors_ubo,
                                    offset: 0,
                                    size: wgpu::BufferSize::new(
                                        std::mem::size_of::<ColorAdjustments>() as u64,
                                    ),
                                },
                            },
                        ],
                        label: mesh_bind_group_label.as_deref(),
                    });

            let bitmap_bind_group_label = create_debug_label!("Bitmap {} bind group", bitmap.0);
            let bitmap_bind_group =
                self.descriptors
                    .device
                    .create_bind_group(&wgpu::BindGroupDescriptor {
                        layout: &self.descriptors.pipelines.bitmap_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::Buffer {
                                    buffer: &self.quad_tex_transforms,
                                    offset: 0,
                                    size: wgpu::BufferSize::new(
                                        std::mem::size_of::<TextureTransforms>() as u64,
                                    ),
                                },
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&texture_view),
                            },
                        ],
                        label: bitmap_bind_group_label.as_deref(),
                    });

            let color_attachment = if self.descriptors.msaa_sample_count >= 2 {
                &self.frame_buffer_view
            } else {
                frame_output.view()
            };
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: color_attachment,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: true,
                    },
                }],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
                    attachment: &self.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: true,
                    }),
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: true,
                    }),
                }),
            });

            render_pass.set_pipeline(
                &self
                    .descriptors
                    .pipelines
                    .bitmap_pipelines
                    .pipeline_for(self.mask_state),
            );
            render_pass.set_bind_group(0, self.descriptors.globals.bind_group(), &[]);
            render_pass.set_bind_group(1, &mesh_bind_group, &[]);
            render_pass.set_bind_group(2, &bitmap_bind_group, &[]);
            render_pass.set_bind_group(
                3,
                self.descriptors
                    .bitmap_samplers
                    .get_bind_group(false, smoothing),
                &[],
            );
            render_pass.set_vertex_buffer(0, self.quad_vbo.slice(..));
            render_pass.set_index_buffer(self.quad_ibo.slice(..), wgpu::IndexFormat::Uint32);

            match self.mask_state {
                MaskState::NoMask => (),
                MaskState::DrawMaskStencil => {
                    debug_assert!(self.num_masks > 0);
                    render_pass.set_stencil_reference(self.num_masks - 1);
                }
                MaskState::DrawMaskedContent | MaskState::ClearMaskStencil => {
                    debug_assert!(self.num_masks > 0);
                    render_pass.set_stencil_reference(self.num_masks);
                }
            };

            render_pass.draw_indexed(0..6, 0, 0..1);
        }
    }

    fn render_shape(&mut self, shape: ShapeHandle, transform: &Transform) {
        let (frame_output, encoder) = if let Some((frame_output, encoder)) = &mut self.current_frame
        {
            (frame_output, encoder)
        } else {
            return;
        };

        let mesh = &mut self.meshes[shape.0];

        let world_matrix = [
            [transform.matrix.a, transform.matrix.b, 0.0, 0.0],
            [transform.matrix.c, transform.matrix.d, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [
                transform.matrix.tx.to_pixels() as f32,
                transform.matrix.ty.to_pixels() as f32,
                0.0,
                1.0,
            ],
        ];

        if transform.color_transform != mesh.colors_last {
            let colors_temp = create_buffer_with_data(
                &self.descriptors.device,
                bytemuck::cast_slice(&[ColorAdjustments::from(transform.color_transform)]),
                wgpu::BufferUsage::COPY_SRC,
                create_debug_label!("Shape {} colors transfer buffer", mesh.shape_id),
            );

            encoder.copy_buffer_to_buffer(
                &colors_temp,
                0,
                &mesh.colors_buffer,
                0,
                std::mem::size_of::<ColorAdjustments>() as u64,
            );

            mesh.colors_last = transform.color_transform;
        }

        let transforms_temp = create_buffer_with_data(
            &self.descriptors.device,
            bytemuck::cast_slice(&[Transforms { world_matrix }]),
            wgpu::BufferUsage::COPY_SRC,
            create_debug_label!("Shape {} transforms transfer buffer", mesh.shape_id),
        );

        encoder.copy_buffer_to_buffer(
            &transforms_temp,
            0,
            &mesh.transforms,
            0,
            std::mem::size_of::<Transforms>() as u64,
        );

        let color_attachment = if self.descriptors.msaa_sample_count >= 2 {
            &self.frame_buffer_view
        } else {
            frame_output.view()
        };
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: color_attachment,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: true,
                },
            }],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
                attachment: &self.depth_texture_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: true,
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: true,
                }),
            }),
        });

        render_pass.set_bind_group(0, self.descriptors.globals.bind_group(), &[]);
        render_pass.set_bind_group(1, &mesh.bind_group, &[]);

        for draw in &mesh.draws {
            match &draw.draw_type {
                DrawType::Color => {
                    render_pass.set_pipeline(
                        &self
                            .descriptors
                            .pipelines
                            .color_pipelines
                            .pipeline_for(self.mask_state),
                    );
                }
                DrawType::Gradient { bind_group, .. } => {
                    render_pass.set_pipeline(
                        &self
                            .descriptors
                            .pipelines
                            .gradient_pipelines
                            .pipeline_for(self.mask_state),
                    );
                    render_pass.set_bind_group(2, bind_group, &[]);
                }
                DrawType::Bitmap {
                    is_repeating,
                    is_smoothed,
                    bind_group,
                    ..
                } => {
                    render_pass.set_pipeline(
                        &self
                            .descriptors
                            .pipelines
                            .bitmap_pipelines
                            .pipeline_for(self.mask_state),
                    );
                    render_pass.set_bind_group(2, bind_group, &[]);
                    render_pass.set_bind_group(
                        3,
                        self.descriptors
                            .bitmap_samplers
                            .get_bind_group(*is_repeating, *is_smoothed),
                        &[],
                    );
                }
            }

            render_pass.set_vertex_buffer(0, draw.vertex_buffer.slice(..));
            render_pass.set_index_buffer(draw.index_buffer.slice(..), wgpu::IndexFormat::Uint32);

            match self.mask_state {
                MaskState::NoMask => (),
                MaskState::DrawMaskStencil => {
                    debug_assert!(self.num_masks > 0);
                    render_pass.set_stencil_reference(self.num_masks - 1);
                }
                MaskState::DrawMaskedContent | MaskState::ClearMaskStencil => {
                    debug_assert!(self.num_masks > 0);
                    render_pass.set_stencil_reference(self.num_masks);
                }
            };

            render_pass.draw_indexed(0..draw.index_count, 0, 0..1);
        }
    }

    fn draw_rect(&mut self, color: Color, matrix: &Matrix) {
        let (frame_output, encoder) = if let Some((frame_output, encoder)) = &mut self.current_frame
        {
            (frame_output, encoder)
        } else {
            return;
        };

        let world_matrix = [
            [matrix.a, matrix.b, 0.0, 0.0],
            [matrix.c, matrix.d, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [
                matrix.tx.to_pixels() as f32,
                matrix.ty.to_pixels() as f32,
                0.0,
                1.0,
            ],
        ];

        let mult_color = [
            f32::from(color.r) / 255.0,
            f32::from(color.g) / 255.0,
            f32::from(color.b) / 255.0,
            f32::from(color.a) / 255.0,
        ];

        let add_color = [0.0, 0.0, 0.0, 0.0];

        let transforms_ubo = create_buffer_with_data(
            &self.descriptors.device,
            bytemuck::cast_slice(&[Transforms { world_matrix }]),
            wgpu::BufferUsage::UNIFORM,
            create_debug_label!("Rectangle transfer buffer"),
        );

        let colors_ubo = create_buffer_with_data(
            &self.descriptors.device,
            bytemuck::cast_slice(&[ColorAdjustments {
                mult_color,
                add_color,
            }]),
            wgpu::BufferUsage::UNIFORM,
            create_debug_label!("Rectangle colors transfer buffer"),
        );

        let bind_group_label = create_debug_label!("Rectangle bind group");
        let bind_group = self
            .descriptors
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &self.descriptors.pipelines.mesh_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer {
                            buffer: &transforms_ubo,
                            offset: 0,
                            size: wgpu::BufferSize::new(std::mem::size_of::<Transforms>() as u64),
                        },
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer {
                            buffer: &colors_ubo,
                            offset: 0,
                            size: wgpu::BufferSize::new(
                                std::mem::size_of::<ColorAdjustments>() as u64
                            ),
                        },
                    },
                ],
                label: bind_group_label.as_deref(),
            });

        let color_attachment = if self.descriptors.msaa_sample_count >= 2 {
            &self.frame_buffer_view
        } else {
            frame_output.view()
        };
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: color_attachment,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: true,
                },
            }],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
                attachment: &self.depth_texture_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: true,
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: true,
                }),
            }),
        });

        render_pass.set_pipeline(
            &self
                .descriptors
                .pipelines
                .color_pipelines
                .pipeline_for(self.mask_state),
        );
        render_pass.set_bind_group(0, self.descriptors.globals.bind_group(), &[]);
        render_pass.set_bind_group(1, &bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.quad_vbo.slice(..));
        render_pass.set_index_buffer(self.quad_ibo.slice(..), wgpu::IndexFormat::Uint32);

        match self.mask_state {
            MaskState::NoMask => (),
            MaskState::DrawMaskStencil => {
                debug_assert!(self.num_masks > 0);
                render_pass.set_stencil_reference(self.num_masks - 1);
            }
            MaskState::DrawMaskedContent | MaskState::ClearMaskStencil => {
                debug_assert!(self.num_masks > 0);
                render_pass.set_stencil_reference(self.num_masks);
            }
        };

        render_pass.draw_indexed(0..6, 0, 0..1);
    }

    fn end_frame(&mut self) {
        if let Some((frame_output, mut encoder)) = self.current_frame.take() {
            // Resolve MSAA.
            if self.descriptors.msaa_sample_count >= 2 {
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                        attachment: &self.frame_buffer_view,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: false,
                        },
                        resolve_target: Some(frame_output.view()),
                    }],
                    depth_stencil_attachment: None,
                });
            }

            self.target.submit(
                &self.descriptors.device,
                &self.descriptors.queue,
                vec![encoder.finish()],
            );
        }
    }

    fn draw_letterbox(&mut self, letterbox: Letterbox) {
        match letterbox {
            Letterbox::None => {}
            Letterbox::Letterbox(margin) => {
                self.draw_rect(
                    Color {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 255,
                    },
                    &Matrix::create_box(
                        self.viewport_width,
                        margin,
                        0.0,
                        Twips::zero(),
                        Twips::zero(),
                    ),
                );
                self.draw_rect(
                    Color {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 255,
                    },
                    &Matrix::create_box(
                        self.viewport_width,
                        margin,
                        0.0,
                        Twips::zero(),
                        Twips::from_pixels((self.viewport_height - margin) as f64),
                    ),
                );
            }
            Letterbox::Pillarbox(margin) => {
                self.draw_rect(
                    Color {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 255,
                    },
                    &Matrix::create_box(
                        margin,
                        self.viewport_height,
                        0.0,
                        Twips::zero(),
                        Twips::zero(),
                    ),
                );
                self.draw_rect(
                    Color {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 255,
                    },
                    &Matrix::create_box(
                        margin,
                        self.viewport_height,
                        0.0,
                        Twips::from_pixels((self.viewport_width - margin) as f64),
                        Twips::zero(),
                    ),
                );
            }
        }
    }

    fn push_mask(&mut self) {
        assert!(
            self.mask_state == MaskState::NoMask || self.mask_state == MaskState::DrawMaskedContent
        );
        self.num_masks += 1;
        self.mask_state = MaskState::DrawMaskStencil;
    }

    fn activate_mask(&mut self) {
        assert!(self.num_masks > 0 && self.mask_state == MaskState::DrawMaskStencil);
        self.mask_state = MaskState::DrawMaskedContent;
    }

    fn deactivate_mask(&mut self) {
        assert!(self.num_masks > 0 && self.mask_state == MaskState::DrawMaskedContent);
        self.mask_state = MaskState::ClearMaskStencil;
    }

    fn pop_mask(&mut self) {
        assert!(self.num_masks > 0 && self.mask_state == MaskState::ClearMaskStencil);
        self.num_masks -= 1;
        self.mask_state = if self.num_masks == 0 {
            MaskState::NoMask
        } else {
            MaskState::DrawMaskedContent
        };
    }

    fn get_bitmap_pixels(&mut self, bitmap: BitmapHandle) -> Option<Bitmap> {
        self.bitmap_registry.get(&bitmap).cloned()
    }

    fn register_bitmap_raw(
        &mut self,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<BitmapHandle, Error> {
        Ok(self
            .register_bitmap(
                None,
                Bitmap {
                    height,
                    width,
                    data: BitmapFormat::Rgba(rgba),
                },
                "RAW",
            )
            .handle)
    }

    fn update_texture(
        &mut self,
        handle: BitmapHandle,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<BitmapHandle, Error> {
        let texture = if let Some((_id, texture)) = self.textures.get(handle.0) {
            &texture.texture
        } else {
            return Err("update_texture: Bitmap not registered".into());
        };

        let extent = wgpu::Extent3d {
            width,
            height,
            depth: 1,
        };

        self.descriptors.queue.write_texture(
            wgpu::TextureCopyView {
                texture: &texture,
                mip_level: 0,
                origin: Default::default(),
            },
            &rgba,
            wgpu::TextureDataLayout {
                offset: 0,
                bytes_per_row: 4 * extent.width,
                rows_per_image: 0,
            },
            extent,
        );

        Ok(handle)
    }
}

fn create_quad_buffers(device: &wgpu::Device) -> (wgpu::Buffer, wgpu::Buffer, wgpu::Buffer) {
    let vertices = [
        GPUVertex {
            position: [0.0, 0.0],
            color: [1.0, 1.0, 1.0, 1.0],
        },
        GPUVertex {
            position: [1.0, 0.0],
            color: [1.0, 1.0, 1.0, 1.0],
        },
        GPUVertex {
            position: [1.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
        },
        GPUVertex {
            position: [0.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
        },
    ];
    let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];

    let vbo = create_buffer_with_data(
        device,
        bytemuck::cast_slice(&vertices),
        wgpu::BufferUsage::VERTEX,
        create_debug_label!("Quad vbo"),
    );

    let ibo = create_buffer_with_data(
        device,
        bytemuck::cast_slice(&indices),
        wgpu::BufferUsage::INDEX,
        create_debug_label!("Quad ibo"),
    );

    let tex_transforms = create_buffer_with_data(
        device,
        bytemuck::cast_slice(&[TextureTransforms {
            u_matrix: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }]),
        wgpu::BufferUsage::UNIFORM,
        create_debug_label!("Quad tex transforms"),
    );

    (vbo, ibo, tex_transforms)
}

/// Converts a gradient to the uniforms used by the shader.
fn swf_gradient_to_uniforms(
    gradient_type: i32,
    gradient: &swf::Gradient,
    focal_point: f32,
) -> GradientUniforms {
    let mut colors: [[f32; 4]; 16] = Default::default();
    let mut ratios: [f32; 16] = Default::default();
    for (i, record) in gradient.records.iter().enumerate() {
        if i >= 16 {
            // TODO: we need to support these!
            break;
        }
        colors[i] = [
            f32::from(record.color.r) / 255.0,
            f32::from(record.color.g) / 255.0,
            f32::from(record.color.b) / 255.0,
            f32::from(record.color.a) / 255.0,
        ];
        ratios[i] = f32::from(record.ratio) / 255.0;
    }

    // Convert colors from sRGB to linear space if necessary.
    if gradient.interpolation == GradientInterpolation::LinearRGB {
        for color in &mut colors[0..gradient.records.len()] {
            *color = srgb_to_linear(*color);
        }
    }

    GradientUniforms {
        gradient_type,
        ratios,
        colors,
        interpolation: (gradient.interpolation == GradientInterpolation::LinearRGB) as i32,
        num_colors: gradient.records.len() as u32,
        repeat_mode: gradient_spread_mode_index(gradient.spread),
        focal_point,
    }
}

#[derive(Debug)]
struct Texture {
    width: u32,
    height: u32,
    texture: wgpu::Texture,
}

struct RuffleVertexCtor {
    color: [f32; 4],
}

impl FillVertexConstructor<GPUVertex> for RuffleVertexCtor {
    fn new_vertex(&mut self, position: lyon::math::Point, _: FillAttributes) -> GPUVertex {
        GPUVertex {
            position: [position.x, position.y],
            color: self.color,
        }
    }
}

impl StrokeVertexConstructor<GPUVertex> for RuffleVertexCtor {
    fn new_vertex(&mut self, position: lyon::math::Point, _: StrokeAttributes) -> GPUVertex {
        GPUVertex {
            position: [position.x, position.y],
            color: self.color,
        }
    }
}
