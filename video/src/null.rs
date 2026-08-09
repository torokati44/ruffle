use crate::VideoStreamHandle;
use crate::backend::VideoBackend;
use crate::error::Error;
use crate::frame::{EncodedFrame, FrameDependency, PresentationTime};
use crate::queue::Presentation;
use ruffle_render::backend::RenderBackend;
use slotmap::SlotMap;
use swf::{VideoCodec, VideoDeblocking};

pub struct NullVideoBackend {
    streams: SlotMap<VideoStreamHandle, ()>,
}

/// Implementation of video that does not decode any video.
///
/// Specifically:
///
///  * Registering a video stream succeeds but does nothing
///  * All video frames are silently marked as keyframes (`None` dependency)
///  * Submitting a frame fails with an error that video decoding is
///    unimplemented, and so nothing is ever presentable
impl NullVideoBackend {
    pub fn new() -> Self {
        Self {
            streams: SlotMap::with_key(),
        }
    }
}

impl Default for NullVideoBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoBackend for NullVideoBackend {
    fn register_video_stream(
        &mut self,
        _num_frames: u32,
        _size: (u16, u16),
        _codec: VideoCodec,
        _filter: VideoDeblocking,
    ) -> Result<VideoStreamHandle, Error> {
        Ok(self.streams.insert(()))
    }

    fn configure_video_stream_decoder(
        &mut self,
        _stream: VideoStreamHandle,
        _configuration_data: &[u8],
    ) -> Result<(), Error> {
        Ok(())
    }

    fn preload_video_stream_frame(
        &mut self,
        _stream: VideoStreamHandle,
        _encoded_frame: EncodedFrame<'_>,
    ) -> Result<FrameDependency, Error> {
        Ok(FrameDependency::None)
    }

    fn submit_video_stream_frame(
        &mut self,
        _stream: VideoStreamHandle,
        _encoded_frame: EncodedFrame<'_>,
        _pts: PresentationTime,
    ) -> Result<(), Error> {
        Err(Error::DecodingNotSupported)
    }

    fn present_video_stream_frame(
        &mut self,
        _stream: VideoStreamHandle,
        _pts: PresentationTime,
        _renderer: &mut dyn RenderBackend,
    ) -> Result<Presentation, Error> {
        Ok(Presentation::Empty)
    }
}
