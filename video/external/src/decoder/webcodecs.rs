use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use crate::decoder::VideoDecoder;

use ruffle_render::bitmap::BitmapFormat;
use ruffle_video::error::Error;
use ruffle_video::frame::{DecodedFrame, EncodedFrame, FrameDependency};

use js_sys::Uint8Array;
use js_sys::futures::{JsFuture, spawn_local};
use tracing::{debug, error, trace, warn};
use tracing_subscriber::{Registry, layer::Layered};
use tracing_wasm::WASMLayer;
use wasm_bindgen::prelude::*;
use web_sys::{
    CodecState, DomException, EncodedVideoChunk, EncodedVideoChunkInit, EncodedVideoChunkType,
    VideoDecoder as WebVideoDecoder, VideoDecoderConfig, VideoDecoderInit, VideoFrame,
    VideoPixelFormat,
};

// Abbreviations used:
//  - NAL: Network Abstraction Layer
//  - NALU: NAL unit
//  - VCL: Video Coding Layer
//  - SPS: Sequence Parameter Set
//  - PPS: Picture Parameter Set
//  - IDR: Instantaneous Decoding Refresh
//  - SEI: Supplemental enhancement information

// NALU type 5 means IDR frame - basically a keyframe.
const NALU_TYPE_IDR: u8 = 5;

fn js_error_to_decoder_error(js_error: JsValue) -> Error {
    Error::DecoderError(
        js_error
            .dyn_ref::<js_sys::Error>()
            .unwrap()
            .message()
            .as_string()
            .unwrap()
            .into(),
    )
}

pub struct H264Decoder {
    /// How many bytes are used to store the length of the NALU (1, 2, 3, or 4).
    length_size: u8,

    /// The WebCodecs decoder object.
    decoder: WebVideoDecoder,

    /// The decoder output callback writes this, and the decode_frame method reads it.
    ///
    /// This in itself results in one frame of delay (because we can't block decode_frame
    /// until the callback is invoked, nor until the pixel data is copied out of the frame
    /// that was passed to it), but it shouldn't matter in practice.
    last_frame: Rc<RefCell<Option<DecodedFrame>>>,

    // Simply keeping these objects alive, as they are used by the JS side.
    // See: https://rustwasm.github.io/wasm-bindgen/examples/closures.html
    #[expect(dead_code)]
    output_callback: Closure<dyn Fn(VideoFrame)>,
    #[expect(dead_code)]
    error_callback: Closure<dyn Fn(DomException)>,
}

impl H264Decoder {
    /// `extradata` should hold "AVCC (MP4) format" decoder configuration, including PPS and SPS.
    /// Make sure it has any start code emulation prevention "three bytes" removed.
    ///
    /// The log_subscriber is needed so that we have proper logging from within the callbacks.
    pub fn new(log_subscriber: Arc<Layered<WASMLayer, Registry>>) -> Result<Self, Error> {
        let last_frame = Rc::new(RefCell::new(None));
        let lf = last_frame.clone();

        let log_subscriber_for_output = log_subscriber.clone();
        let output = move |frame: VideoFrame| {
            let _subscriber = tracing::subscriber::set_default(log_subscriber_for_output.clone());
            let visible_rect = frame.visible_rect().unwrap();
            let width = visible_rect.width() as u32;
            let height = visible_rect.height() as u32;

            let pixel_format = frame.format().unwrap();
            let (buffer_size, bitmap_format) = match pixel_format {
                VideoPixelFormat::I420 => (
                    width as usize * height as usize * 3 / 2,
                    BitmapFormat::Yuv420p,
                ),
                VideoPixelFormat::Bgrx => {
                    (width as usize * height as usize * 4, BitmapFormat::Rgba)
                }
                other_format => {
                    error!("Unsupported pixel format: {:?}", other_format);
                    frame.close();
                    return;
                }
            };

            // The pixel data is copied into a buffer on the JS heap, rather than directly
            // into a `Vec`, because `copyTo` is asynchronous, and the WASM linear memory a
            // `Vec` lives in may be grown (and thus moved, detaching any view JS holds of
            // it) while the copy is still in flight.
            let buffer = Uint8Array::new_with_length(buffer_size as u32);
            let copy_done = frame.copy_to_with_u8_array(&buffer);

            let last_frame = last_frame.clone();
            let log_subscriber_for_copy = log_subscriber_for_output.clone();
            spawn_local(async move {
                let _subscriber = tracing::subscriber::set_default(log_subscriber_for_copy);

                match JsFuture::from(copy_done).await {
                    Ok(_) => {
                        let mut data = buffer.to_vec();
                        if pixel_format == VideoPixelFormat::Bgrx {
                            for pixel in data.chunks_mut(4) {
                                pixel.swap(0, 2);
                                pixel[3] = 0xff;
                            }
                        }
                        last_frame.replace(Some(DecodedFrame::new(
                            width,
                            height,
                            bitmap_format,
                            data,
                        )));
                    }
                    Err(e) => error!("Failed to copy pixel data out of the frame: {:?}", e),
                }

                // Now that the copy is done (or has failed), the frame's underlying
                // (potentially GPU-backed) media resource can be released. This is not just
                // to save memory: decoders hand out frames from a small fixed-size pool, and
                // if these are only reclaimed whenever the JS garbage collector gets around
                // to it, the pool runs dry, and the decoder stops producing output entirely.
                frame.close();
            });
        };

        let log_subscriber_for_error = log_subscriber.clone();
        let error = move |error: &DomException| {
            let _subscriber = tracing::subscriber::set_default(log_subscriber_for_error.clone());
            error!("WebCodecs error: {:}", error.message());
        };

        let output_callback = Closure::new(output);
        let error_callback = Closure::new(move |exception| error(&exception));

        let decoder = WebVideoDecoder::new(&VideoDecoderInit::new(
            error_callback.as_ref().unchecked_ref(),
            output_callback.as_ref().unchecked_ref(),
        ))
        .map_err(js_error_to_decoder_error)?;

        Ok(Self {
            length_size: 0,
            decoder,
            output_callback,
            error_callback,
            last_frame: lf,
        })
    }
}

impl Drop for H264Decoder {
    fn drop(&mut self) {
        // Frees the underlying (potentially hardware) decoder instance immediately,
        // rather than leaving it to the JS garbage collector.
        // Closing an already closed decoder would throw, hence the state check.
        if self.decoder.state() != CodecState::Closed {
            // This aborts any work still in the queue, which is fine, as no one is
            // going to be interested in the results of it anymore.
            if let Err(e) = self.decoder.close() {
                warn!("Error closing WebCodecs decoder: {:?}", e);
            }
        }
    }
}

/// Provides an iterator for individual consecutive NALUs in a byte stream,
/// also providing the type of each NALU for easier usage.
fn iter_nalus(data: &[u8], length_size: usize) -> impl Iterator<Item = (u8, &[u8])> {
    trace!(
        "iter_nalus on a {} long chunk with length_size {}",
        data.len(),
        length_size
    );

    let mut rest = data;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }

        if rest.len() < length_size {
            warn!("Not enough data to read NALU length");
            return None;
        }

        // Extracting and skipping over the NALU length.
        let mut encoded_len = 0;
        for b in rest.iter().take(length_size) {
            encoded_len = (encoded_len << 8) | *b as usize;
        }
        trace!("encoded_len: {}", encoded_len);

        if rest.len() < length_size + encoded_len {
            warn!("Not enough data to read NALU");
            return None;
        }

        // Extracting and skipping over the NALU type and data.
        let nalu_type = rest[length_size] & 0b0001_1111;
        let nalu;
        (nalu, rest) = rest.split_at(length_size + encoded_len);

        trace!("nalu_type: {}", nalu_type);
        trace!("rest len: {}", rest.len());
        Some((nalu_type, nalu))
    })
}

impl VideoDecoder for H264Decoder {
    fn configure_decoder(&mut self, configuration_data: &[u8]) -> Result<(), Error> {
        // extradata[0]: configuration version, always 1
        // extradata[1]: profile
        // extradata[2]: compatibility
        // extradata[3]: level
        // extradata[4]: 6 reserved bits | NALU length size - 1

        // The codec string is the profile, compatibility, and level bytes as hex.

        if configuration_data.len() < 5 {
            return Err(Error::DecoderError(
                "Invalid configuration data for H264 decoder".into(),
            ));
        }
        if configuration_data[0] != 1 {
            return Err(Error::DecoderError(
                "Invalid configuration version for H264 decoder".into(),
            ));
        }

        self.length_size = (configuration_data[4] & 0b0000_0011) + 1;

        trace!("length_size: {}", self.length_size);

        let codec_string = format!(
            "avc1.{:02x}{:02x}{:02x}",
            configuration_data[1], configuration_data[2], configuration_data[3]
        );
        let config = VideoDecoderConfig::new(&codec_string);
        trace!("decoder state: {:?}", self.decoder.state());
        trace!("configuring decoder with: {:?}", &configuration_data[1..4]);

        let data = Uint8Array::from(configuration_data);
        config.set_description(&data);
        config.set_optimize_for_latency(true);
        self.decoder
            .configure(&config)
            .map_err(js_error_to_decoder_error)?;

        trace!("decoder state: {:?}", self.decoder.state());
        Ok(())
    }

    fn preload_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<FrameDependency, Error> {
        debug!("preloading frame {}", encoded_frame.frame_id);

        for (nalu_type, _nalu) in iter_nalus(encoded_frame.data, self.length_size as usize) {
            // "After the decoding of an IDR picture all following coded pictures in decoding order can
            // be decoded without inter prediction from any picture decoded prior to the IDR picture."
            if nalu_type == NALU_TYPE_IDR {
                trace!("is key");
                return Ok(FrameDependency::None);
            }
        }

        trace!("is not key");
        Ok(FrameDependency::Past)
    }

    fn decode_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<DecodedFrame, Error> {
        debug!("decoding frame {}", encoded_frame.frame_id);
        trace!("decoder state: {:?}", self.decoder.state());
        trace!("queue size: {}", self.decoder.decode_queue_size());

        let mut frame_type = EncodedVideoChunkType::Delta;
        for (nalu_type, _nalu) in iter_nalus(encoded_frame.data, self.length_size as usize) {
            if nalu_type == NALU_TYPE_IDR {
                frame_type = EncodedVideoChunkType::Key;
            }
        }
        trace!("frame type: {:?}", frame_type);

        // The timestamp doesn't matter for us.
        let init = EncodedVideoChunkInit::new(&Uint8Array::from(encoded_frame.data), 0, frame_type);
        let chunk = EncodedVideoChunk::new(&init).unwrap();

        self.decoder
            .decode(&chunk)
            .map_err(js_error_to_decoder_error)?;
        trace!("decoder state: {:?}", self.decoder.state());

        match self.last_frame.borrow_mut().take() {
            Some(frame) => Ok(frame),
            None => Err(Error::DecoderError(
                "No output frame produced by the decoder".into(),
            )),
        }
    }
}
