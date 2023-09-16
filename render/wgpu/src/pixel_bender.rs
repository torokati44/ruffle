use std::cell::RefCell;
use std::num::NonZeroU64;
use std::sync::OnceLock;
use std::{borrow::Cow, cell::Cell, sync::Arc};

use indexmap::IndexMap;
use ruffle_render::error::Error as BitmapError;
use ruffle_render::pixel_bender::{
    ImageInputTexture, PixelBenderShaderHandle, PixelBenderShaderImpl, PixelBenderType,
    OUT_COORD_NAME,
};
use ruffle_render::{
    bitmap::BitmapHandle,
    pixel_bender::{PixelBenderParam, PixelBenderShader, PixelBenderShaderArgument},
};
use wgpu::util::StagingBelt;
use wgpu::{
    BindGroupEntry, BindingResource, BlendComponent, BufferDescriptor, BufferUsages,
    ColorTargetState, ColorWrites, CommandEncoder, FrontFace, ImageCopyTexture, PipelineLayout,
    RenderPipeline, RenderPipelineDescriptor, SamplerBindingType, ShaderModuleDescriptor,
    TextureDescriptor, TextureFormat, TextureView, VertexState,
};

use crate::filters::{FilterSource, VERTEX_BUFFERS_DESCRIPTION_FILTERS};
use crate::raw_texture_as_texture;
use crate::utils::SampleCountMap;
use crate::{
    as_texture, backend::WgpuRenderBackend, descriptors::Descriptors, target::RenderTarget, Texture,
};

#[derive(Debug)]
pub struct PixelBenderWgpuShader {
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline_layout: PipelineLayout,
    pipelines: SampleCountMap<OnceLock<RenderPipeline>>,
    vertex_shader: wgpu::ShaderModule,
    fragment_shader: wgpu::ShaderModule,
    shader: PixelBenderShader,
    float_parameters_buffer: wgpu::Buffer,
    float_parameters_buffer_size: u64,
    int_parameters_buffer: wgpu::Buffer,
    int_parameters_buffer_size: u64,
    zeroed_out_of_range_mode: wgpu::Buffer,
    staging_belt: RefCell<StagingBelt>,
}

impl PixelBenderWgpuShader {
    /// Gets a `RenderPipeline` for the specified sample count
    fn get_pipeline(&self, descriptors: &Descriptors, samples: u32) -> &wgpu::RenderPipeline {
        self.pipelines.get_or_init(samples, || {
            descriptors
                .device
                .create_render_pipeline(&RenderPipelineDescriptor {
                    label: create_debug_label!("PixelBender shader pipeline").as_deref(),
                    layout: Some(&self.pipeline_layout),
                    vertex: VertexState {
                        module: &self.vertex_shader,
                        entry_point: naga_pixelbender::VERTEX_SHADER_ENTRYPOINT,
                        buffers: &VERTEX_BUFFERS_DESCRIPTION_FILTERS,
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &self.fragment_shader,
                        entry_point: naga_pixelbender::FRAGMENT_SHADER_ENTRYPOINT,
                        targets: &[Some(ColorTargetState {
                            format: TextureFormat::Rgba8Unorm,
                            // FIXME - what should this be?
                            blend: Some(wgpu::BlendState {
                                color: BlendComponent::OVER,
                                alpha: BlendComponent::OVER,
                            }),
                            write_mask: ColorWrites::all(),
                        })],
                    }),
                    primitive: wgpu::PrimitiveState {
                        front_face: FrontFace::Ccw,
                        cull_mode: None,
                        ..Default::default()
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState {
                        count: samples,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                    multiview: Default::default(),
                })
        })
    }
}

impl PixelBenderShaderImpl for PixelBenderWgpuShader {
    fn parsed_shader(&self) -> &PixelBenderShader {
        &self.shader
    }
}

pub fn as_cache_holder(handle: &PixelBenderShaderHandle) -> &PixelBenderWgpuShader {
    <dyn PixelBenderShaderImpl>::downcast_ref(&*handle.0).unwrap()
}

impl PixelBenderWgpuShader {
    pub fn new(descriptors: &Descriptors, shader: PixelBenderShader) -> PixelBenderWgpuShader {
        let mut layout_entries = vec![
            // One sampler per filter/wrapping combination - see BitmapFilters
            // An AGAL shader can use any of these samplers, so
            // we need to bind them all.
            wgpu::BindGroupLayoutEntry {
                binding: naga_pixelbender::SAMPLER_CLAMP_NEAREST,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: naga_pixelbender::SAMPLER_CLAMP_LINEAR,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: naga_pixelbender::SAMPLER_CLAMP_BILINEAR,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: naga_pixelbender::SHADER_FLOAT_PARAMETERS_INDEX,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: naga_pixelbender::SHADER_INT_PARAMETERS_INDEX,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: naga_pixelbender::ZEROED_OUT_OF_RANGE_MODE_INDEX,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ];

        for param in &shader.params {
            if let PixelBenderParam::Texture { index, .. } = param {
                let binding = naga_pixelbender::TEXTURE_START_BIND_INDEX + *index as u32;
                layout_entries.push(wgpu::BindGroupLayoutEntry {
                    binding,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                });
            }
        }

        let globals_layout_label =
            create_debug_label!("PixelBender bind group layout for {:?}", shader.name);
        let bind_group_layout =
            descriptors
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: globals_layout_label.as_deref(),
                    entries: &layout_entries,
                });

        let pipeline_layout_label =
            create_debug_label!("PixelBender pipeline layout for {:?}", shader.name);
        let pipeline_layout =
            descriptors
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: pipeline_layout_label.as_deref(),
                    bind_group_layouts: &[&bind_group_layout],
                    push_constant_ranges: &[],
                });

        let shaders =
            naga_pixelbender::ShaderBuilder::build(&shader).expect("Failed to compile shader");

        let float_label =
            create_debug_label!("PixelBender float parameters buffer for {:?}", shader.name);

        let float_parameters_buffer = descriptors.device.create_buffer(&BufferDescriptor {
            label: float_label.as_deref(),
            size: shaders.float_parameters_buffer_size,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let int_label =
            create_debug_label!("PixelBender int parameters buffer for {:?}", shader.name);

        let int_parameters_buffer = descriptors.device.create_buffer(&BufferDescriptor {
            label: int_label.as_deref(),
            size: shaders.int_parameters_buffer_size,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let zeroed_out_of_range_mode = descriptors.device.create_buffer(&BufferDescriptor {
            label: create_debug_label!("PixelBender zeroed_out_of_range_mode parameter buffer")
                .as_deref(),
            size: shaders.int_parameters_buffer_size,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let vertex_shader = descriptors
            .device
            .create_shader_module(ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Naga(Cow::Owned(shaders.vertex)),
            });

        let fragment_shader = descriptors
            .device
            .create_shader_module(ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Naga(Cow::Owned(shaders.fragment)),
            });

        PixelBenderWgpuShader {
            bind_group_layout,
            pipeline_layout,
            pipelines: Default::default(),
            shader,
            vertex_shader,
            fragment_shader,
            float_parameters_buffer,
            float_parameters_buffer_size: shaders.float_parameters_buffer_size,
            int_parameters_buffer,
            int_parameters_buffer_size: shaders.int_parameters_buffer_size,
            zeroed_out_of_range_mode,
            // FIXME - come up with a good chunk size
            staging_belt: RefCell::new(StagingBelt::new(8)),
        }
    }
}

fn image_input_as_texture<'a>(input: &'a ImageInputTexture<'a>) -> &wgpu::Texture {
    match input {
        ImageInputTexture::Bitmap(handle) => &as_texture(handle).texture,
        ImageInputTexture::TextureRef(raw_texture) => raw_texture_as_texture(*raw_texture),
    }
}

impl<T: RenderTarget> WgpuRenderBackend<T> {
    pub(super) fn compile_pixelbender_shader_impl(
        &mut self,
        shader: PixelBenderShader,
    ) -> Result<PixelBenderShaderHandle, BitmapError> {
        let handle = PixelBenderWgpuShader::new(&self.descriptors, shader);
        Ok(PixelBenderShaderHandle(Arc::new(handle)))
    }
}

pub enum ShaderMode {
    ShaderJob,
    Filter,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run_pixelbender_shader_impl(
    descriptors: &Descriptors,
    shader: PixelBenderShaderHandle,
    mode: ShaderMode,
    arguments: &[PixelBenderShaderArgument],
    target: &wgpu::Texture,
    render_command_encoder: &mut CommandEncoder,
    color_attachment: Option<wgpu::RenderPassColorAttachment>,
    sample_count: u32,
    // FIXME - do we cover the whole source or the whole dest?
    source: &FilterSource,
) -> Result<(), BitmapError> {
    let compiled_shader = &as_cache_holder(&shader);
    let mut staging_belt = compiled_shader.staging_belt.borrow_mut();

    let mut arguments = arguments.to_vec();

    let mut bind_group_entries = vec![
        BindGroupEntry {
            binding: naga_pixelbender::SAMPLER_CLAMP_NEAREST,
            resource: BindingResource::Sampler(&descriptors.bitmap_samplers.clamp_nearest),
        },
        BindGroupEntry {
            binding: naga_pixelbender::SAMPLER_CLAMP_LINEAR,
            resource: BindingResource::Sampler(&descriptors.bitmap_samplers.clamp_linear),
        },
        BindGroupEntry {
            binding: naga_pixelbender::SAMPLER_CLAMP_BILINEAR,
            // FIXME - create bilinear sampler
            resource: BindingResource::Sampler(&descriptors.bitmap_samplers.clamp_linear),
        },
        BindGroupEntry {
            binding: naga_pixelbender::SHADER_FLOAT_PARAMETERS_INDEX,
            resource: BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &compiled_shader.float_parameters_buffer,
                offset: 0,
                size: Some(NonZeroU64::new(compiled_shader.float_parameters_buffer_size).unwrap()),
            }),
        },
        BindGroupEntry {
            binding: naga_pixelbender::SHADER_INT_PARAMETERS_INDEX,
            resource: BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &compiled_shader.int_parameters_buffer,
                offset: 0,
                size: Some(NonZeroU64::new(compiled_shader.int_parameters_buffer_size).unwrap()),
            }),
        },
        BindGroupEntry {
            binding: naga_pixelbender::ZEROED_OUT_OF_RANGE_MODE_INDEX,
            resource: BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &compiled_shader.zeroed_out_of_range_mode,
                offset: 0,
                size: Some(NonZeroU64::new(std::mem::size_of::<f32>() as u64 * 4).unwrap()),
            }),
        },
    ];

    let mut zeroed_out_of_range_mode_slice = staging_belt.write_buffer(
        render_command_encoder,
        &compiled_shader.zeroed_out_of_range_mode,
        0,
        NonZeroU64::new(std::mem::size_of::<f32>() as u64 * 4).unwrap(),
        &descriptors.device,
    );

    // This would ideally be a single f32, but web requires at least 16 bytes
    zeroed_out_of_range_mode_slice.copy_from_slice(bytemuck::cast_slice(&[match mode {
        // When a Shader is run via a ShaderJob, out-of-range texture sample coordinates
        // seem to be clamped to the edge of the texture (despite what the docs describe)
        ShaderMode::ShaderJob => [0.0f32, 0.0f32, 0.0f32, 0.0f32],
        // When a Shader is run through a ShaderFilter, out-of-range texture sample coordinates
        // return transparent black (0.0, 0.0, 0.0, 0.0). This is easiest to observe with
        // BitmapData.applyFilter when the BitampData destination is larger than the source.
        ShaderMode::Filter => [1.0f32, 1.0f32, 1.0f32, 1.0f32],
    }]));
    drop(zeroed_out_of_range_mode_slice);

    let mut texture_views: IndexMap<u8, TextureView> = Default::default();

    let mut target_clone = None;

    let mut float_offset = 0;
    let mut int_offset = 0;

    let mut first_image = None;

    for input in &mut arguments {
        match input {
            PixelBenderShaderArgument::ImageInput { index, texture, .. } => {
                let input_texture = &image_input_as_texture(texture.as_ref().unwrap());
                if std::ptr::eq(*input_texture, target) {
                    // The input is the same as the output - we need to clone the input.
                    // We will write to the original output, and use a clone of the input as a texture input binding
                    let cached_fresh_handle = target_clone.get_or_insert_with(|| {
                        let extent = wgpu::Extent3d {
                            width: target.width(),
                            height: target.height(),
                            depth_or_array_layers: 1,
                        };
                        let fresh_texture = descriptors.device.create_texture(&TextureDescriptor {
                            label: Some("PixelBenderShader target clone"),
                            size: extent,
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            usage: wgpu::TextureUsages::COPY_DST
                                | wgpu::TextureUsages::TEXTURE_BINDING,
                            view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
                        });
                        render_command_encoder.copy_texture_to_texture(
                            ImageCopyTexture {
                                texture: target,
                                mip_level: 0,
                                origin: Default::default(),
                                aspect: Default::default(),
                            },
                            ImageCopyTexture {
                                texture: &fresh_texture,
                                mip_level: 0,
                                origin: Default::default(),
                                aspect: Default::default(),
                            },
                            extent,
                        );

                        BitmapHandle(Arc::new(Texture {
                            texture: Arc::new(fresh_texture),
                            bind_linear: Default::default(),
                            bind_nearest: Default::default(),
                            copy_count: Cell::new(0),
                        }))
                    });
                    *texture = Some(cached_fresh_handle.clone().into());
                }
                let wgpu_texture = image_input_as_texture(texture.as_ref().unwrap());
                texture_views.insert(
                    *index,
                    wgpu_texture.create_view(&wgpu::TextureViewDescriptor::default()),
                );
            }
            PixelBenderShaderArgument::ValueInput { index, value } => {
                let param = &compiled_shader.shader.params[*index as usize];

                let name = match param {
                    PixelBenderParam::Normal { name, .. } => name,
                    _ => unreachable!(),
                };

                if name == OUT_COORD_NAME {
                    continue;
                }

                #[derive(Debug)]
                enum FloatOrInt {
                    Float(Vec<f32>),
                    Int(Vec<i32>),
                }

                impl FloatOrInt {
                    fn len(&self) -> usize {
                        match self {
                            FloatOrInt::Float(v) => v.len(),
                            FloatOrInt::Int(v) => v.len(),
                        }
                    }
                }

                let value_vec = match value {
                    PixelBenderType::TFloat(f1) => FloatOrInt::Float(vec![*f1, 0.0, 0.0, 0.0]),
                    PixelBenderType::TFloat2(f1, f2) => FloatOrInt::Float(vec![*f1, *f2, 0.0, 0.0]),
                    PixelBenderType::TFloat3(f1, f2, f3) => {
                        FloatOrInt::Float(vec![*f1, *f2, *f3, 0.0])
                    }
                    PixelBenderType::TFloat4(f1, f2, f3, f4) => {
                        FloatOrInt::Float(vec![*f1, *f2, *f3, *f4])
                    }
                    PixelBenderType::TInt(i1) => FloatOrInt::Int(vec![*i1 as i32, 0, 0, 0]),
                    PixelBenderType::TInt2(i1, i2) => {
                        FloatOrInt::Int(vec![*i1 as i32, *i2 as i32, 0, 0])
                    }
                    PixelBenderType::TInt3(i1, i2, i3) => {
                        FloatOrInt::Int(vec![*i1 as i32, *i2 as i32, *i3 as i32, 0])
                    }
                    PixelBenderType::TInt4(i1, i2, i3, i4) => {
                        FloatOrInt::Int(vec![*i1 as i32, *i2 as i32, *i3 as i32, *i4 as i32])
                    }
                    // We treat the input as being in column-major order. Despite what the Flash docs claim,
                    // this seems to be what Flash Player does.
                    PixelBenderType::TFloat2x2(arr) => FloatOrInt::Float(arr.to_vec()),
                    PixelBenderType::TFloat3x3(arr) => {
                        // Add a zero after every 3 values to created zero-padded vec4s
                        let mut vec4_arr = Vec::with_capacity(16);
                        for (i, val) in arr.iter().enumerate() {
                            vec4_arr.push(*val);
                            if i % 3 == 2 {
                                vec4_arr.push(0.0);
                            }
                        }
                        FloatOrInt::Float(vec4_arr)
                    }
                    PixelBenderType::TFloat4x4(arr) => FloatOrInt::Float(arr.to_vec()),
                    _ => unreachable!("Unimplemented value {value:?}"),
                };

                assert_eq!(
                    value_vec.len() % 4,
                    0,
                    "value_vec should represent concatenated vec4fs"
                );
                let num_vec4s = value_vec.len() / 4;
                // Both float32 and int are 4 bytes
                let component_size_bytes = 4;

                let (buffer, vec4_count) = if matches!(value_vec, FloatOrInt::Float(_)) {
                    let res = (&compiled_shader.float_parameters_buffer, float_offset);
                    float_offset += num_vec4s;
                    res
                } else {
                    let res = (&compiled_shader.int_parameters_buffer, int_offset);
                    int_offset += num_vec4s;
                    res
                };

                let mut buffer_slice = staging_belt.write_buffer(
                    render_command_encoder,
                    buffer,
                    vec4_count as u64 * 4 * component_size_bytes,
                    NonZeroU64::new(value_vec.len() as u64 * component_size_bytes).unwrap(),
                    &descriptors.device,
                );
                match value_vec {
                    FloatOrInt::Float(v) => {
                        buffer_slice.copy_from_slice(bytemuck::cast_slice(&v));
                    }
                    FloatOrInt::Int(v) => {
                        buffer_slice.copy_from_slice(bytemuck::cast_slice(&v));
                    }
                }
            }
        }
    }

    // This needs to be a separate loop, so that we can get references into `texture_views`
    for input in &arguments {
        match input {
            PixelBenderShaderArgument::ImageInput { index, texture, .. } => {
                let wgpu_texture = image_input_as_texture(texture.as_ref().unwrap());

                if first_image.is_none() {
                    first_image = Some(wgpu_texture);
                }

                let binding = naga_pixelbender::TEXTURE_START_BIND_INDEX + *index as u32;
                bind_group_entries.push(BindGroupEntry {
                    binding,
                    resource: BindingResource::TextureView(&texture_views[index]),
                });
            }
            PixelBenderShaderArgument::ValueInput { .. } => {}
        }
    }

    let bind_group = descriptors
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &compiled_shader.bind_group_layout,
            entries: &bind_group_entries,
        });

    staging_belt.finish();

    let vertices = source.vertices(&descriptors.device);

    let pipeline = compiled_shader.get_pipeline(descriptors, sample_count);

    let mut render_pass = render_command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        occlusion_query_set: None,
        timestamp_writes: None,
        label: Some("PixelBender render pass"),
        color_attachments: &[color_attachment],
        depth_stencil_attachment: None,
    });
    render_pass.set_bind_group(0, &bind_group, &[]);
    render_pass.set_pipeline(pipeline);

    render_pass.set_vertex_buffer(0, vertices.slice(..));
    render_pass.set_index_buffer(
        descriptors.quad.indices.slice(..),
        wgpu::IndexFormat::Uint32,
    );

    render_pass.draw_indexed(0..6, 0, 0..1);

    // Note - we just drop the staging belt, instead of recalling it,
    // since we're not going to use it again.

    Ok(())
}
