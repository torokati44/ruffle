#[cfg(feature = "webcodecs")]
use crate::decoder::LowDelay;
use crate::decoder::VideoDecoder;
#[cfg(feature = "openh264")]
use crate::decoder::openh264::OpenH264Codec;

use ruffle_render::backend::RenderBackend;
use ruffle_video::VideoStreamHandle;
use ruffle_video::backend::VideoBackend;
use ruffle_video::error::Error;
use ruffle_video::frame::{DecodedFrameOut, EncodedFrame, FrameDependency, PresentationTime};
use ruffle_video::queue::{Presentation, PresentationQueue};
use ruffle_video_software::backend::SoftwareVideoBackend;
use slotmap::SlotMap;

use swf::{VideoCodec, VideoDeblocking};

// Just to avoid the several conditional imports.
#[cfg(feature = "webcodecs")]
type LogSubscriberArc = std::sync::Arc<
    tracing_subscriber::layer::Layered<tracing_wasm::WASMLayer, tracing_subscriber::Registry>,
>;

enum ProxyOrStream {
    /// These streams are passed through to the wrapped software
    /// backend, accessed using the stored ("inner") handle,
    /// which is completely internal to this backend.
    Proxied(VideoStreamHandle),

    /// These streams are handled by this backend directly.
    Owned(VideoStream),
}

/// A video backend that falls back to the software backend for most codecs,
/// except for H.264, for which it uses an external decoder.
pub struct ExternalVideoBackend {
    streams: SlotMap<VideoStreamHandle, ProxyOrStream>,
    #[cfg(feature = "openh264")]
    openh264_codec: Option<OpenH264Codec>,
    // Needed for the callbacks in the `webcodecs` backend.
    #[cfg(feature = "webcodecs")]
    log_subscriber: Option<LogSubscriberArc>,
    software: SoftwareVideoBackend,
}

impl Default for ExternalVideoBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl ExternalVideoBackend {
    fn make_decoder(&mut self) -> Result<Box<dyn VideoDecoder>, Error> {
        #[cfg(feature = "openh264")]
        if let Some(h264_codec) = self.openh264_codec.as_ref() {
            let decoder = Box::new(crate::decoder::openh264::H264Decoder::new(h264_codec));
            return Ok(decoder);
        }

        #[cfg(feature = "webcodecs")]
        {
            let log_subscriber = self
                .log_subscriber
                .clone()
                .ok_or(Error::DecoderError("log subscriber not set".into()))?;
            let decoder = crate::decoder::webcodecs::H264Decoder::new(log_subscriber);
            return decoder
                .map(|decoder| Box::new(LowDelay::new(decoder)) as Box<dyn VideoDecoder>);
        }

        #[allow(unreachable_code)]
        Err(Error::DecoderError("No H.264 decoder available".into()))
    }

    // Neither the `openh264` nor the `webcodecs` backend will be available.
    pub fn new() -> Self {
        Self {
            streams: SlotMap::with_key(),
            #[cfg(feature = "openh264")]
            openh264_codec: None,
            #[cfg(feature = "webcodecs")]
            log_subscriber: None,
            software: SoftwareVideoBackend::new(),
        }
    }

    #[cfg(feature = "openh264")]
    pub fn new_with_openh264(openh264_codec: OpenH264Codec) -> Self {
        Self {
            streams: SlotMap::with_key(),
            openh264_codec: Some(openh264_codec),
            #[cfg(feature = "webcodecs")]
            log_subscriber: None,
            software: SoftwareVideoBackend::new(),
        }
    }

    #[cfg(feature = "webcodecs")]
    pub fn new_with_webcodecs(log_subscriber: LogSubscriberArc) -> Self {
        Self {
            streams: SlotMap::with_key(),
            #[cfg(feature = "openh264")]
            openh264_codec: None,
            log_subscriber: Some(log_subscriber),
            software: SoftwareVideoBackend::new(),
        }
    }
}

// NOTE: The stream handles coming in through this API must not be
// conflated with the ones stored in `streams` as `Proxied`.
impl VideoBackend for ExternalVideoBackend {
    fn register_video_stream(
        &mut self,
        num_frames: u32,
        size: (u16, u16),
        codec: VideoCodec,
        filter: VideoDeblocking,
    ) -> Result<VideoStreamHandle, Error> {
        let proxy_or_stream = if codec == VideoCodec::H264 {
            let decoder = self.make_decoder()?;
            let stream = VideoStream::new(decoder);
            ProxyOrStream::Owned(stream)
        } else {
            ProxyOrStream::Proxied(
                self.software
                    .register_video_stream(num_frames, size, codec, filter)?,
            )
        };

        Ok(self.streams.insert(proxy_or_stream))
    }

    fn configure_video_stream_decoder(
        &mut self,
        stream: VideoStreamHandle,
        configuration_data: &[u8],
    ) -> Result<(), Error> {
        let stream = self
            .streams
            .get_mut(stream)
            .ok_or(Error::VideoStreamIsNotRegistered)?;

        match stream {
            ProxyOrStream::Proxied(handle) => self
                .software
                .configure_video_stream_decoder(*handle, configuration_data),
            ProxyOrStream::Owned(stream) => stream.decoder.configure_decoder(configuration_data),
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
            .ok_or(Error::VideoStreamIsNotRegistered)?;

        match stream {
            ProxyOrStream::Proxied(handle) => self
                .software
                .preload_video_stream_frame(*handle, encoded_frame),
            ProxyOrStream::Owned(stream) => stream.decoder.preload_frame(encoded_frame),
        }
    }

    fn submit_video_stream_frame(
        &mut self,
        stream: VideoStreamHandle,
        encoded_frame: EncodedFrame<'_>,
        pts: PresentationTime,
    ) -> Result<(), Error> {
        let stream = self
            .streams
            .get_mut(stream)
            .ok_or(Error::VideoStreamIsNotRegistered)?;

        match stream {
            ProxyOrStream::Proxied(handle) => {
                self.software
                    .submit_video_stream_frame(*handle, encoded_frame, pts)
            }
            ProxyOrStream::Owned(stream) => stream.submit(encoded_frame, pts),
        }
    }

    fn present_video_stream_frame(
        &mut self,
        stream: VideoStreamHandle,
        pts: PresentationTime,
        renderer: &mut dyn RenderBackend,
    ) -> Result<Presentation, Error> {
        let stream = self
            .streams
            .get_mut(stream)
            .ok_or(Error::VideoStreamIsNotRegistered)?;

        match stream {
            ProxyOrStream::Proxied(handle) => self
                .software
                .present_video_stream_frame(*handle, pts, renderer),
            ProxyOrStream::Owned(stream) => stream.queue.present(pts, renderer),
        }
    }

    fn flush_video_stream(&mut self, stream: VideoStreamHandle) -> Result<(), Error> {
        let stream = self
            .streams
            .get_mut(stream)
            .ok_or(Error::VideoStreamIsNotRegistered)?;

        match stream {
            ProxyOrStream::Proxied(handle) => self.software.flush_video_stream(*handle),
            ProxyOrStream::Owned(stream) => stream.flush(),
        }
    }

    fn video_stream_is_drained(&self, stream: VideoStreamHandle) -> bool {
        match self.streams.get(stream) {
            Some(ProxyOrStream::Proxied(handle)) => self.software.video_stream_is_drained(*handle),
            Some(ProxyOrStream::Owned(stream)) => stream.queue.is_drained(),
            None => true,
        }
    }

    fn reset_video_stream(&mut self, stream: VideoStreamHandle) -> Result<(), Error> {
        let stream = self
            .streams
            .get_mut(stream)
            .ok_or(Error::VideoStreamIsNotRegistered)?;

        match stream {
            ProxyOrStream::Proxied(handle) => self.software.reset_video_stream(*handle),
            ProxyOrStream::Owned(stream) => stream.reset(),
        }
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
