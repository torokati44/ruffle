use crate::decoder::{LowDelay, VideoDecoder};
use ruffle_render::backend::RenderBackend;
use ruffle_render::bitmap::{BitmapHandle, BitmapInfo, PixelRegion};
use ruffle_video::VideoStreamHandle;
use ruffle_video::backend::VideoBackend;
use ruffle_video::error::Error;
use ruffle_video::frame::{DecodedFrame, DecodedFrameOut, EncodedFrame, FrameDependency};
use slotmap::SlotMap;
use swf::{VideoCodec, VideoDeblocking};

/// Software video backend that proxies to CPU-only codec implementations that
/// ship with Ruffle.
pub struct SoftwareVideoBackend {
    streams: SlotMap<VideoStreamHandle, VideoStream>,
}

impl Default for SoftwareVideoBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareVideoBackend {
    pub fn new() -> Self {
        Self {
            streams: SlotMap::with_key(),
        }
    }
}

impl VideoBackend for SoftwareVideoBackend {
    #[allow(unreachable_code, unused_variables)]
    fn register_video_stream(
        &mut self,
        _num_frames: u32,
        size: (u16, u16),
        codec: VideoCodec,
        filter: VideoDeblocking,
    ) -> Result<VideoStreamHandle, Error> {
        // None of these codecs use bidirectional prediction, so they all decode
        // straight through and only need adapting to the submit/poll interface.
        let decoder: Box<dyn VideoDecoder> = match codec {
            #[cfg(feature = "h263")]
            VideoCodec::H263 => Box::new(LowDelay::new(crate::decoder::h263::H263Decoder::new(
                filter,
            ))),
            #[cfg(feature = "vp6")]
            VideoCodec::Vp6 => Box::new(LowDelay::new(crate::decoder::vp6::Vp6Decoder::new(
                false, size,
            ))),
            #[cfg(feature = "vp6")]
            VideoCodec::Vp6WithAlpha => Box::new(LowDelay::new(
                crate::decoder::vp6::Vp6Decoder::new(true, size),
            )),
            #[cfg(feature = "screenvideo")]
            VideoCodec::ScreenVideo => Box::new(LowDelay::new(
                crate::decoder::screen::ScreenVideoDecoder::new(),
            )),
            other => return Err(Error::UnsupportedCodec(other)),
        };
        let stream = VideoStream::new(decoder);
        let stream_handle = self.streams.insert(stream);
        Ok(stream_handle)
    }

    fn preload_video_stream_frame(
        &mut self,
        stream: VideoStreamHandle,
        encoded_frame: EncodedFrame<'_>,
    ) -> Result<FrameDependency, Error> {
        let stream = self
            .streams
            .get_mut(stream)
            .ok_or(Error::VideoStreamIsNotRegistered)?;

        stream.decoder.preload_frame(encoded_frame)
    }

    fn configure_video_stream_decoder(
        &mut self,
        _stream: VideoStreamHandle,
        _configuration_data: &[u8],
    ) -> Result<(), Error> {
        // None of the software decoders require configuration.
        Ok(())
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
            .ok_or(Error::VideoStreamIsNotRegistered)?;

        let frame = stream.decode_frame(encoded_frame)?;

        let width = frame.width();
        let height = frame.height();

        let handle = if let Some(bitmap) = stream.bitmap.clone() {
            renderer.update_texture(&bitmap, frame, PixelRegion::for_whole_size(width, height))?;
            bitmap
        } else {
            renderer.register_bitmap(frame)?
        };
        stream.bitmap = Some(handle.clone());

        Ok(BitmapInfo {
            handle,
            width,
            height,
        })
    }
}

/// A single preloaded video stream.
pub struct VideoStream {
    bitmap: Option<BitmapHandle>,
    decoder: Box<dyn VideoDecoder>,
    /// Reused buffer for collecting decoder output.
    polled: Vec<DecodedFrameOut>,
}

impl VideoStream {
    fn new(decoder: Box<dyn VideoDecoder>) -> Self {
        Self {
            decoder,
            bitmap: None,
            polled: Vec::new(),
        }
    }

    /// Submit one frame and take the picture straight back out again.
    ///
    /// This holds only because every codec here decodes without delay; it goes
    /// away once callers drive submission and presentation separately.
    fn decode_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<DecodedFrame, Error> {
        self.decoder.submit_frame(encoded_frame)?;
        self.polled.clear();
        self.decoder.poll_frames(&mut self.polled)?;
        self.polled
            .pop()
            .map(|out| out.frame)
            .ok_or_else(|| Error::DecoderError("No output frame produced by the decoder".into()))
    }
}
