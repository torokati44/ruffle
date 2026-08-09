use ruffle_render::bitmap::Bitmap;
use swf::VideoCodec;

/// An encoded video frame of some video codec.
pub struct EncodedFrame<'a> {
    /// The codec used to encode the frame.
    pub codec: VideoCodec,

    /// The raw bitstream data to funnel into the codec.
    pub data: &'a [u8],

    /// A caller-specified frame ID. Frame IDs must be consistent between
    /// subsequent uses of the same data stream.
    pub frame_id: u32,
}

impl<'a> EncodedFrame<'a> {
    /// Borrow this frame's data.
    pub fn data(&'a self) -> &'a [u8] {
        self.data
    }
}

/// A decoded frame of video. It can be in whichever format the decoder chooses.
pub type DecodedFrame = Bitmap<'static>;

/// A picture that has come out of a decoder, tagged with the `frame_id` of the
/// `EncodedFrame` it was decoded from.
///
/// Decoders are not obliged to hand pictures back in the order they were given
/// them - a codec with bidirectional prediction generally cannot - so the tag
/// is what lets the caller work out when each one is meant to be shown.
pub struct DecodedFrameOut {
    /// The `frame_id` of the `EncodedFrame` this was decoded from.
    pub frame_id: u32,

    /// The picture itself.
    pub frame: DecodedFrame,
}

/// What dependencies a given video frame has on any previous frames.
#[derive(Copy, Clone, Debug)]
pub enum FrameDependency {
    /// This frame has no reference frames and can be seeked to at any time.
    None,

    /// This frame has some number of reference frames that prohibit any
    /// out-of-order decoding.
    ///
    /// The only legal way to decode a `Past` frame is to decode every prior
    /// frame from the last `None` frame. In the event that there is no prior
    /// `None` frame, then video decoding should start from the beginning.
    Past,
}

impl FrameDependency {
    /// Determine if this given frame is a keyframe.
    ///
    /// A keyframe is a frame that can be independently seeked to without
    /// decoding any prior or future frames.
    pub fn is_keyframe(self) -> bool {
        matches!(self, FrameDependency::None)
    }
}
