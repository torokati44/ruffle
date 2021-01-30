// Much of this was copied from the (gigantic) bindings auto-generated by `bindgen`.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

#[repr(C)]
pub struct AVCodec {
    private: [u8; 0],
}
#[repr(C)]
pub struct AVFrame {
    private: [u8; 0],
}
#[repr(C)]
pub struct AVPacket {
    private: [u8; 0],
}
#[repr(C)]
pub struct AVCodecContext {
    private: [u8; 0],
}
#[repr(C)]
pub struct AVDictionary {
    private: [u8; 0],
}
#[repr(C)]
pub struct SwsContext {
    private: [u8; 0],
}

extern "C" {
    pub fn avcodec_alloc_context3(codec: *const AVCodec) -> *mut AVCodecContext;
    pub fn avcodec_free_context(avctx: *mut *mut AVCodecContext);

    pub fn avcodec_open2(
        avctx: *mut AVCodecContext,
        codec: *const AVCodec,
        options: *mut *mut AVDictionary,
    ) -> ::std::os::raw::c_int;

    pub fn av_packet_alloc() -> *mut AVPacket;
    pub fn av_packet_free(pkt: *mut *mut AVPacket);

    pub fn av_frame_alloc() -> *mut AVFrame;
    pub fn av_frame_free(frame: *mut *mut AVFrame);

    pub fn avcodec_send_packet(
        avctx: *mut AVCodecContext,
        avpkt: *const AVPacket,
    ) -> ::std::os::raw::c_int;
    pub fn avcodec_receive_frame(
        avctx: *mut AVCodecContext,
        frame: *mut AVFrame,
    ) -> ::std::os::raw::c_int;

}

// These are our own helpers
extern "C" {
    pub static mut ff_vp6f_decoder_ptr: *mut AVCodec;

    pub fn packet_set_size(pkt :*mut AVPacket, size: i32);
    pub fn packet_data(
        arg1: *mut AVPacket,
    ) -> *mut ::std::os::raw::c_uchar;

    pub fn frame_width(arg1: *mut AVFrame) -> ::std::os::raw::c_int;
    pub fn frame_height(arg1: *mut AVFrame) -> ::std::os::raw::c_int;
    pub fn frame_data(
        arg1: *mut AVFrame,
        arg2: ::std::os::raw::c_int,
    ) -> *mut ::std::os::raw::c_uchar;
    pub fn frame_linesize(arg1: *mut AVFrame, arg2: ::std::os::raw::c_int)
        -> ::std::os::raw::c_int;

    pub fn make_converter_context(yuv_frame: *mut AVFrame) -> *mut SwsContext;
    pub fn convert_yuv_to_rgba(
        context: *mut SwsContext,
        yuv_frame: *mut AVFrame,
        rgba_data: *mut u8,
    );
}

use std::marker::Sized;
use std::mem::size_of;
use std::{alloc::Layout, ptr::write_unaligned};

#[cfg(target_arch = "wasm32")]
unsafe fn wrapped_alloc(size: u32) -> *mut u8 {
    let modified_size = size as usize + 4;
    let info_ptr = std::alloc::alloc(Layout::from_size_align(modified_size, 4).unwrap());
    if info_ptr.is_null() {
        return info_ptr;
    }

    let result_ptr = info_ptr.add(4);

    (info_ptr as *mut u32).write_unaligned(modified_size as u32);

    result_ptr
}

#[cfg(target_arch = "wasm32")]
unsafe fn wrapped_dealloc(ptr: *mut u8) {
    assert!(!ptr.is_null());
    let info_ptr = ptr.sub(4);
    let modified_size = (info_ptr as *mut u32).read_unaligned();
    std::alloc::dealloc(info_ptr, Layout::from_size_align(modified_size as usize, 4).unwrap());
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
fn vp6_custom_malloc(bytes: usize) -> *mut u8 {
    unsafe { wrapped_alloc(bytes as u32) }
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
fn vp6_custom_realloc(ptr: *mut u8, bytes: usize) -> *mut u8 {
    unsafe {
        if ptr.is_null() {
            return vp6_custom_malloc(bytes);
        }

        let info_ptr = ptr.sub(4);
        let old_size = (info_ptr as *mut u32).read_unaligned();
        let new_size = bytes + 4;
        let new_ptr = std::alloc::realloc(
            info_ptr,
            Layout::from_size_align(old_size as usize, 4).unwrap(),
            new_size,
        );

        (new_ptr as *mut u32).write_unaligned(new_size as u32);
        new_ptr.add(4)
    }
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
fn vp6_custom_free(ptr: *mut u8) {
    unsafe {
        if !ptr.is_null() {
            wrapped_dealloc(ptr)
        }
    }
}
