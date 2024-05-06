
use crate::decoder::VideoDecoder;

use ruffle_render::bitmap::BitmapFormat;
use ruffle_video::error::Error;
use ruffle_video::frame::{DecodedFrame, EncodedFrame, FrameDependency};

use web_sys::{DomException, EncodedVideoChunk, EncodedVideoChunkInit, EncodedVideoChunkType, VideoDecoder as WebVideoDecoder, VideoDecoderConfig, VideoDecoderInit, VideoFrame};
use js_sys::{Function, Uint8Array};
use js_sys::{Array, Date};
use wasm_bindgen::prelude::*;
use web_sys::{Document, Element, HtmlElement, Window};



/// H264 video decoder.
pub struct H264Decoder {
    /// How many bytes are used to store the length of the NALU (1, 2, 3, or 4).
    length_size: u8,

    decoder: WebVideoDecoder,

    o_cb: Closure::<dyn Fn(VideoFrame)>,
    e_cb: Closure::<dyn Fn(DomException)>,
}

impl H264Decoder {
    /// `extradata` should hold "AVCC (MP4) format" decoder configuration, including PPS and SPS.
    /// Make sure it has any start code emulation prevention "three bytes" removed.
    pub fn new() -> Self {



        fn output(output: &VideoFrame) {
            tracing::warn!("webcodecs output frame");
        }

        fn error(error: &DomException) {
            tracing::warn!("webcodecs error");
        }

        let o = Closure::<dyn Fn(VideoFrame)>::new(move |o| output(&o));
        let e = Closure::<dyn Fn(DomException)>::new(move |e| error(&e));

        Self {
            length_size: 0,
            decoder: WebVideoDecoder::new(&VideoDecoderInit::new(e.as_ref().unchecked_ref(), o.as_ref().unchecked_ref())).unwrap(),
            o_cb: o,
            e_cb: e
        }
    }
}

impl Drop for H264Decoder {
    fn drop(&mut self) {

    }
}

impl VideoDecoder for H264Decoder {
    fn configure_decoder(&mut self, configuration_data: &[u8]) -> Result<(), Error> {
        let config = VideoDecoderConfig::new("avc1.*");
        tracing::warn!("configuring decoder");
        self.decoder.configure(&config);
        Ok(())
    }

    fn preload_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<FrameDependency, Error> {
        tracing::warn!("preloading frame");
        Ok(FrameDependency::None)
    }

    fn decode_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<DecodedFrame, Error> {
        tracing::warn!("decoding frame");
        let data = Uint8Array::from(encoded_frame.data());
        let init = EncodedVideoChunkInit::new(&data, 0.1, EncodedVideoChunkType::Key);
        let chunk = EncodedVideoChunk::new(&init).unwrap();
        self.decoder.decode(&chunk);
        return Err(Error::DecoderError(
            "No output frame produced by the decoder".into(),
        ));
    }
}
