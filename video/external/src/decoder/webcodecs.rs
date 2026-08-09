use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use crate::decoder::VideoDecoder;

use ruffle_render::bitmap::BitmapFormat;
use ruffle_video::error::Error;
use ruffle_video::frame::{DecodedFrame, DecodedFrameOut, EncodedFrame, FrameDependency};

use js_sys::Uint8Array;
use tracing::{debug, error, trace, warn};
use tracing_subscriber::{Registry, layer::Layered};
use tracing_wasm::WASMLayer;
use wasm_bindgen::prelude::*;
use web_sys::{
    DomException, EncodedVideoChunk, EncodedVideoChunkInit, EncodedVideoChunkType,
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

    /// Pictures the output callback has delivered but that have not been
    /// collected yet.
    ///
    /// The callback runs from the event loop rather than from inside
    /// `decode()`, so nothing submitted during a tick can come back during that
    /// same tick. Frames are fed far enough ahead of the playhead for that not
    /// to matter, and each one is recognised by the `frame_id` carried on it
    /// rather than by when it turns up.
    ready: Rc<RefCell<Vec<DecodedFrameOut>>>,

    /// The AVCC configuration record, kept so that the decoder can be
    /// configured again after a reset.
    configuration: Vec<u8>,

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
        let ready: Rc<RefCell<Vec<DecodedFrameOut>>> = Rc::new(RefCell::new(Vec::new()));
        let ready_for_output = ready.clone();

        let log_subscriber_for_output = log_subscriber.clone();
        let output = move |output: &VideoFrame| {
            let _subscriber = tracing::subscriber::set_default(log_subscriber_for_output.clone());
            let visible_rect = output.visible_rect().unwrap();
            let width = visible_rect.width() as u32;
            let height = visible_rect.height() as u32;

            let frame = match output.format().unwrap() {
                VideoPixelFormat::I420 => {
                    let mut data: Vec<u8> = vec![0; width as usize * height as usize * 3 / 2];
                    let _ = output.copy_to_with_u8_slice(&mut data);
                    Some(DecodedFrame::new(
                        width,
                        height,
                        BitmapFormat::Yuv420p,
                        data,
                    ))
                }
                VideoPixelFormat::Bgrx => {
                    let mut data: Vec<u8> = vec![0; width as usize * height as usize * 4];
                    let _ = output.copy_to_with_u8_slice(&mut data);
                    for pixel in data.chunks_mut(4) {
                        pixel.swap(0, 2);
                        pixel[3] = 0xff;
                    }
                    Some(DecodedFrame::new(width, height, BitmapFormat::Rgba, data))
                }
                VideoPixelFormat::Nv12 => {
                    let luma_len = width as usize * height as usize;
                    let chroma_len = (width as usize).div_ceil(2) * (height as usize).div_ceil(2);
                    let mut data: Vec<u8> = vec![0; luma_len + chroma_len * 2];
                    let _ = output.copy_to_with_u8_slice(&mut data);
                    let chroma = data.split_off(luma_len);
                    let chroma_pairs = chroma.as_chunks::<2>().0;
                    for uv in chroma_pairs {
                        data.push(uv[0]);
                    }
                    for uv in chroma_pairs {
                        data.push(uv[1]);
                    }
                    Some(DecodedFrame::new(
                        width,
                        height,
                        BitmapFormat::Yuv420p,
                        data,
                    ))
                }
                other_format => {
                    error!("Unsupported pixel format: {:?}", other_format);
                    None
                }
            };

            if let Some(frame) = frame {
                // The chunk was submitted carrying its `frame_id` as the
                // timestamp, and it comes back on the picture decoded from it.
                // Frames arrive in presentation order, which is generally not
                // the order they were submitted in.
                // Round-tripped through the i32 the chunk carried, so this
                // recovers the original id exactly.
                ready_for_output.borrow_mut().push(DecodedFrameOut {
                    frame_id: output.timestamp() as i64 as u32,
                    frame,
                });
            }

            output.close();
        };

        let log_subscriber_for_error = log_subscriber.clone();
        let error = move |error: &DomException| {
            let _subscriber = tracing::subscriber::set_default(log_subscriber_for_error.clone());
            error!("WebCodecs error: {:}", error.message());
        };

        let output_callback = Closure::new(move |frame| output(&frame));
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
            ready,
            configuration: Vec::new(),
        })
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
        self.decoder
            .configure(&config)
            .map_err(js_error_to_decoder_error)?;

        // Kept so that `reset`, which leaves the decoder unconfigured, can put
        // it back the way it was without the container having to re-send this.
        self.configuration = configuration_data.to_vec();

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

    fn submit_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<(), Error> {
        debug!("submitting frame {}", encoded_frame.frame_id);
        trace!("decoder state: {:?}", self.decoder.state());
        trace!("queue size: {}", self.decoder.decode_queue_size());

        let mut frame_type = EncodedVideoChunkType::Delta;
        for (nalu_type, _nalu) in iter_nalus(encoded_frame.data, self.length_size as usize) {
            if nalu_type == NALU_TYPE_IDR {
                frame_type = EncodedVideoChunkType::Key;
            }
        }
        trace!("frame type: {:?}", frame_type);

        // WebCodecs hands the timestamp back on whichever picture this chunk
        // turns into, so it is what we use to recognise it. Presentation timing
        // is the container's business, not the decoder's, so the frame's own
        // index serves perfectly well here.
        let init = EncodedVideoChunkInit::new(
            &Uint8Array::from(encoded_frame.data),
            encoded_frame.frame_id as i32,
            frame_type,
        );
        let chunk = EncodedVideoChunk::new(&init).unwrap();

        self.decoder
            .decode(&chunk)
            .map_err(js_error_to_decoder_error)?;
        trace!("decoder state: {:?}", self.decoder.state());

        Ok(())
    }

    fn poll_frames(&mut self, out: &mut Vec<DecodedFrameOut>) -> Result<(), Error> {
        out.append(&mut self.ready.borrow_mut());
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Error> {
        // This resolves a promise once the decoder has emitted everything it is
        // holding, but there is nothing useful to do with that: the pictures
        // arrive through the output callback either way, and the caller is
        // already prepared for them to turn up a tick or two later.
        let _ = self.decoder.flush();
        Ok(())
    }

    fn reset(&mut self) -> Result<(), Error> {
        self.ready.borrow_mut().clear();

        self.decoder.reset().map_err(js_error_to_decoder_error)?;

        // A reset decoder is an unconfigured one, and the container only sends
        // the configuration record at the start of the stream.
        if !self.configuration.is_empty() {
            let configuration = std::mem::take(&mut self.configuration);
            let result = self.configure_decoder(&configuration);
            self.configuration = configuration;
            result?;
        }

        // Anything the reconfiguration shook loose belongs to the old position.
        self.ready.borrow_mut().clear();
        Ok(())
    }
}
