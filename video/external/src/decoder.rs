use ruffle_video::error::Error;
use ruffle_video::frame::{DecodedFrame, EncodedFrame, FrameDependency};


// bindgen ../openh264/codec/api/wels/codec_api.h --no-prepend-enum-name \
//         --dynamic-loading OpenH264 -o openh264_sys.rs

#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(dead_code)]
mod openh264_sys;

#[cfg(feature = "openh264")]
pub mod openh264;

/// Trait for video decoders.
/// This should be implemented for each video codec.
pub trait VideoDecoder {
    /// Preload a frame.
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
    fn preload_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<FrameDependency, Error>;

    /// Decode a frame of a given video stream.
    ///
    /// This function is provided the external index of the frame, the codec
    /// used to decode the data, and what codec to decode it with. The codec
    /// provided here must match the one used to register the video stream.
    ///
    /// Frames may be decoded in any order that does not violate the frame
    /// dependencies declared by the output of `preload_video_stream_frame`.
    ///
    /// The decoded frame should be returned. An `Error` can be returned if
    /// a drawable bitmap can not be produced.
    fn decode_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<DecodedFrame, Error>;
}
