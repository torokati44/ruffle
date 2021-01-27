use std::ptr::slice_from_raw_parts_mut;

use crate::bindings::*;
pub struct VP6State {
    context: *mut AVCodecContext,
    packet: *mut AVPacket,
    yuv_frame: *mut AVFrame,
    sws_context: *mut SwsContext,
}

impl VP6State {
    pub fn new() -> Self {
        unsafe {
            let codec = ff_vp6f_decoder_ptr;
            let context: *mut AVCodecContext = avcodec_alloc_context3(codec);

            avcodec_open2(context, codec, std::ptr::null_mut::<*mut AVDictionary>());

            let packet: *mut AVPacket = av_packet_alloc();
            let yuv_frame: *mut AVFrame = av_frame_alloc();

            Self {
                context,
                packet,
                yuv_frame,
                sws_context: std::ptr::null_mut(),
            }
        }
    }

    pub fn decode(&mut self, encoded_frame: &[u8]) -> (Vec<u8>, (usize, usize)) {
        unsafe {
            packet_set_size(self.packet, encoded_frame.len() as i32);

            let data = packet_data(self.packet);
            for (i, e) in encoded_frame.iter().enumerate() {
                (*slice_from_raw_parts_mut(data, encoded_frame.len()))[i] = *e;
            }

            let ret = avcodec_send_packet(self.context, self.packet);
            let ret = avcodec_receive_frame(self.context, self.yuv_frame);

            if self.sws_context.is_null() {
                self.sws_context = make_converter_context(self.yuv_frame);
            }

            let w = frame_width(self.yuv_frame) as usize;
            let h = frame_height(self.yuv_frame) as usize;
            let num_pixels = w * h;
            let mut rgba_data = vec![0; num_pixels * 4];
            convert_yuv_to_rgba(self.sws_context, self.yuv_frame, rgba_data.as_mut_ptr());

            (rgba_data, (w, h))
        }
    }
}

// This trivial implementation of `drop` adds a print to console.
impl Drop for VP6State {
    fn drop(&mut self) {
        unsafe {
            // TODO: free the sws context
            av_frame_free(&mut self.yuv_frame);
            av_packet_free(&mut self.packet);
            avcodec_free_context(&mut self.context);
        }
    }
}
