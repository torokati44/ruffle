use std::cell::{Ref, RefCell};
use std::rc::Rc;

use crate::decoder::VideoDecoder;

use ruffle_render::bitmap::{BitmapFormat, BitmapHandle};
use ruffle_video::error::Error;
use ruffle_video::frame::{DecodedFrame, EncodedFrame, FrameDependency};

use js_sys::{Function, Uint8Array};
use ruffle_video::VideoStreamHandle;
use wasm_bindgen::{prelude::*, JsObject};
use web_sys::{
    DomException, EncodedVideoChunk, EncodedVideoChunkInit, EncodedVideoChunkType,
    VideoDecoder as WebVideoDecoder, VideoDecoderConfig, VideoDecoderInit, VideoFrame,
    VideoFrameCopyToOptions, VideoPixelFormat,
};

/// H264 video decoder.
pub struct H264Decoder {
    /// How many bytes are used to store the length of the NALU (1, 2, 3, or 4).
    length_size: u8,

    decoder: WebVideoDecoder,

    // this is called by JS, kept for refkeeping purposes
    output_callback: Closure<dyn Fn(VideoFrame)>,
    error_callback: Closure<dyn Fn(DomException)>,

    last_frame: Rc<RefCell<Option<DecodedFrame>>>,
}

impl H264Decoder {
    /// `extradata` should hold "AVCC (MP4) format" decoder configuration, including PPS and SPS.
    /// Make sure it has any start code emulation prevention "three bytes" removed.
    pub fn new(callback: Rc<RefCell<dyn Fn(DecodedFrame)>>) -> Self {
        let mut last_frame = Rc::new(RefCell::new(None::<DecodedFrame>));
        let mut lf2 = last_frame.clone();

        let output = move |frame: VideoFrame| {
            tracing::warn!("webcodecs output frame");
            let visible_rect = frame.visible_rect().unwrap();
            let visible_rect2 = frame.visible_rect().unwrap();

            let mut lf3 = lf2.clone();
            let mut cb3 = callback.clone();

            let done = move |layout: JsValue| {
                let mut frame = lf3.as_ref().borrow_mut();
                let cb = cb3.as_ref().borrow_mut();
                if let Some(frame) = frame.as_ref() {
                    cb(frame.clone());
                }
            };

            let copy_done_callback = Closure::<dyn FnMut(JsValue) + 'static>::new(done);

            match frame.format().unwrap() {
                VideoPixelFormat::I420 => {
                    let mut bitmap = lf2.as_ref().borrow_mut();
                    bitmap.replace(DecodedFrame::new(
                        visible_rect.width() as u32,
                        visible_rect.height() as u32,
                        BitmapFormat::Yuv420p,
                        vec![
                            0;
                            visible_rect.width() as usize * visible_rect.height() as usize * 3 / 2
                        ],
                    ));
                    let _ = frame
                        .copy_to_with_u8_array(&mut bitmap.as_mut().unwrap().data_mut())
                        .then(&copy_done_callback);
                }
                VideoPixelFormat::Bgrx => {
                    let mut bitmap = lf2.as_ref().borrow_mut();
                    bitmap.replace(DecodedFrame::new(
                        visible_rect.width() as u32,
                        visible_rect.height() as u32,
                        BitmapFormat::Rgba,
                        vec![0; visible_rect.width() as usize * visible_rect.height() as usize * 4],
                    ));
                    let _ = frame
                        .copy_to_with_u8_array(&mut bitmap.as_mut().unwrap().data_mut())
                        .then(&copy_done_callback);
                }
                _ => {
                    assert!(
                        false,
                        "unsupported pixel format: {:?}",
                        frame.format().unwrap()
                    );
                }
            };

            copy_done_callback.forget(); // TODO: not leak!
        };

        let error = |error: DomException| {
            tracing::error!("webcodecs error {:}", error.message());
        };

        let output_callback = Closure::<dyn Fn(VideoFrame)>::new(output);
        let error_callback = Closure::<dyn Fn(DomException)>::new(error);

        let decoder = WebVideoDecoder::new(&VideoDecoderInit::new(
            error_callback.as_ref().unchecked_ref(),
            output_callback.as_ref().unchecked_ref(),
        ))
        .unwrap();

        Self {
            length_size: 0,
            decoder,
            output_callback,
            error_callback,
            last_frame,
        }
    }
}

impl Drop for H264Decoder {
    fn drop(&mut self) {}
}

impl VideoDecoder for H264Decoder {
    fn configure_decoder(&mut self, configuration_data: &[u8]) -> Result<(), Error> {
        // extradata[0]: configuration version, always 1
        // extradata[1]: profile
        // extradata[2]: compatibility
        // extradata[3]: level
        // extradata[4]: 6 reserved bits | NALU length size - 1

        // The codec string is the profile, compatibility, and level bytes as hex.

        self.length_size = (configuration_data[4] & 0b0000_0011) + 1;

        tracing::warn!("length_size: {}", self.length_size);

        let codec_string = format!(
            "avc1.{:02x}{:02x}{:02x}",
            configuration_data[1], configuration_data[2], configuration_data[3]
        );
        let mut config = VideoDecoderConfig::new(&codec_string);
        tracing::warn!("configuring decoder: {:?}", &configuration_data[1..4]);
        tracing::info!("{:?}", self.decoder.state());

        let data = Uint8Array::from(configuration_data);
        config.description(&data);
        config.optimize_for_latency(true);
        self.decoder.configure(&config);
        tracing::info!("{:?}", self.decoder.state());
        Ok(())
    }

    fn preload_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<FrameDependency, Error> {
        tracing::warn!("preloading frame");

        let nal_unit_type = encoded_frame.data[self.length_size as usize] & 0b0001_1111;

        // 3.62 instantaneous decoding refresh (IDR) picture:
        // After the decoding of an IDR picture all following coded pictures in decoding order can
        // be decoded without inter prediction from any picture decoded prior to the IDR picture.
        if nal_unit_type == 5u8 {
            // openh264_sys::NAL_SLICE_IDR as u8
            tracing::info!("is key");
            Ok(FrameDependency::None)
        } else {
            tracing::info!("is not key");
            Ok(FrameDependency::Past)
        }
    }

    fn decode_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<DecodedFrame, Error> {
        tracing::warn!("decoding frame {}", encoded_frame.frame_id);
        tracing::info!("{:?}", self.decoder.state());

        tracing::warn!("queue size: {}", self.decoder.decode_queue_size());

        let mut offset = 0;

        while offset < encoded_frame.data.len() {
            let mut encoded_len = 0;

            for i in 0..self.length_size {
                encoded_len = (encoded_len << 8) | encoded_frame.data[offset + i as usize] as u32;
            }

            tracing::warn!(
                "encoded_len: {}, chunk length: {}",
                encoded_len,
                encoded_frame.data.len()
            );

            let nal_unit_type =
                encoded_frame.data[offset + self.length_size as usize] & 0b0001_1111;

            tracing::warn!("nal_unit_type: {}", nal_unit_type);

            if nal_unit_type != 6u8 {
                // skipping SEI NALus
                // 3.62 instantaneous decoding refresh (IDR) picture:
                // After the decoding of an IDR picture all following coded pictures in decoding order can
                // be decoded without inter prediction from any picture decoded prior to the IDR picture.
                let frame_type = if nal_unit_type == 5u8 {
                    // openh264_sys::NAL_SLICE_IDR as u8
                    tracing::info!("is key");
                    EncodedVideoChunkType::Key
                } else {
                    tracing::info!("is not key");
                    EncodedVideoChunkType::Delta
                };
                let timestamp = (encoded_frame.frame_id as f64 - 1.0) * 1000000.0 * 0.5
                    + encoded_frame.time_offset as f64 * 1000.0;
                tracing::warn!(
                    "time offset: {}, timestamp: {}",
                    encoded_frame.time_offset,
                    timestamp
                );
                let data = Uint8Array::from(
                    &encoded_frame.data
                        [offset..offset + encoded_len as usize + self.length_size as usize],
                );
                let init = EncodedVideoChunkInit::new(&data, timestamp, frame_type);
                let chunk = EncodedVideoChunk::new(&init).unwrap();

                self.decoder.decode(&chunk);
                tracing::info!("{:?}", self.decoder.state());
            }

            offset += encoded_len as usize + self.length_size as usize;
        }

        assert!(
            offset == encoded_frame.data.len(),
            "Incomplete NALu at the end"
        );

        self.last_frame
            .borrow()
            .as_ref()
            .map(|frame| frame.clone())
            .ok_or(Error::DecoderError("asd".into()))
    }
}
