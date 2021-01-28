use std::ptr::slice_from_raw_parts_mut;

use crate::bindings::*;
pub struct VP6State {
    pub decoded_frames: i32,

    pub context: *mut AVCodecContext,
    pub packet: *mut AVPacket,
    pub frame: *mut AVFrame,
}

impl VP6State {
    pub fn new() -> Self {
        unsafe {
            let mut codec = ff_vp6f_decoder_ptr;
            let mut context: *mut AVCodecContext = avcodec_alloc_context3(codec);

            avcodec_open2(context, codec, std::ptr::null_mut::<*mut AVDictionary>());

            let mut packet: *mut AVPacket = av_packet_alloc();
            let mut frame: *mut AVFrame = av_frame_alloc();

            Self {
                decoded_frames: 0,
                context,
                packet,
                frame,
            }
        }
    }

    pub fn decode(&mut self, encoded_frame: &[u8]) -> (Vec<u8>, (usize, usize)) {
        unsafe {
            let inbuf =
                av_malloc(encoded_frame.len() as usize + AV_INPUT_BUFFER_PADDING_SIZE as usize) as *mut u8;

            for (i, e) in encoded_frame.iter().enumerate() {
                (*slice_from_raw_parts_mut(inbuf, encoded_frame.len()))[i] = *e;
            }

            av_packet_from_data(self.packet, inbuf, encoded_frame.len() as i32);

            let ret = avcodec_send_packet(self.context, self.packet);
            println!("ret from sendpacket: {:}", ret);
            let ret = avcodec_receive_frame(self.context, self.frame);
            println!("ret from recv frame: {:}", ret);

            let w = frame_width( self.frame) as usize;
            let h = frame_height( self.frame) as usize;

            let num_pixels = w * h;
            let d = slice_from_raw_parts_mut(
                frame_data( self.frame, 0),
                num_pixels as usize,
            );

            let mut r = vec![255; num_pixels*4];
            for i in 0..num_pixels {
                let v = (*d)[i];
                r[i*4] = v;
                r[i*4+1] = v;
                r[i*4+2] = v;
                r[i*4+3] = 255;
            }

            (r, (w, h))
        }
    }
}

// This trivial implementation of `drop` adds a print to console.
impl Drop for VP6State {
    fn drop(&mut self) {
        unsafe {
            av_packet_free(&mut self.packet);
            av_frame_free(&mut self.frame);
            avcodec_free_context(&mut self.context);
        }
    }
}

/*
unsafe {

avcodec_free_context(&mut ctx);
av_frame_free(&mut picture);
av_packet_free(&mut pkt);

*/
