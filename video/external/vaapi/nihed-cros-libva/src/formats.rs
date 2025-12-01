//! Provides Rust wrappers for common enumerations used throughout the library.

use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign};

use crate::bindings;

/// Colourspace parameters.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RTFormat {
    /// YUV 4:2:0 8-bit.
    YUV420          = bindings::constants::VA_RT_FORMAT_YUV420,
    /// YUV 4:2:2 8-bit.
    YUV422          = bindings::constants::VA_RT_FORMAT_YUV422,
    /// YUV 4:4:4 8-bit.
    YUV444          = bindings::constants::VA_RT_FORMAT_YUV444,
    /// YUV 4:1:1 8-bit.
    YUV411          = bindings::constants::VA_RT_FORMAT_YUV411,
    /// Greyscale 8-bit.
    YUV400          = bindings::constants::VA_RT_FORMAT_YUV400,
    /// YUV 4:2:0 10-bit.
    YUV420_10       = bindings::constants::VA_RT_FORMAT_YUV420_10,
    /// YUV 4:2:2 10-bit.
    YUV422_10       = bindings::constants::VA_RT_FORMAT_YUV422_10,
    /// YUV 4:4:4 10-bit.
    YUV444_10       = bindings::constants::VA_RT_FORMAT_YUV444_10,
    /// YUV 4:2:0 12-bit.
    YUV420_12       = bindings::constants::VA_RT_FORMAT_YUV420_12,
    /// YUV 4:2:2 12-bit.
    YUV422_12       = bindings::constants::VA_RT_FORMAT_YUV422_12,
    /// YUV 4:4:4 12-bit.
    YUV444_12       = bindings::constants::VA_RT_FORMAT_YUV444_12,
    /// Packed RGB, 16 bits per pixel.
    RGB16           = bindings::constants::VA_RT_FORMAT_RGB16,
    /// Packed RGB, 32 bits per pixel, 8 bits per colour sample.
    RGB32           = bindings::constants::VA_RT_FORMAT_RGB32,
    /// Planar RGB, 8 bits per sample.
    RGBP            = bindings::constants::VA_RT_FORMAT_RGBP,
    /// Packed RGB, 32 bits per pixel, 10 bits per colour sample.
    RGB32_10        = bindings::constants::VA_RT_FORMAT_RGB32_10,
    /// Protected or unknown format.
    Protected       = bindings::constants::VA_RT_FORMAT_PROTECTED
}

impl From<RTFormat> for u32 {
    fn from(val: RTFormat) -> u32 {
        val as u32
    }
}

impl From<u32> for RTFormat {
    fn from(val: u32) -> RTFormat {
        match val {
            bindings::constants::VA_RT_FORMAT_YUV420 => RTFormat::YUV420,
            bindings::constants::VA_RT_FORMAT_YUV422 => RTFormat::YUV422,
            bindings::constants::VA_RT_FORMAT_YUV444 => RTFormat::YUV444,
            bindings::constants::VA_RT_FORMAT_YUV411 => RTFormat::YUV411,
            bindings::constants::VA_RT_FORMAT_YUV400 => RTFormat::YUV400,
            bindings::constants::VA_RT_FORMAT_YUV420_10 => RTFormat::YUV420_10,
            bindings::constants::VA_RT_FORMAT_YUV422_10 => RTFormat::YUV422_10,
            bindings::constants::VA_RT_FORMAT_YUV444_10 => RTFormat::YUV444_10,
            bindings::constants::VA_RT_FORMAT_YUV420_12 => RTFormat::YUV420_12,
            bindings::constants::VA_RT_FORMAT_YUV422_12 => RTFormat::YUV422_12,
            bindings::constants::VA_RT_FORMAT_YUV444_12 => RTFormat::YUV444_12,
            bindings::constants::VA_RT_FORMAT_RGB16 => RTFormat::RGB16,
            bindings::constants::VA_RT_FORMAT_RGB32 => RTFormat::RGB32,
            bindings::constants::VA_RT_FORMAT_RGBP => RTFormat::RGBP,
            bindings::constants::VA_RT_FORMAT_RGB32_10 => RTFormat::RGB32_10,
            bindings::constants::VA_RT_FORMAT_PROTECTED => RTFormat::Protected,
            _ => RTFormat::Protected,
        }
    }
}

impl std::fmt::Display for RTFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Recognized FOURCCs.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(non_camel_case_types)]
pub enum VAFourcc {
    /// NV12: two-plane 8-bit YUV 4:2:0.
    /// The first plane contains Y, the second plane contains U and V in pairs of bytes.
    NV12 = bindings::constants::VA_FOURCC_NV12,
    /// NV21: two-plane 8-bit YUV 4:2:0.
    /// Same as NV12, but with U and V swapped.
    NV21 = bindings::constants::VA_FOURCC_NV21,
    /// AI44: packed 4-bit YA.
    ///
    /// The bottom half of each byte contains luma, the top half contains alpha.
    A144 = bindings::constants::VA_FOURCC_AI44,
    /// RGBA: packed 8-bit RGBA.
    ///
    // Four bytes per pixel: red, green, blue, alpha.
    RGBA = bindings::constants::VA_FOURCC_RGBA,
    /// RGBX: packed 8-bit RGB.
    ///
    /// Four bytes per pixel: red, green, blue, unspecified.
    RGBX = bindings::constants::VA_FOURCC_RGBX,
    /// BGRA: packed 8-bit RGBA.
    ///
    /// Four bytes per pixel: blue, green, red, alpha.
    BGRA = bindings::constants::VA_FOURCC_BGRA,
    /// BGRX: packed 8-bit RGB.
    ///
    /// Four bytes per pixel: blue, green, red, unspecified.
    BGRX = bindings::constants::VA_FOURCC_BGRX,
    /// ARGB: packed 8-bit RGBA.
    ///
    /// Four bytes per pixel: alpha, red, green, blue.
    ARGB = bindings::constants::VA_FOURCC_ARGB,
    /// XRGB: packed 8-bit RGB.
    ///
    /// Four bytes per pixel: unspecified, red, green, blue.
    XRGB = bindings::constants::VA_FOURCC_XRGB,
    /// ABGR: packed 8-bit RGBA.
    ///
    /// Four bytes per pixel: alpha, blue, green, red.
    ABGR = bindings::constants::VA_FOURCC_ABGR,
    /// XBGR: packed 8-bit RGB.
    ///
    /// Four bytes per pixel: unspecified, blue, green, red.
    XBGR = bindings::constants::VA_FOURCC_XBGR,
    /// UYUV: packed 8-bit YUV 4:2:2.
    ///
    /// Four bytes per pair of pixels: U, Y, U, V.
    UYVY = bindings::constants::VA_FOURCC_UYVY,
    /// YUY2: packed 8-bit YUV 4:2:2.
    ///
    /// Four bytes per pair of pixels: Y, U, Y, V.
    YUY2 = bindings::constants::VA_FOURCC_YUY2,
    /// AYUV: packed 8-bit YUVA 4:4:4.
    ///
    /// Four bytes per pixel: A, Y, U, V.
    AYUV = bindings::constants::VA_FOURCC_AYUV,
    /// NV11: two-plane 8-bit YUV 4:1:1.
    ///
    /// The first plane contains Y, the second plane contains U and V in pairs of bytes.
    NV11 = bindings::constants::VA_FOURCC_NV11,
    /// YV12: three-plane 8-bit YUV 4:2:0.
    ///
    /// The three planes contain Y, V and U respectively.
    YV12 = bindings::constants::VA_FOURCC_YV12,
    /// P208: two-plane 8-bit YUV 4:2:2.
    ///
    /// The first plane contains Y, the second plane contains U and V in pairs of bytes.
    P208 = bindings::constants::VA_FOURCC_P208,
    /// I420: three-plane 8-bit YUV 4:2:0.
    ///
    /// The three planes contain Y, U and V respectively.
    I420 = bindings::constants::VA_FOURCC_I420,
    /// YV24: three-plane 8-bit YUV 4:4:4.
    ///
    /// The three planes contain Y, V and U respectively.
    YV24 = bindings::constants::VA_FOURCC_YV24,
    /// YV32: four-plane 8-bit YUVA 4:4:4
    ///
    /// The four planes contain Y, V, U and A respectively.
    YV32 = bindings::constants::VA_FOURCC_YV32,
    /// Y800: 8-bit greyscale.
    Y800 = bindings::constants::VA_FOURCC_Y800,
    /// IMC3: three-plane 8-bit YUV 4:2:0.
    ///
    /// Equivalent to YV12, but with the additional constraint that the pitch of all three planes
    /// must be the same.
    IMC3 = bindings::constants::VA_FOURCC_IMC3,
    /// 411P: three-plane 8-bit YUV 4:1:1.
    ///
    /// The three planes contain Y, U and V respectively.
    YUV_411P = bindings::constants::VA_FOURCC_411P,
    /// 411R: three-plane 8-bit YUV.
    ///
    /// The subsampling is the transpose of 4:1:1 - full chroma appears on every fourth line.
    /// The three planes contain Y, U and V respectively.
    YUV_411R = bindings::constants::VA_FOURCC_411R,
    /// 422H: three-plane 8-bit YUV 4:2:2.
    ///
    /// The three planes contain Y, U and V respectively.
    YUV_422H = bindings::constants::VA_FOURCC_422H,
    /// 422V: three-plane 8-bit YUV 4:4:0.
    ///
    /// The three planes contain Y, U and V respectively.
    YUV_422V = bindings::constants::VA_FOURCC_422V,
    /// 444P: three-plane 8-bit YUV 4:4:4.
    ///
    /// The three planes contain Y, U and V respectively.
    YUV_444P = bindings::constants::VA_FOURCC_444P,
    /// RGBP: three-plane 8-bit RGB.
    ///
    /// The three planes contain red, green and blue respectively.
    RGBP = bindings::constants::VA_FOURCC_RGBP,
    /// BGRP: three-plane 8-bit RGB.
    ///
    /// The three planes contain blue, green and red respectively.
    BGRP = bindings::constants::VA_FOURCC_BGRP,
    /// RG16: packed 5/6-bit RGB.
    ///
    /// Each pixel is a two-byte little-endian value.
    /// Red, green and blue are found in bits 15:11, 10:5, 4:0 respectively.
    RGB565 = bindings::constants::VA_FOURCC_RGB565,
    /// BG16: packed 5/6-bit RGB.
    ///
    /// Each pixel is a two-byte little-endian value.
    /// Blue, green and red are found in bits 15:11, 10:5, 4:0 respectively.
    BGR565 = bindings::constants::VA_FOURCC_BGR565,
    /// Y210: packed 10-bit YUV 4:2:2.
    ///
    /// Eight bytes represent a pair of pixels.  Each sample is a two-byte little-endian value,
    /// with the bottom six bits ignored.  The samples are in the order Y, U, Y, V.
    Y210 = bindings::constants::VA_FOURCC_Y210,
    /// Y212: packed 12-bit YUV 4:2:2.
    ///
    /// Eight bytes represent a pair of pixels.  Each sample is a two-byte little-endian value,
    /// with the bottom six bits ignored.  The samples are in the order Y, U, Y, V.
    Y212 = bindings::constants::VA_FOURCC_Y212,
    /// Y216: packed 16-bit YUV 4:2:2.
    ///
    /// Eight bytes represent a pair of pixels.  Each sample is a two-byte little-endian value.
    /// The samples are in the order Y, U, Y, V.
    Y216 = bindings::constants::VA_FOURCC_Y216,
    /// Y410: packed 10-bit YUVA 4:4:4.
    ///
    /// Each pixel is a six-byte little-endian value.
    /// A, V, Y, U are found in bits 31:30, 29:20, 19:10, 9:0 respectively.
    Y410 = bindings::constants::VA_FOURCC_Y410,
    /// Y412: packed 12-bit YUVA 4:4:4.
    ///
    ///  Each pixel is a set of four samples, each of which is a two-byte little-endian value.
    /// The samples are in the order A, V, Y, U.
    Y412 = bindings::constants::VA_FOURCC_Y412,
    /// Y416: packed 16-bit YUVA 4:4:4.
    ///
    /// Each pixel is a set of four samples, each of which is a two-byte little-endian value.
    /// The samples are in the order A, V, Y, U.
    Y416 = bindings::constants::VA_FOURCC_Y416,
    /// YV16: three-plane 8-bit YUV 4:2:2.
    ///
    /// The three planes contain Y, V and U respectively.
    YV16 = bindings::constants::VA_FOURCC_YV16,
    /// P010: two-plane 10-bit YUV 4:2:0.
    ///
    /// Each sample is a two-byte little-endian value with the bottom six bits ignored.
    /// The first plane contains Y, the second plane contains U and V in pairs of samples.
    P010 = bindings::constants::VA_FOURCC_P010,
    /// P012: two-plane 12-bit YUV 4:2:0.
    ///
    /// Each sample is a two-byte little-endian value.  The first plane contains Y, the second
    /// plane contains U and V in pairs of samples.
    P012 = bindings::constants::VA_FOURCC_P012,
    /// P016: two-plane 16-bit YUV 4:2:0.
    ///
    /// Each sample is a two-byte little-endian value.  The first plane contains Y, the second
    /// plane contains U and V in pairs of samples.
    P016 = bindings::constants::VA_FOURCC_P016,
    /// I010: three-plane 10-bit YUV 4:2:0.
    ///
    /// Each sample is a two-byte little-endian value with the top six bits ignored.
    /// The three planes contain Y, V and U respectively.
    I010 = bindings::constants::VA_FOURCC_I010,
    /// IYUV: three-plane 8-bit YUV 4:2:0.
    ///
    /// @deprecated Use I420 instead.
    IYUV = bindings::constants::VA_FOURCC_IYUV,
    /// 10-bit Pixel RGB formats.
    A2R10G10B10 = bindings::constants::VA_FOURCC_A2R10G10B10,
    /// 10-bit Pixel BGR formats.
    A2B10G10R10 = bindings::constants::VA_FOURCC_A2B10G10R10,
    /// 10-bit Pixel RGB formats without alpha.
    X2R10G10B10 = bindings::constants::VA_FOURCC_X2R10G10B10,
    /// 10-bit Pixel BGR formats without alpha.
    X2B10G10R10 = bindings::constants::VA_FOURCC_X2B10G10R10,
    /// Y8: 8-bit greyscale.
    ///
    /// Only a single sample, 8 bit Y plane for monochrome images
    Y8 = bindings::constants::VA_FOURCC_Y8,
    /// Y16: 16-bit greyscale.
    ///
    /// Only a single sample, 16 bit Y plane for monochrome images
    Y16 = bindings::constants::VA_FOURCC_Y16,
    /// VYUV: packed 8-bit YUV 4:2:2.
    ///
    /// Four bytes per pair of pixels: V, Y, U, V.
    VYUY = bindings::constants::VA_FOURCC_VYUY,
    /// YVYU: packed 8-bit YUV 4:2:2.
    ///
    /// Four bytes per pair of pixels: Y, V, Y, U.
    YVYU = bindings::constants::VA_FOURCC_YVYU,
    /// AGRB64: three-plane 16-bit ARGB 16:16:16:16
    ///
    /// The four planes contain: alpha, red, green, blue respectively.
    ARGB64 = bindings::constants::VA_FOURCC_ARGB64,
    /// ABGR64: three-plane 16-bit ABGR 16:16:16:16
    ///
    /// The four planes contain: alpha, blue, green, red respectively.
    ABGR64 = bindings::constants::VA_FOURCC_ABGR64,
    /// XYUV: packed 8-bit YUVX 4:4:4.
    ///
    /// Four bytes per pixel: X, Y, U, V.
    XYUV = bindings::constants::VA_FOURCC_XYUV,
}

impl From<VAFourcc> for u32 {
    fn from(val: VAFourcc) -> u32 {
        val as u32
    }
}

impl TryFrom<u32> for VAFourcc {
    type Error = ();
    fn try_from(val: u32) -> Result<Self, Self::Error> {
        match val {
            bindings::constants::VA_FOURCC_NV12 => Ok(VAFourcc::NV12),
            bindings::constants::VA_FOURCC_NV21 => Ok(VAFourcc::NV21),
            bindings::constants::VA_FOURCC_AI44 => Ok(VAFourcc::A144),
            bindings::constants::VA_FOURCC_RGBA => Ok(VAFourcc::RGBA),
            bindings::constants::VA_FOURCC_RGBX => Ok(VAFourcc::RGBX),
            bindings::constants::VA_FOURCC_BGRA => Ok(VAFourcc::BGRA),
            bindings::constants::VA_FOURCC_BGRX => Ok(VAFourcc::BGRX),
            bindings::constants::VA_FOURCC_ARGB => Ok(VAFourcc::ARGB),
            bindings::constants::VA_FOURCC_XRGB => Ok(VAFourcc::XRGB),
            bindings::constants::VA_FOURCC_ABGR => Ok(VAFourcc::ABGR),
            bindings::constants::VA_FOURCC_XBGR => Ok(VAFourcc::XBGR),
            bindings::constants::VA_FOURCC_UYVY => Ok(VAFourcc::UYVY),
            bindings::constants::VA_FOURCC_YUY2 => Ok(VAFourcc::YUY2),
            bindings::constants::VA_FOURCC_AYUV => Ok(VAFourcc::AYUV),
            bindings::constants::VA_FOURCC_NV11 => Ok(VAFourcc::NV11),
            bindings::constants::VA_FOURCC_YV12 => Ok(VAFourcc::YV12),
            bindings::constants::VA_FOURCC_P208 => Ok(VAFourcc::P208),
            bindings::constants::VA_FOURCC_I420 => Ok(VAFourcc::I420),
            bindings::constants::VA_FOURCC_YV24 => Ok(VAFourcc::YV24),
            bindings::constants::VA_FOURCC_YV32 => Ok(VAFourcc::YV32),
            bindings::constants::VA_FOURCC_Y800 => Ok(VAFourcc::Y800),
            bindings::constants::VA_FOURCC_IMC3 => Ok(VAFourcc::IMC3),
            bindings::constants::VA_FOURCC_411P => Ok(VAFourcc::YUV_411P),
            bindings::constants::VA_FOURCC_411R => Ok(VAFourcc::YUV_411R),
            bindings::constants::VA_FOURCC_422H => Ok(VAFourcc::YUV_422H),
            bindings::constants::VA_FOURCC_422V => Ok(VAFourcc::YUV_422V),
            bindings::constants::VA_FOURCC_444P => Ok(VAFourcc::YUV_444P),
            bindings::constants::VA_FOURCC_RGBP => Ok(VAFourcc::RGBP),
            bindings::constants::VA_FOURCC_BGRP => Ok(VAFourcc::BGRP),
            bindings::constants::VA_FOURCC_RGB565 => Ok(VAFourcc::RGB565),
            bindings::constants::VA_FOURCC_BGR565 => Ok(VAFourcc::BGR565),
            bindings::constants::VA_FOURCC_Y210 => Ok(VAFourcc::Y210),
            bindings::constants::VA_FOURCC_Y212 => Ok(VAFourcc::Y212),
            bindings::constants::VA_FOURCC_Y216 => Ok(VAFourcc::Y216),
            bindings::constants::VA_FOURCC_Y410 => Ok(VAFourcc::Y410),
            bindings::constants::VA_FOURCC_Y412 => Ok(VAFourcc::Y412),
            bindings::constants::VA_FOURCC_Y416 => Ok(VAFourcc::Y416),
            bindings::constants::VA_FOURCC_YV16 => Ok(VAFourcc::YV16),
            bindings::constants::VA_FOURCC_P010 => Ok(VAFourcc::P010),
            bindings::constants::VA_FOURCC_P012 => Ok(VAFourcc::P012),
            bindings::constants::VA_FOURCC_P016 => Ok(VAFourcc::P016),
            bindings::constants::VA_FOURCC_I010 => Ok(VAFourcc::I010),
            bindings::constants::VA_FOURCC_IYUV => Ok(VAFourcc::IYUV),
            bindings::constants::VA_FOURCC_A2R10G10B10 => Ok(VAFourcc::A2R10G10B10),
            bindings::constants::VA_FOURCC_A2B10G10R10 => Ok(VAFourcc::A2B10G10R10),
            bindings::constants::VA_FOURCC_X2R10G10B10 => Ok(VAFourcc::X2R10G10B10),
            bindings::constants::VA_FOURCC_X2B10G10R10 => Ok(VAFourcc::X2B10G10R10),
            bindings::constants::VA_FOURCC_Y8 => Ok(VAFourcc::Y8),
            bindings::constants::VA_FOURCC_Y16 => Ok(VAFourcc::Y16),
            bindings::constants::VA_FOURCC_VYUY => Ok(VAFourcc::VYUY),
            bindings::constants::VA_FOURCC_YVYU => Ok(VAFourcc::YVYU),
            bindings::constants::VA_FOURCC_ARGB64 => Ok(VAFourcc::ARGB64),
            bindings::constants::VA_FOURCC_ABGR64 => Ok(VAFourcc::ABGR64),
            bindings::constants::VA_FOURCC_XYUV => Ok(VAFourcc::XYUV),
            _ => Err(()),
        }
    }
}

/// H.264 picture flags structure.
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct H264PictureFlags(u32);

impl H264PictureFlags {
    /// Returns picture flags value as 32-bit integer.
    pub fn bits(self) -> u32 {
        self.0
    }
}

impl From<H264PictureFlags> for u32 {
    fn from(val: H264PictureFlags) -> u32 {
        val.bits()
    }
}

impl BitOr<H264PictureFlag> for H264PictureFlags {
    type Output = Self;

    fn bitor(self, rhs: H264PictureFlag) -> Self {
        Self(self.0 | rhs.bits())
    }
}

impl BitOrAssign<H264PictureFlag> for H264PictureFlags {
    fn bitor_assign(&mut self, rhs: H264PictureFlag) {
        self.0 |= rhs.bits();
    }
}

impl BitXor<H264PictureFlag> for H264PictureFlags {
    type Output = Self;

    fn bitxor(self, rhs: H264PictureFlag) -> Self {
        Self(self.0 ^ rhs.bits())
    }
}

impl BitXorAssign<H264PictureFlag> for H264PictureFlags {
    fn bitxor_assign(&mut self, rhs: H264PictureFlag) {
        self.0 ^= rhs.bits();
    }
}

impl BitAnd<H264PictureFlag> for H264PictureFlags {
    type Output = Self;

    fn bitand(self, rhs: H264PictureFlag) -> Self {
        Self(self.0 & rhs.bits())
    }
}

impl BitAndAssign<H264PictureFlag> for H264PictureFlags {
    fn bitand_assign(&mut self, rhs: H264PictureFlag) {
        self.0 &= rhs.bits();
    }
}

/// H.264 picture property flags.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum H264PictureFlag {
    /// Picture is invalid.
    Invalid             = bindings::constants::VA_PICTURE_H264_INVALID,
    /// Top field present.
    TopField            = bindings::constants::VA_PICTURE_H264_TOP_FIELD,
    /// Bottom field present.
    BottomField         = bindings::constants::VA_PICTURE_H264_BOTTOM_FIELD,
    /// Picture is used as a short-term reference.
    ShortTermReference  = bindings::constants::VA_PICTURE_H264_SHORT_TERM_REFERENCE,
    /// Picture is used as a short-term reference.
    LongTermReference  = bindings::constants::VA_PICTURE_H264_LONG_TERM_REFERENCE,
}

impl From<H264PictureFlag> for H264PictureFlags {
    fn from(val: H264PictureFlag) -> Self {
        Self(val.bits())
    }
}

impl From<H264PictureFlag> for u32 {
    fn from(val: H264PictureFlag) -> u32 {
        val.bits()
    }
}

impl H264PictureFlag {
    pub(crate) fn bits(self) -> u32 {
        self as u32
    }
}

/// Slice data contents type.
///
/// There will be cases where the bitstream buffer will not have enough room to hold
/// the data for the entire slice, and the following flags will be used in the slice
/// parameter to signal to the server for the possible cases.
/// If a slice parameter buffer and slice data buffer pair is sent to the server with
/// the slice data partially in the slice data buffer (BEGIN and MIDDLE cases below),
/// then a slice parameter and data buffer needs to be sent again to complete this slice.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VASliceDataFlag {
    /// Whole slice is in the buffer.
    All         = bindings::constants::VA_SLICE_DATA_FLAG_ALL,
    /// The beginning of the slice is in the buffer but the end if not.
    Begin       = bindings::constants::VA_SLICE_DATA_FLAG_BEGIN,
    /// Neither beginning nor end of the slice is in the buffer.
    Middle      = bindings::constants::VA_SLICE_DATA_FLAG_MIDDLE,
    /// End of the slice is in the buffer.
    End         = bindings::constants::VA_SLICE_DATA_FLAG_END,
}
