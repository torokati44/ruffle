use crate::decoder::VideoDecoder;
use ruffle_render::bitmap::BitmapFormat;
use ruffle_video::error::Error;
use ruffle_video::frame::{DecodedFrame, EncodedFrame, FrameDependency};

#[derive(thiserror::Error, Debug)]
pub enum H264Error {
    /*
    #[error("Picture wasn't found in the video stream")]
    NoPictureInVideoStream,

    #[error("Decoder error")]
    DecoderError(#[from] h263_rs::Error),

    #[error("Invalid picture type code: {0:?}")]
    InvalidPictureType(PictureTypeCode),

    #[error("Picture is missing width and height details")]
    MissingWidthHeight,
    */
}

impl From<H264Error> for Error {
    fn from(error: H264Error) -> Self {
        Error::DecoderError(Box::new(error))
    }
}

/// H264 video decoder.
pub struct H264Decoder();

impl H264Decoder {
    pub fn new() -> Self {
        println!("Creating H264 decoder");
        Self(

        )
    }
}

impl VideoDecoder for H264Decoder {
    fn preload_frame(&mut self, _encoded_frame: EncodedFrame<'_>) -> Result<FrameDependency, Error> {
        println!("Preloading frame");
        Ok(FrameDependency::None)
    }

    fn decode_frame(&mut self, _encoded_frame: EncodedFrame<'_>) -> Result<DecodedFrame, Error> {
        println!("Decoding frame");
        Ok(DecodedFrame::new(
            1,
            1,
            BitmapFormat::Rgb,
            vec![255, 0, 255],
        ))
    }
}

impl Default for H264Decoder {
    fn default() -> Self {
        Self::new()
    }
}
