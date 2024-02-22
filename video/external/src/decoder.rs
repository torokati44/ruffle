// bindgen ../openh264/codec/api/wels/codec_api.h --no-prepend-enum-name \
//         --dynamic-loading OpenH264 -o openh264_sys.rs
#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(dead_code)]
mod openh264_sys;

pub mod openh264;

pub use ruffle_video_software::decoder::VideoDecoder;
