use swf::VideoCodec;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Unsupported video codec type: {0:?}")]
    UnsupportedCodec(VideoCodec),

    #[error("Video stream is not registered")]
    VideoStreamIsNotRegistered,

    #[error("Couldn't create bitmap for video frame")]
    BitmapError(#[from] ruffle_render::error::Error),

    #[error("Video decoding isn't supported")]
    DecodingNotSupported,

    #[error(transparent)]
    DecoderError(Box<dyn std::error::Error + Send + Sync>),
}
