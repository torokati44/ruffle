use crate::VideoStreamHandle;
use crate::error::Error;
use crate::frame::{EncodedFrame, FrameDependency, PresentationTime};
use crate::queue::Presentation;
use ruffle_render::backend::RenderBackend;
use swf::{VideoCodec, VideoDeblocking};

/// A backend that provides access to some number of video decoders.
///
/// Implementations of `VideoBackend` are not required to actually support
/// decoding any video formats. However, they must interoperate with at least
/// one `RenderBackend` so that renderable video frames may be passed from the
/// decoder to the renderer.
pub trait VideoBackend {
    /// Register a new video stream.
    ///
    /// Most of the parameters provided to this function are advisory: the
    /// actual video data stream provided to the decoder may vary in size or
    /// number of frames. This function should return an `Error` if it is not
    /// possible to decode video with the given parameters.
    fn register_video_stream(
        &mut self,
        num_frames: u32,
        size: (u16, u16),
        codec: VideoCodec,
        filter: VideoDeblocking,
    ) -> Result<VideoStreamHandle, Error>;

    /// Configure the decoder of a given video stream.
    ///
    /// The `configuration_data` contains codec-specific parameters.
    fn configure_video_stream_decoder(
        &mut self,
        stream: VideoStreamHandle,
        configuration_data: &[u8],
    ) -> Result<(), Error>;

    /// Preload a frame of a given video stream.
    ///
    /// No decoding is intended to happen at this point in time. Instead, the
    /// video data should be inspected to determine inter-frame dependencies
    /// between this and any previous frames in the stream.
    ///
    /// Frames should be preloaded in the order that they are received.
    ///
    /// Any dependencies listed here are inherent to the video bitstream. The
    /// containing video stream is also permitted to introduce additional
    /// interframe dependencies.
    fn preload_video_stream_frame(
        &mut self,
        stream: VideoStreamHandle,
        encoded_frame: EncodedFrame<'_>,
    ) -> Result<FrameDependency, Error>;

    /// Queue a frame of a given video stream for decoding, to be shown at
    /// `pts`.
    ///
    /// Frames must be submitted in decode order, which is the order they occur
    /// in the bitstream, and must not violate the frame dependencies declared
    /// by the output of `preload_video_stream_frame`. That order need not match
    /// the order the frames are to be presented in: `pts` is what says when
    /// each one is actually due.
    ///
    /// Nothing is decoded to screen here. A frame submitted now may not be
    /// presentable until several more have followed it.
    fn submit_video_stream_frame(
        &mut self,
        stream: VideoStreamHandle,
        encoded_frame: EncodedFrame<'_>,
        pts: PresentationTime,
    ) -> Result<(), Error>;

    /// Move a video stream's presentation clock to `pts`, putting the newest
    /// frame that is due by then on screen.
    ///
    /// Frames that were due earlier but never got shown are dropped rather than
    /// displayed late.
    ///
    /// A returned `BitmapInfo` will be renderable only on the given
    /// `RenderBackend`. `VideoBackend` implementations are allowed to return an
    /// error if a drawable bitmap cannot be produced for the given renderer.
    ///
    /// Any previously returned bitmaps may be updated, invalidated, or
    /// reclaimed by whatever means the decoder implementation chooses.
    fn present_video_stream_frame(
        &mut self,
        stream: VideoStreamHandle,
        pts: PresentationTime,
        renderer: &mut dyn RenderBackend,
    ) -> Result<Presentation, Error>;

    /// Tell a video stream's decoder that no more frames are coming, so that it
    /// releases the pictures it is still holding back for reordering.
    ///
    /// Without this the last few frames of a stream with bidirectional
    /// prediction never come out at all.
    fn flush_video_stream(&mut self, stream: VideoStreamHandle) -> Result<(), Error>;

    /// Whether a video stream has shown everything it has decoded.
    ///
    /// A stream that has run out of frames to feed can still have several
    /// pictures waiting for their turn on screen, so this is what says whether
    /// playback has really finished.
    fn video_stream_is_drained(&self, stream: VideoStreamHandle) -> bool;

    /// Throw away everything a video stream has decoded but not yet shown,
    /// along with any decoder state, because playback has jumped elsewhere.
    ///
    /// The picture currently on screen is kept: Flash Player does not blank the
    /// video while seeking.
    fn reset_video_stream(&mut self, stream: VideoStreamHandle) -> Result<(), Error>;
}
