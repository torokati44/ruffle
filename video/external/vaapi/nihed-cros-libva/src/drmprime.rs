/// Kernel DRM buffer memory type.
pub const VA_SURFACE_ATTRIB_MEM_TYPE_KERNEL_DRM: u32 =      0x10000000;
/// DRM PRIME memory type (old version)
///
/// This supports only single objects with restricted memory layout.
/// Used with VASurfaceAttribExternalBuffers.
pub const VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME: u32 =        0x20000000;
/// DRM PRIME memory type
///
/// Used with VADRMPRIMESurfaceDescriptor.
pub const VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2: u32 =      0x40000000;
/// DRM PRIME3 memory type
///
/// Used with VADRMPRIME3SurfaceDescriptor.
pub const VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_3: u32 =      0x08000000;

/// Surface object description.
#[derive(Debug,Clone,Copy,Default)]
#[repr(C)]
pub struct VADRMPrimeObject {
    /// DRM PRIME file descriptor for this object.
    pub fd: i32,
    /// Total size of this object (may include regions which are not part of the surface).
    pub size: u32,
    /// Format modifier applied to this object.
    pub drm_format_modifier: u64,
}

/// Surface layer description.
#[derive(Debug,Clone,Copy,Default)]
#[repr(C)]
pub struct VADRMPrimeLayer {
    /// DRM format fourcc of this layer (DRM_FOURCC_*).
    pub drm_format: u32,
    /// Number of planes in this layer.
    pub num_planes: u32,
    /// Index in the objects array of the object containing each plane.
    pub object_index: [u32; 4],
    /// Offset within the object of each plane.
    pub offset: [u32; 4],
    /// Pitch of each plane.
    pub pitch: [u32; 4],
}

/**
 * External buffer descriptor for a DRM PRIME surface.
 *
 * For export, call vaExportSurfaceHandle() with mem_type set to
 * VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2 and pass a pointer to an
 * instance of this structure to fill.
 * If VA_EXPORT_SURFACE_SEPARATE_LAYERS is specified on export, each
 * layer will contain exactly one plane.  For example, an NV12
 * surface will be exported as two layers, one of DRM_FORMAT_R8 and
 * one of DRM_FORMAT_GR88.
 * If VA_EXPORT_SURFACE_COMPOSED_LAYERS is specified on export,
 * there will be exactly one layer.
 *
 * For import, call vaCreateSurfaces() with the MemoryType attribute
 * set to VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2 and the
 * ExternalBufferDescriptor attribute set to point to an array of
 * num_surfaces instances of this structure.
 * The number of planes which need to be provided for a given layer
 * is dependent on both the format and the format modifier used for
 * the objects containing it.  For example, the format DRM_FORMAT_RGBA
 * normally requires one plane, but with the format modifier
 * I915_FORMAT_MOD_Y_TILED_CCS it requires two planes - the first
 * being the main data plane and the second containing the color
 * control surface.
 * Note that a given driver may only support a subset of possible
 * representations of a particular format.  For example, it may only
 * support NV12 surfaces when they are contained within a single DRM
 * object, and therefore fail to create such surfaces if the two
 * planes are in different DRM objects.
 * Note that backend driver will retrieve the resource represent by fd,
 * and a valid surface ID is generated. Backend driver will not close
 * the file descriptor. Application should handle the release of the fd.
 * releasing the fd will not impact the existence of the surface.
 */
#[derive(Debug,Clone,Copy,Default)]
#[repr(C)]
pub struct VADRMPRIMESurfaceDescriptor {
    /// Pixel format fourcc of the whole surface (VA_FOURCC_*).
    pub fourcc: u32,
    /// Width of the surface in pixels.
    pub width: u32,
    /// Height of the surface in pixels.
    pub height: u32,
    /// Number of distinct DRM objects making up the surface.
    pub num_objects: u32,
    /// Description of each object.
    pub objects: [VADRMPrimeObject; 4],
    /// Number of layers making up the surface.
    pub num_layers: u32,
    /// Description of each layer in the surface.
    pub layers: [VADRMPrimeLayer; 4],
}

/**
 * External buffer descriptor for a DRM PRIME surface with flags
 *
 * This structure is an extention for VADRMPRIMESurfaceDescriptor,
 * it has the same behavior as if used with VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2.
 *
 * The field "flags" is added, see "Surface external buffer descriptor flags".
 * To use this structure, use VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_3 instead.
 */
#[derive(Debug,Clone,Copy,Default)]
#[repr(C)]
pub struct VADRMPRIME3SurfaceDescriptor {
    /// Pixel format fourcc of the whole surface (VA_FOURCC_*).
    pub fourcc: u32,
    /// Width of the surface in pixels.
    pub width: u32,
    /// Height of the surface in pixels.
    pub height: u32,
    /// Number of distinct DRM objects making up the surface.
    pub num_objects: u32,
    /// Description of each object.
    pub objects: [VADRMPrimeObject; 4],
    /// Number of layers making up the surface.
    pub num_layers: u32,
    /// Description of each layer in the surface.
    pub layers: [VADRMPrimeLayer; 4],
    /// flags. See "Surface external buffer descriptor flags".
    pub flags: u32,
    /// reserved bytes, must be zero
    reserved: [u32; 8 - 1],
}
