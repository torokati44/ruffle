
use crate::decoder::VideoDecoder;

use ruffle_render::bitmap::BitmapFormat;
use ruffle_video::error::Error;
use ruffle_video::frame::{DecodedFrame, EncodedFrame, FrameDependency};

/// H264 video decoder.
pub struct H264Decoder {
    /// How many bytes are used to store the length of the NALU (1, 2, 3, or 4).
    length_size: u8,
}

impl H264Decoder {
    /// `extradata` should hold "AVCC (MP4) format" decoder configuration, including PPS and SPS.
    /// Make sure it has any start code emulation prevention "three bytes" removed.
    pub fn new() -> Self {
        Self {
            length_size: 0,
        }
    }
}

impl Drop for H264Decoder {
    fn drop(&mut self) {

    }
}

impl VideoDecoder for H264Decoder {
    fn configure_decoder(&mut self, configuration_data: &[u8]) -> Result<(), Error> {
        Ok(())
    }

    fn preload_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<FrameDependency, Error> {
        Ok(FrameDependency::None)
    }

    fn decode_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<DecodedFrame, Error> {

        return Err(Error::DecoderError(
            "No output frame produced by the decoder".into(),
        ));
    }
}
