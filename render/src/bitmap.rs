use gc_arena::Collect;
use h263_rs_yuv::bt601::yuv420_to_rgba;
use std::fmt::Debug;
use std::sync::Arc;

use downcast_rs::{impl_downcast, Downcast};

use crate::backend::RenderBackend;

#[derive(Clone, Debug, Collect)]
#[collect(require_static)]
pub struct BitmapHandle(pub Arc<dyn BitmapHandleImpl>);

pub trait BitmapHandleImpl: Downcast + Debug {}
impl_downcast!(BitmapHandleImpl);

/// Info returned by the `register_bitmap` methods.
#[derive(Clone, Debug, Collect)]
#[collect(require_static)]
pub struct BitmapInfo {
    pub handle: BitmapHandle,
    pub width: u16,
    pub height: u16,
}

#[derive(Copy, Clone, Debug)]
pub struct BitmapSize {
    pub width: u16,
    pub height: u16,
}

/// An object that returns a bitmap given an ID.
///
/// This is used by render backends to get the bitmap used in a bitmap fill.
/// For movie libraries, this will return the bitmap with the given character ID.
pub trait BitmapSource {
    fn bitmap_size(&self, id: u16) -> Option<BitmapSize>;
    fn bitmap_handle(&self, id: u16, renderer: &mut dyn RenderBackend) -> Option<BitmapHandle>;
}

pub type RgbaBufRead<'a> = Box<dyn FnOnce(&[u8], u32) + 'a>;

pub trait SyncHandle: Downcast + Debug {
    /// Retrieves the rendered pixels from a previous `render_offscreen` call
    fn retrieve_offscreen_texture(
        self: Box<Self>,
        with_rgba: RgbaBufRead,
    ) -> Result<(), crate::error::Error>;
}
impl_downcast!(SyncHandle);

impl Clone for Box<dyn SyncHandle> {
    fn clone(&self) -> Box<dyn SyncHandle> {
        panic!("SyncHandle should have been consumed before clone() is called!")
    }
}

/// Decoded bitmap data from an SWF tag.
#[derive(Clone, Debug, Collect)]
#[collect(require_static)]
pub struct Bitmap {
    width: u32,
    height: u32,
    format: BitmapFormat,
    data: Vec<u8>,
}

impl Bitmap {
    /// Ensures that `data` is the correct size for the given `width` and `height`.
    pub fn new(width: u32, height: u32, format: BitmapFormat, mut data: Vec<u8>) -> Self {
        // If the size is incorrect, either we screwed up or the decoder screwed up.
        let expected_len = format.length_for_size(width as usize, height as usize);
        if data.len() != expected_len {
            tracing::warn!(
                "Incorrect bitmap data size, expected {} bytes, got {}",
                data.len(),
                expected_len
            );
            // Truncate or zero pad to the expected size.
            data.resize(expected_len, 0);
        }
        Self {
            width,
            height,
            format,
            data,
        }
    }

    pub fn to_rgb(mut self) -> Self {
        // Converts this bitmap to RGB, if it is not already.
        match self.format {
            BitmapFormat::Rgb => {} // no-op
            BitmapFormat::Rgba => unreachable!("Can't convert RGBA Bitmap to RGB"),
            BitmapFormat::Yuv420p => {
                let luma_len = (self.width * self.height) as usize;
                let chroma_len = (self.chroma_width() * self.chroma_height()) as usize;

                let y = &self.data[0..luma_len];
                let u = &self.data[luma_len..luma_len + chroma_len];
                let v = &self.data[luma_len + chroma_len..luma_len + 2 * chroma_len];

                self.data = yuv420_to_rgba(y, u, v, self.width as usize)
                    .chunks_exact(4)
                    .flat_map(|rgba| [rgba[0], rgba[1], rgba[2]])
                    .collect();
            }
            BitmapFormat::Yuva420p => unreachable!("Can't convert YUVA Bitmap to RGB"),
        }

        self.format = BitmapFormat::Rgb;

        self
    }

    pub fn to_rgba(mut self) -> Self {
        // Converts this bitmap to RGBA, if it is not already.
        match self.format {
            BitmapFormat::Rgb => {
                self.data = self
                    .data
                    .chunks_exact(3)
                    .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
                    .collect();
            }
            BitmapFormat::Rgba => {} // no-op
            BitmapFormat::Yuv420p => {
                let luma_len = (self.width * self.height) as usize;
                let chroma_len = (self.chroma_width() * self.chroma_height()) as usize;

                let y = &self.data[0..luma_len];
                let u = &self.data[luma_len..luma_len + chroma_len];
                let v = &self.data[luma_len + chroma_len..luma_len + 2 * chroma_len];

                self.data = yuv420_to_rgba(y, u, v, self.width as usize);
            }
            BitmapFormat::Yuva420p => {
                let luma_len = (self.width * self.height) as usize;
                let chroma_len = (self.chroma_width() * self.chroma_height()) as usize;

                let y = &self.data[0..luma_len];
                let u = &self.data[luma_len..luma_len + chroma_len];
                let v = &self.data[luma_len + chroma_len..luma_len + chroma_len + chroma_len];
                let a = &self.data[luma_len + 2 * chroma_len..2 * luma_len + 2 * chroma_len];

                let rgba = yuv420_to_rgba(y, u, v, self.width as usize);

                // RGB components need to be clamped to alpha to avoid invalid premultiplied colors
                self.data = rgba
                    .chunks_exact(4)
                    .zip(a)
                    .flat_map(|(rgba, a)| [rgba[0].min(*a), rgba[1].min(*a), rgba[2].min(*a), *a])
                    .collect()
            }
        }

        self.format = BitmapFormat::Rgba;

        self
    }

    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn chroma_width(&self) -> u32 {
        match self.format {
            BitmapFormat::Yuv420p | BitmapFormat::Yuva420p => (self.width + 1) / 2,
            _ => unreachable!("Can't get chroma width for non-YUV bitmap"),
        }
    }

    pub fn chroma_height(&self) -> u32 {
        match self.format {
            BitmapFormat::Yuv420p | BitmapFormat::Yuva420p => (self.height + 1) / 2,
            _ => unreachable!("Can't get chroma height for non-YUV bitmap"),
        }
    }

    #[inline]
    pub fn format(&self) -> BitmapFormat {
        self.format
    }

    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    #[inline]
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    pub fn as_colors(&self) -> impl Iterator<Item = i32> + '_ {
        let chunks = match self.format {
            BitmapFormat::Rgb => self.data.chunks_exact(3),
            BitmapFormat::Rgba => self.data.chunks_exact(4),
            _ => unimplemented!(
                "Can't iterate over non-RGB(A) bitmaps as colors, convert with `to_rgba` first"
            ),
        };
        chunks.map(|chunk| {
            let red = chunk[0];
            let green = chunk[1];
            let blue = chunk[2];
            let alpha = chunk.get(3).copied().unwrap_or(0xFF);
            i32::from_le_bytes([blue, green, red, alpha])
        })
    }
}

/// The pixel format of the bitmap data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BitmapFormat {
    /// 24-bit RGB.
    Rgb,

    /// 32-bit RGBA with premultiplied alpha.
    Rgba,

    /// planar YUV 420
    Yuv420p,

    /// planar YUV 420, premultiplied with alpha (RGB channels are to be clamped after conversion)
    Yuva420p,
}

impl BitmapFormat {
    #[inline]
    pub fn length_for_size(self, width: usize, height: usize) -> usize {
        match self {
            BitmapFormat::Rgb => width * height * 3,
            BitmapFormat::Rgba => width * height * 4,
            BitmapFormat::Yuv420p => width * height + ((width + 1) / 2) * ((height + 1) / 2) * 2,
            BitmapFormat::Yuva420p => {
                width * height * 2 + ((width + 1) / 2) * ((height + 1) / 2) * 2
            }
        }
    }
}
