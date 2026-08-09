use crate::decoder::{LowDelay, VideoDecoder};
use ruffle_render::backend::RenderBackend;
use ruffle_video::VideoStreamHandle;
use ruffle_video::backend::VideoBackend;
use ruffle_video::error::Error;
use ruffle_video::frame::{DecodedFrameOut, EncodedFrame, FrameDependency, PresentationTime};
use ruffle_video::queue::{Presentation, PresentationQueue};
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

    fn submit_video_stream_frame(
        &mut self,
        stream: VideoStreamHandle,
        encoded_frame: EncodedFrame<'_>,
        pts: PresentationTime,
    ) -> Result<(), Error> {
        self.streams
            .get_mut(stream)
            .ok_or(Error::VideoStreamIsNotRegistered)?
            .submit(encoded_frame, pts)
    }

    fn present_video_stream_frame(
        &mut self,
        stream: VideoStreamHandle,
        pts: PresentationTime,
        renderer: &mut dyn RenderBackend,
    ) -> Result<Presentation, Error> {
        self.streams
            .get_mut(stream)
            .ok_or(Error::VideoStreamIsNotRegistered)?
            .queue
            .present(pts, renderer)
    }

    fn flush_video_stream(&mut self, stream: VideoStreamHandle) -> Result<(), Error> {
        self.streams
            .get_mut(stream)
            .ok_or(Error::VideoStreamIsNotRegistered)?
            .flush()
    }

    fn video_stream_is_drained(&self, stream: VideoStreamHandle) -> bool {
        self.streams
            .get(stream)
            .is_none_or(|stream| stream.queue.is_drained())
    }

    fn reset_video_stream(&mut self, stream: VideoStreamHandle) -> Result<(), Error> {
        self.streams
            .get_mut(stream)
            .ok_or(Error::VideoStreamIsNotRegistered)?
            .reset()
    }
}

/// A single preloaded video stream.
pub struct VideoStream {
    decoder: Box<dyn VideoDecoder>,
    queue: PresentationQueue,
    /// Reused buffer for moving pictures out of the decoder and into the queue.
    polled: Vec<DecodedFrameOut>,
}

impl VideoStream {
    fn new(decoder: Box<dyn VideoDecoder>) -> Self {
        Self {
            decoder,
            queue: PresentationQueue::new(),
            polled: Vec::new(),
        }
    }

    fn submit(
        &mut self,
        encoded_frame: EncodedFrame<'_>,
        pts: PresentationTime,
    ) -> Result<(), Error> {
        let frame_id = encoded_frame.frame_id;
        self.decoder.submit_frame(encoded_frame)?;
        self.queue.submitted(frame_id, pts);
        self.pump()
    }

    /// Collect whatever the decoder has finished into the queue.
    fn pump(&mut self) -> Result<(), Error> {
        self.polled.clear();
        self.decoder.poll_frames(&mut self.polled)?;
        self.queue.absorb(&mut self.polled);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Error> {
        self.decoder.flush()?;
        self.pump()
    }

    fn reset(&mut self) -> Result<(), Error> {
        self.queue.reset();
        self.polled.clear();
        self.decoder.reset()
    }
}
