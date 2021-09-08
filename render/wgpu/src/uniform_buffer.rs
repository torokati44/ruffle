use bytemuck::Pod;
use futures::{
    executor::{LocalPool, LocalSpawner},
    task::LocalSpawnExt,
};
use std::{convert::TryInto, marker::PhantomData, mem};
use wgpu::util::StagingBelt;

/// A simple chunked bump allacator for managing dynamic uniforms that change per-draw.
/// Each draw call may use `UniformBuffer::write_uniforms` can be used to queue
/// the upload of uniform data to the GPU.
pub struct UniformBuffer<T: Pod> {
    blocks: Vec<Block>,
    buffer_layout: wgpu::BindGroupLayout,
    staging_belt: StagingBelt,
    executor: LocalPool,
    spawner: LocalSpawner,
    cur_block: usize,
    cur_offset: u32,
    _phantom: PhantomData<T>,
}

impl<T: Pod> UniformBuffer<T> {
    /// The size of each block.
    /// Uniforms are copied into each block until it reaches capacity, at which point a new
    /// block will be allocated.
    const BLOCK_SIZE: u32 = 65536;

    /// The uniform data size for a single draw call.
    const UNIFORMS_SIZE: u64 = mem::size_of::<T>() as u64;

    /// The aligned uniform data size required by wgpu.
    const ALIGNED_UNIFORMS_SIZE: u32 = {
        let align_mask = wgpu::BIND_BUFFER_ALIGNMENT - 1;
        let aligned_data_size = (Self::UNIFORMS_SIZE + align_mask) & !align_mask;
        aligned_data_size as u32
    };

    /// Creates a new `UniformBuffer` with the given uniform layout.
    pub fn new(buffer_layout: wgpu::BindGroupLayout) -> Self {
        let executor = LocalPool::new();
        let spawner = executor.spawner();
        Self {
            blocks: Vec::with_capacity(8),
            buffer_layout,
            executor,
            spawner,
            staging_belt: StagingBelt::new(u64::from(Self::BLOCK_SIZE) / 2),
            cur_block: 0,
            cur_offset: 0,
            _phantom: PhantomData,
        }
    }

    /// Returns the bind group layout for the uniforms in this buffer.
    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.buffer_layout
    }

    /// Resets the buffer and staging belt.
    /// Should be called at the start of a frame.
    pub fn reset(&mut self) {
        self.cur_block = 0;
        self.cur_offset = 0;
        let _ = self.spawner.spawn_local(self.staging_belt.recall());
        self.executor.run_until_stalled();
    }

    /// Enqueue `data` for upload into the given command encoder, and set the bind group on `render_pass`
    /// to use the uniform data.
    pub fn write_uniforms<'a>(
        &'a mut self,
        device: &wgpu::Device,
        command_encoder: &mut wgpu::CommandEncoder,
        render_pass: &mut wgpu::RenderPass<'a>,
        bind_group_index: u32,
        data: &T,
    ) {
        // Allocate a new block if we've exceeded our capacity.
        if self.cur_block >= self.blocks.len() {
            self.allocate_block(device);
        }
        let block = &self.blocks[self.cur_block];

        // Copy the data into the buffer via the staging belt.
        self.staging_belt
            .write_buffer(
                command_encoder,
                &block.buffer,
                self.cur_offset.into(),
                Self::UNIFORMS_SIZE.try_into().unwrap(),
                device,
            )
            .copy_from_slice(bytemuck::cast_slice(&std::slice::from_ref(data)));

        // Set the bind group to the final uniform location.
        render_pass.set_bind_group(bind_group_index, &block.bind_group, &[self.cur_offset]);

        // Advance offset.
        self.cur_offset += Self::ALIGNED_UNIFORMS_SIZE;
        // Advance to next buffer if we are out of room in this buffer.
        if Self::BLOCK_SIZE - self.cur_offset < Self::ALIGNED_UNIFORMS_SIZE {
            self.cur_block += 1;
            self.cur_offset = 0;
        }
    }

    /// Should be called at the end of a frame.
    pub fn finish(&mut self) {
        self.staging_belt.finish();
    }

    /// Adds a newly allocated buffer to the block list, and returns it.
    fn allocate_block(&mut self, device: &wgpu::Device) -> &Block {
        let buffer_label = create_debug_label!("Dynamic buffer");
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: buffer_label.as_deref(),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            size: Self::BLOCK_SIZE.into(),
            mapped_at_creation: false,
        });

        let bind_group_label = create_debug_label!("Dynamic buffer bind group");
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: bind_group_label.as_deref(),
            layout: &self.buffer_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<T>() as u64),
                }),
            }],
        });

        self.blocks.push(Block { buffer, bind_group });
        self.blocks.last().unwrap()
    }
}

/// A block of GPU memory that will contain our uniforms.
#[derive(Debug)]
struct Block {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}
