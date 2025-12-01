// Copyright 2022 The ChromiumOS Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use crate::bindings;

/// Return status.
pub type VAResult<T> = Result<T, VAError>;

/// Non-successful return values of `VAStatus`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum VAError {
    /// Current operation has failed.
    OperationFailed             = bindings::constants::VA_STATUS_ERROR_OPERATION_FAILED,
    /// Allocation failed.
    AllocationFailed            = bindings::constants::VA_STATUS_ERROR_ALLOCATION_FAILED,
    /// Invalid display ID.
    InvalidDisplay              = bindings::constants::VA_STATUS_ERROR_INVALID_DISPLAY,
    /// Invalid configuration.
    InvalidConfig               = bindings::constants::VA_STATUS_ERROR_INVALID_CONFIG,
    /// Invalid context.
    InvalidContext              = bindings::constants::VA_STATUS_ERROR_INVALID_CONTEXT,
    /// Invalid surface ID.
    InvalidSurface              = bindings::constants::VA_STATUS_ERROR_INVALID_SURFACE,
    /// Invalid buffer.
    InvalidBuffer               = bindings::constants::VA_STATUS_ERROR_INVALID_BUFFER,
    /// Invalid image.
    InvalidImage                = bindings::constants::VA_STATUS_ERROR_INVALID_IMAGE,
    /// Invalid subpicture.
    InvalidSubpicture           = bindings::constants::VA_STATUS_ERROR_INVALID_SUBPICTURE,
    /// Requested attribute is not supported.
    AttrNotSupported            = bindings::constants::VA_STATUS_ERROR_ATTR_NOT_SUPPORTED,
    /// Maximum number of allowed elements has been exceeded.
    MaxNumExceeded              = bindings::constants::VA_STATUS_ERROR_MAX_NUM_EXCEEDED,
    /// Unsupported codec profile.
    UnsupportedProfile          = bindings::constants::VA_STATUS_ERROR_UNSUPPORTED_PROFILE,
    /// Unsupported entrypoint.
    UnsupportedEntrypoint       = bindings::constants::VA_STATUS_ERROR_UNSUPPORTED_ENTRYPOINT,
    /// Unsupported RT format.
    UnsupportedRTFormat         = bindings::constants::VA_STATUS_ERROR_UNSUPPORTED_RT_FORMAT,
    /// Unsupported buffer type.
    UnsupportedBuffertype       = bindings::constants::VA_STATUS_ERROR_UNSUPPORTED_BUFFERTYPE,
    /// Surface is still being worked on.
    SurfaceBusy                 = bindings::constants::VA_STATUS_ERROR_SURFACE_BUSY,
    /// Requested flag is not supported.
    FlagNotSupported            = bindings::constants::VA_STATUS_ERROR_FLAG_NOT_SUPPORTED,
    /// Invalid parameter.
    InvalidParameter            = bindings::constants::VA_STATUS_ERROR_INVALID_PARAMETER,
    /// Requested resolution is not supported.
    ResolutionNotSupported      = bindings::constants::VA_STATUS_ERROR_RESOLUTION_NOT_SUPPORTED,
    /// Unimplemented feature.
    Unimplemented               = bindings::constants::VA_STATUS_ERROR_UNIMPLEMENTED,
    /// Surface is still being displayed.
    SurfaceInDisplaying         = bindings::constants::VA_STATUS_ERROR_SURFACE_IN_DISPLAYING,
    /// Invalid image format.
    InvalidImageFormat          = bindings::constants::VA_STATUS_ERROR_INVALID_IMAGE_FORMAT,
    /// Generic decoding error.
    DecodingError               = bindings::constants::VA_STATUS_ERROR_DECODING_ERROR,
    /// Generic encoding error.
    EncodingError               = bindings::constants::VA_STATUS_ERROR_ENCODING_ERROR,
    /**
     * An invalid/unsupported value was supplied.
     *
     * This is a catch-all error code for invalid or unsupported values.
     * e.g. value exceeding the valid range, invalid type in the context
     * of generic attribute values.
     */
    InvalidValue                = bindings::constants::VA_STATUS_ERROR_INVALID_VALUE,
    /// An unsupported filter was supplied.
    UnsupportedFilter           = bindings::constants::VA_STATUS_ERROR_UNSUPPORTED_FILTER,
    /// An invalid filter chain was supplied.
    InvalidFilterChain          = bindings::constants::VA_STATUS_ERROR_INVALID_FILTER_CHAIN,
    /// Indicate HW busy (e.g. run multiple encoding simultaneously).
    HWBusy                      = bindings::constants::VA_STATUS_ERROR_HW_BUSY,
    /// An unsupported memory type was supplied.
    UnsupportedMemoryType       = bindings::constants::VA_STATUS_ERROR_UNSUPPORTED_MEMORY_TYPE,
    /// Indicate allocated buffer size is not enough for input or output
    NotEnoughBuffer             = bindings::constants::VA_STATUS_ERROR_NOT_ENOUGH_BUFFER,
    /// Operation has timed out.
    Timedout                    = bindings::constants::VA_STATUS_ERROR_TIMEDOUT,
    /// Unknown error.
    Unknown                     = bindings::constants::VA_STATUS_ERROR_UNKNOWN,
}

impl std::fmt::Display for VAError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub(crate) trait ConvertStatus {
    fn check(self) -> VAResult<()>;
}

impl ConvertStatus for bindings::VAStatus {
    fn check(self) -> VAResult<()> {
        match self as u32 {
            bindings::constants::VA_STATUS_SUCCESS => Ok(()),
            bindings::constants::VA_STATUS_ERROR_OPERATION_FAILED => Err(VAError::OperationFailed),
            bindings::constants::VA_STATUS_ERROR_ALLOCATION_FAILED => Err(VAError::AllocationFailed),
            bindings::constants::VA_STATUS_ERROR_INVALID_DISPLAY => Err(VAError::InvalidDisplay),
            bindings::constants::VA_STATUS_ERROR_INVALID_CONFIG => Err(VAError::InvalidConfig),
            bindings::constants::VA_STATUS_ERROR_INVALID_CONTEXT => Err(VAError::InvalidContext),
            bindings::constants::VA_STATUS_ERROR_INVALID_SURFACE => Err(VAError::InvalidSurface),
            bindings::constants::VA_STATUS_ERROR_INVALID_BUFFER => Err(VAError::InvalidBuffer),
            bindings::constants::VA_STATUS_ERROR_INVALID_IMAGE => Err(VAError::InvalidImage),
            bindings::constants::VA_STATUS_ERROR_INVALID_SUBPICTURE => Err(VAError::InvalidSubpicture),
            bindings::constants::VA_STATUS_ERROR_ATTR_NOT_SUPPORTED => Err(VAError::AttrNotSupported),
            bindings::constants::VA_STATUS_ERROR_MAX_NUM_EXCEEDED => Err(VAError::MaxNumExceeded),
            bindings::constants::VA_STATUS_ERROR_UNSUPPORTED_PROFILE => Err(VAError::UnsupportedProfile),
            bindings::constants::VA_STATUS_ERROR_UNSUPPORTED_ENTRYPOINT => Err(VAError::UnsupportedEntrypoint),
            bindings::constants::VA_STATUS_ERROR_UNSUPPORTED_RT_FORMAT => Err(VAError::UnsupportedRTFormat),
            bindings::constants::VA_STATUS_ERROR_UNSUPPORTED_BUFFERTYPE => Err(VAError::UnsupportedBuffertype),
            bindings::constants::VA_STATUS_ERROR_SURFACE_BUSY => Err(VAError::SurfaceBusy),
            bindings::constants::VA_STATUS_ERROR_FLAG_NOT_SUPPORTED => Err(VAError::FlagNotSupported),
            bindings::constants::VA_STATUS_ERROR_INVALID_PARAMETER => Err(VAError::InvalidParameter),
            bindings::constants::VA_STATUS_ERROR_RESOLUTION_NOT_SUPPORTED => Err(VAError::ResolutionNotSupported),
            bindings::constants::VA_STATUS_ERROR_UNIMPLEMENTED => Err(VAError::Unimplemented),
            bindings::constants::VA_STATUS_ERROR_SURFACE_IN_DISPLAYING => Err(VAError::SurfaceInDisplaying),
            bindings::constants::VA_STATUS_ERROR_INVALID_IMAGE_FORMAT => Err(VAError::InvalidImageFormat),
            bindings::constants::VA_STATUS_ERROR_DECODING_ERROR => Err(VAError::DecodingError),
            bindings::constants::VA_STATUS_ERROR_ENCODING_ERROR => Err(VAError::EncodingError),
            bindings::constants::VA_STATUS_ERROR_INVALID_VALUE => Err(VAError::InvalidValue),
            bindings::constants::VA_STATUS_ERROR_UNSUPPORTED_FILTER => Err(VAError::UnsupportedFilter),
            bindings::constants::VA_STATUS_ERROR_INVALID_FILTER_CHAIN => Err(VAError::InvalidFilterChain),
            bindings::constants::VA_STATUS_ERROR_HW_BUSY => Err(VAError::HWBusy),
            bindings::constants::VA_STATUS_ERROR_UNSUPPORTED_MEMORY_TYPE => Err(VAError::UnsupportedMemoryType),
            bindings::constants::VA_STATUS_ERROR_NOT_ENOUGH_BUFFER => Err(VAError::NotEnoughBuffer),
            bindings::constants::VA_STATUS_ERROR_TIMEDOUT => Err(VAError::Timedout),
            _ => Err(VAError::Unknown),
        }
    }
}

/// Surface status.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VASurfaceStatus {
    /// Rendering in progress.
    Rendering = 1,
    /// Displaying in progress (not safe to render into it).
    ///
    /// This status is useful if surface is used as the source of an overlay.
    Displaying = 2,
    /// Not being rendered or displayed.
    Ready = 3,
    /// Indicate a skipped frame during encode.
    Skipped = 8,
}

impl std::fmt::Display for VASurfaceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
