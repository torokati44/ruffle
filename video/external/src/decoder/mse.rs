use std::cell::{Ref, RefCell};
use std::rc::Rc;

use crate::decoder::VideoDecoder;

use ruffle_render::bitmap::{BitmapFormat, BitmapHandle};
use ruffle_video::error::Error;
use ruffle_video::frame::{DecodedFrame, EncodedFrame, FrameDependency};

use js_sys::{Function, Uint8Array};
use ruffle_video::VideoStreamHandle;
use wasm_bindgen::{prelude::*, JsObject};
use web_sys::{DomException, MediaSource, HtmlVideoElement, HtmlCanvasElement, Url, CanvasRenderingContext2d};

/// H264 video decoder.
pub struct H264Decoder {
    /// How many bytes are used to store the length of the NALU (1, 2, 3, or 4).
    length_size: u8,

    video_element: HtmlVideoElement,
    canvas_element: HtmlCanvasElement, // TODO offscreen canvas?

    media_source: MediaSource,

}

impl H264Decoder {
    /// `extradata` should hold "AVCC (MP4) format" decoder configuration, including PPS and SPS.
    /// Make sure it has any start code emulation prevention "three bytes" removed.
    pub fn new() -> Self {

        let window = web_sys::window().unwrap();
		let document = window.document().unwrap();
		let body = document.body().expect("document expect to have have a body");
		let val = document.create_element("p").unwrap();
        let mut video_element : HtmlVideoElement = document.create_element("video").unwrap()
        .dyn_into().unwrap();


    let canvas_element : HtmlCanvasElement = document.create_element("canvas").unwrap()
    .dyn_into().unwrap();

        let media_source = MediaSource::new().unwrap();
        video_element.set_src(Url::create_object_url_with_source(&media_source).unwrap().as_str());
        Self {
            length_size: 0,
            video_element,
            canvas_element,
            media_source,
        }

        /*
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
        */
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

                let sb = self.media_source.add_source_buffer("video/mp4; codecs=\"avc1.42E01E, mp4a.40.2\"").unwrap();
                sb.append_buffer_with_array_buffer(&data.buffer()).unwrap();

                let context: CanvasRenderingContext2d = self.canvas_element
                    .get_context("2d") // TODO: "bitmaprenderer" ?
                    .unwrap().unwrap()
                    .dyn_into()
                    .unwrap();

                context.draw_image_with_html_video_element(&self.video_element, 0.0, 0.0);

                let image_data = context.get_image_data(0.0, 0.0, 20.0, 20.0).unwrap();

                let data = image_data.data();


                return Ok(DecodedFrame::new(
                    20,
                    20,
                    BitmapFormat::Rgba,
                    data.0,
                ));
            }

            offset += encoded_len as usize + self.length_size as usize;
        }

        assert!(
            offset == encoded_frame.data.len(),
            "Incomplete NALu at the end"
        );

        Err(Error::DecoderError("aaa".into()))
    }
}
