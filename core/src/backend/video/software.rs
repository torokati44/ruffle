//! Pure software video decoding backend.

use crate::backend::render::{BitmapHandle, BitmapInfo, RenderBackend};
use crate::backend::video::{
    EncodedFrame, Error, FrameDependency, VideoBackend, VideoStreamHandle,
};
use generational_arena::Arena;
use swf::{VideoCodec, VideoDeblocking};
use vp6_dec_rs::Vp6State;

/// A single preloaded video stream.
pub enum VideoStream {
    /// A VP6 video stream, with or without alpha channel.
    Vp6(Vp6State, Option<BitmapHandle>),
}

/// Software video backend that proxies to CPU-only codec implementations that
/// ship with Ruffle.
pub struct SoftwareVideoBackend {
    streams: Arena<VideoStream>,
}

impl Default for SoftwareVideoBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareVideoBackend {
    pub fn new() -> Self {
        Self {
            streams: Arena::new(),
        }
    }
}

impl VideoBackend for SoftwareVideoBackend {
    fn register_video_stream(
        &mut self,
        _num_frames: u32,
        size: (u16, u16),
        codec: VideoCodec,
        _filter: VideoDeblocking,
    ) -> Result<VideoStreamHandle, Error> {
        match codec {
            VideoCodec::Vp6 => Ok(self
                .streams
                .insert(VideoStream::Vp6(Vp6State::new(false, size), None))),
            VideoCodec::Vp6WithAlpha => Ok(self
                .streams
                .insert(VideoStream::Vp6(Vp6State::new(true, size), None))),
            _ => Err(format!("Unsupported video codec type {:?}", codec).into()),
        }
    }

    fn preload_video_stream_frame(
        &mut self,
        stream: VideoStreamHandle,
        encoded_frame: EncodedFrame<'_>,
    ) -> Result<FrameDependency, Error> {
        let stream = self
            .streams
            .get_mut(stream)
            .ok_or("Unregistered video stream")?;

        match stream {
            VideoStream::Vp6(state, _last_bitmap) => {
                // Luckily the very first bit of the encoded frames is exactly
                // this flag, so we don't have to bother asking any "proper"
                // decoder or parser.
                Ok(
                    if !encoded_frame.data.is_empty() && (encoded_frame.data[0] & 0b_1000_0000) == 0
                    {
                        // based on: https://wiki.multimedia.cx/index.php/On2_VP6
                        let marker = encoded_frame.data[0] & 0b_0000_0001;
                        let version2 = encoded_frame.data[1] & 0b_0000_0110;
                        let has_offset = marker == 1 || version2 == 0;

                        let macroblock_height = encoded_frame.data[ if has_offset { 4 } else { 2 } ];
                        let macroblock_width = encoded_frame.data[ if has_offset { 5 } else { 3 } ];

                        let coded_width = 16 * macroblock_width as u16;
                        let coded_height = 16 * macroblock_height as u16;

                        let dw = (state.bounds.0 as i16 - coded_width as i16) as i8;
                        let dh = (state.bounds.1 as i16 - coded_height as i16) as i8;

                        state.set_adjustment(dw, dh);

                        FrameDependency::None
                    } else {
                        FrameDependency::Past
                    },
                )
            }
        }
    }

    fn decode_video_stream_frame(
        &mut self,
        stream: VideoStreamHandle,
        encoded_frame: EncodedFrame<'_>,
        renderer: &mut dyn RenderBackend,
    ) -> Result<BitmapInfo, Error> {
        let stream = self
            .streams
            .get_mut(stream)
            .ok_or("Unregistered video stream")?;

        match stream {
            VideoStream::Vp6(state, last_bitmap) => {
                let (rgba, (width, height)) = state.decode(encoded_frame.data);
                println!("{} x {}", width, height);

                let handle = if let Some(lb) = last_bitmap {
                    renderer.update_texture(*lb, width as u32, height as u32, rgba)?
                } else {
                    renderer.register_bitmap_raw(width as u32, height as u32, rgba)?
                };

                *last_bitmap = Some(handle);

                Ok(BitmapInfo {
                    handle,
                    width: width as u16,
                    height: height as u16,
                })
            }
        }
    }
}
