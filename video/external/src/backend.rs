use crate::decoder::VideoDecoder;
use generational_arena::Arena;
use ruffle_render::backend::RenderBackend;
use ruffle_render::bitmap::{BitmapHandle, BitmapInfo, PixelRegion};
use ruffle_video::backend::VideoBackend;
use ruffle_video::error::Error;
use ruffle_video::frame::{EncodedFrame, FrameDependency};
use ruffle_video::VideoStreamHandle;
use ruffle_video_software::backend::SoftwareVideoBackend;
use swf::{VideoCodec, VideoDeblocking};

/// Software video backend that proxies to CPU-only codec implementations that
/// ship with Ruffle.
pub struct ExternalVideoBackend {
    streams: Arena<VideoStream>,
    software: SoftwareVideoBackend,
}

impl Default for ExternalVideoBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl ExternalVideoBackend {
    pub fn new() -> Self {
        Self {
            streams: Arena::new(),
            software: SoftwareVideoBackend::new(),
        }
    }
}

impl VideoBackend for ExternalVideoBackend {
    #[allow(unreachable_code, unused_variables)]
    fn register_video_stream(
        &mut self,
        num_frames: u32,
        size: (u16, u16),
        codec: VideoCodec,
        filter: VideoDeblocking,
    ) -> Result<VideoStreamHandle, Error> {
        println!("Registering video stream");

        // FIXME: since the main and the fallback (software) backends have independent
        // arenas, they could assign the same handles (`Index`es) to different streams.
        // Each index may refer to a H264, and a non-H264 stream, so it's no longer a
        // unique identifier. Ideas: shift the fallback handles by a large amount (hack),
        // put the stream into the fallback backend's arena (or share the arena with it).
        if codec == VideoCodec::H264 {
            let decoder = Box::new(crate::decoder::openh264::H264Decoder::new());
            let stream = VideoStream::new(decoder);
            let stream_handle = self.streams.insert(stream);
            Ok(stream_handle)
        } else {
            self.software
                .register_video_stream(num_frames, size, codec, filter)
        }
    }

    fn preload_video_stream_frame(
        &mut self,
        stream: VideoStreamHandle,
        encoded_frame: EncodedFrame<'_>,
    ) -> Result<FrameDependency, Error> {
        println!("Preloading video stream");
        if encoded_frame.codec == VideoCodec::H264 {
            let stream = self
                .streams
                .get_mut(stream)
                .ok_or(Error::VideoStreamIsNotRegistered)?;

            stream.decoder.preload_frame(encoded_frame)
        } else {
            self.software
                .preload_video_stream_frame(stream, encoded_frame)
        }
    }

    fn decode_video_stream_frame(
        &mut self,
        stream: VideoStreamHandle,
        encoded_frame: EncodedFrame<'_>,
        renderer: &mut dyn RenderBackend,
    ) -> Result<BitmapInfo, Error> {
        //println!("Decoding video frame");
        if encoded_frame.codec == VideoCodec::H264 {
            let stream = self
                .streams
                .get_mut(stream)
                .ok_or(Error::VideoStreamIsNotRegistered)?;

            let frame = stream.decoder.decode_frame(encoded_frame)?;

            let w = frame.width();
            let h = frame.height();

            let handle = if let Some(bitmap) = stream.bitmap.clone() {
                renderer.update_texture(&bitmap, frame, PixelRegion::for_whole_size(w, h))?;
                bitmap
            } else {
                renderer.register_bitmap(frame)?
            };
            stream.bitmap = Some(handle.clone());

            Ok(BitmapInfo {
                handle,
                width: w as u16,
                height: h as u16,
            })
        } else {
            self.software
                .decode_video_stream_frame(stream, encoded_frame, renderer)
        }
    }
}

/// A single preloaded video stream.
pub struct VideoStream {
    bitmap: Option<BitmapHandle>,
    decoder: Box<dyn VideoDecoder>,
}

impl VideoStream {
    fn new(decoder: Box<dyn VideoDecoder>) -> Self {
        Self {
            decoder,
            bitmap: None,
        }
    }
}
