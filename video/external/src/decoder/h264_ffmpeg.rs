use core::ffi;
use std::ffi::{c_char, c_int, c_uchar, c_void, CString};

use crate::decoder::VideoDecoder;
use ruffle_render::bitmap::BitmapFormat;
use ruffle_video::error::Error;
use ruffle_video::frame::{DecodedFrame, EncodedFrame, FrameDependency};

#[repr(C)]
pub struct AVPacket {
    pub buf: *mut c_void,
    pub pts: i64,
    pub dts: i64,
    pub data: *mut u8,
    pub size: c_int,
    pub stream_index: c_int,
    pub flags: c_int,
    pub side_data: *mut c_void,
    pub side_data_elems: c_int,
    pub duration: i64,
    pub pos: i64,
    pub convergence_duration: i64,
}

#[repr(C)]
pub struct AVCodecParameters {
    pub codec_type: i32,
    pub codec_id: u32,
    pub codec_tag: u32,
    pub extradata: *mut u8,
    pub extradata_size: c_int,
    pub format: c_int,
    pub bit_rate: i64,
    pub bits_per_coded_sample: c_int,
    pub bits_per_raw_sample: c_int,
    pub profile: c_int,
    pub level: c_int,
    pub width: c_int,
    pub height: c_int,
    pub sample_aspect_ratio: [c_int; 2],
    pub field_order: u32,
    pub color_range: u32,
    pub color_primaries: u32,
    pub color_trc: u32,
    pub color_space: u32,
    pub chroma_location: u32,
    pub video_delay: c_int,
    pub channel_layout: u64,
    pub channels: c_int,
    pub sample_rate: c_int,
    pub block_align: c_int,
    pub frame_size: c_int,
    pub initial_padding: c_int,
    pub trailing_padding: c_int,
    pub seek_preroll: c_int,
}

#[derive(thiserror::Error, Debug)]
pub enum H264Error {
    /*
    #[error("Picture wasn't found in the video stream")]
    NoPictureInVideoStream,

    #[error("Decoder error")]
    DecoderError(#[from] h263_rs::Error),

    #[error("Invalid picture type code: {0:?}")]
    InvalidPictureType(PictureTypeCode),

    #[error("Picture is missing width and height details")]
    MissingWidthHeight,
    */
}

impl From<H264Error> for Error {
    fn from(error: H264Error) -> Self {
        Error::DecoderError(Box::new(error))
    }
}

/// H264 video decoder.
pub struct H264Decoder {
    is_opened: bool,
    context: *const c_void,
    decoder: *const c_void,
    packet: *mut AVPacket,
    yuv_frame: *const c_void,
    // sws_context: *const c_void,
}

use std::sync::OnceLock;

static LIBAVCODEC: OnceLock<libloading::Library> = OnceLock::new();

struct Ffmpeg {
    av_malloc: libloading::Symbol<'static, unsafe extern "C" fn(usize) -> *mut c_uchar>,
    avcodec_find_decoder_by_name:
        libloading::Symbol<'static, unsafe extern "C" fn(*const c_char) -> *const c_void>,
    avcodec_alloc_context3:
        libloading::Symbol<'static, unsafe extern "C" fn(*const c_void) -> *const c_void>,
    avcodec_open2: libloading::Symbol<
        'static,
        unsafe extern "C" fn(*const c_void, *const c_void, *const c_void) -> *const c_void,
    >,
    av_packet_alloc: libloading::Symbol<'static, unsafe extern "C" fn() -> *mut AVPacket>,
    av_frame_alloc: libloading::Symbol<'static, unsafe extern "C" fn() -> *const c_void>,
    avcodec_send_packet: libloading::Symbol<
        'static,
        unsafe extern "C" fn(*const c_void, *mut AVPacket) -> ffi::c_int,
    >,
    avcodec_receive_frame: libloading::Symbol<
        'static,
        unsafe extern "C" fn(*const c_void, *const c_void) -> ffi::c_int,
    >,
    av_grow_packet:
        libloading::Symbol<'static, unsafe extern "C" fn(*mut AVPacket, c_int) -> ffi::c_int>,
    av_shrink_packet:
        libloading::Symbol<'static, unsafe extern "C" fn(*mut AVPacket, c_int) -> ffi::c_int>,
    avcodec_parameters_alloc:
        libloading::Symbol<'static, unsafe extern "C" fn() -> *mut AVCodecParameters>,
    avcodec_parameters_to_context: libloading::Symbol<
        'static,
        unsafe extern "C" fn(*const c_void, *const AVCodecParameters) -> ffi::c_int,
    >,
}

impl Ffmpeg {
    fn new() -> Ffmpeg {
        unsafe {
            let libavcodec: &libloading::Library =
                LIBAVCODEC.get_or_init(|| libloading::Library::new("libavcodec.so").unwrap());

            let av_malloc: libloading::Symbol<unsafe extern "C" fn(usize) -> *mut c_uchar> =
                libavcodec.get(b"av_malloc").unwrap();
            let avcodec_find_decoder_by_name: libloading::Symbol<
                unsafe extern "C" fn(*const c_char) -> *const c_void,
            > = libavcodec.get(b"avcodec_find_decoder_by_name").unwrap();
            let avcodec_alloc_context3: libloading::Symbol<
                unsafe extern "C" fn(*const c_void) -> *const c_void,
            > = libavcodec.get(b"avcodec_alloc_context3").unwrap();
            let avcodec_open2: libloading::Symbol<
                unsafe extern "C" fn(*const c_void, *const c_void, *const c_void) -> *const c_void,
            > = libavcodec.get(b"avcodec_open2").unwrap();
            let av_packet_alloc: libloading::Symbol<unsafe extern "C" fn() -> *mut AVPacket> =
                libavcodec.get(b"av_packet_alloc").unwrap();
            let av_frame_alloc: libloading::Symbol<unsafe extern "C" fn() -> *const c_void> =
                libavcodec.get(b"av_frame_alloc").unwrap();
            let avcodec_send_packet: libloading::Symbol<
                unsafe extern "C" fn(*const c_void, *mut AVPacket) -> ffi::c_int,
            > = libavcodec.get(b"avcodec_send_packet").unwrap();
            let avcodec_receive_frame: libloading::Symbol<
                unsafe extern "C" fn(*const c_void, *const c_void) -> ffi::c_int,
            > = libavcodec.get(b"avcodec_receive_frame").unwrap();
            let av_grow_packet: libloading::Symbol<
                unsafe extern "C" fn(*mut AVPacket, c_int) -> ffi::c_int,
            > = libavcodec.get(b"av_grow_packet").unwrap();
            let av_shrink_packet: libloading::Symbol<
                unsafe extern "C" fn(*mut AVPacket, c_int) -> ffi::c_int,
            > = libavcodec.get(b"av_shrink_packet").unwrap();
            let avcodec_parameters_alloc: libloading::Symbol<
                unsafe extern "C" fn() -> *mut AVCodecParameters,
            > = libavcodec.get(b"avcodec_parameters_alloc").unwrap();
            let avcodec_parameters_to_context: libloading::Symbol<
                unsafe extern "C" fn(*const c_void, *const AVCodecParameters) -> ffi::c_int,
            > = libavcodec.get(b"avcodec_parameters_to_context").unwrap();

            Ffmpeg {
                av_malloc,
                avcodec_find_decoder_by_name,
                avcodec_alloc_context3,
                avcodec_open2,
                av_packet_alloc,
                av_frame_alloc,
                avcodec_send_packet,
                avcodec_receive_frame,
                av_grow_packet,
                av_shrink_packet,
                avcodec_parameters_alloc,
                avcodec_parameters_to_context,
            }
        }
    }
}

impl H264Decoder {
    pub fn new() -> Self {
        println!("Creating H264 decoder");

        unsafe {
            let ffmpeg = Ffmpeg::new();

            let h264: CString = CString::new("h264").unwrap();
            let h264_decoder = (ffmpeg.avcodec_find_decoder_by_name)(h264.as_ptr());
            let context = (ffmpeg.avcodec_alloc_context3)(h264_decoder);

            let packet = (ffmpeg.av_packet_alloc)();
            let yuv_frame = (ffmpeg.av_frame_alloc)();

            println!("{:#?} {:#?}", packet, yuv_frame);

            Self {
                is_opened: false,
                context,
                decoder: h264_decoder,
                packet,
                yuv_frame,
            }
        }
    }
}

impl VideoDecoder for H264Decoder {
    fn preload_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<FrameDependency, Error> {
        println!("Preloading frame");

        assert_eq!(self.is_opened, false);

        let ffmpeg = Ffmpeg::new();

        unsafe {
            // Create codec parameters and copy the avcC box as extradata
            let codec_params = (ffmpeg.avcodec_parameters_alloc)();

            println!("params alloc'd, copying");

            (*codec_params).extradata = (ffmpeg.av_malloc)(encoded_frame.data.len());

            for i in 0..encoded_frame.data.len() {
                (*codec_params)
                    .extradata
                    .add(i as usize)
                    .write(encoded_frame.data[i]);
            }

            println!("params copied");

            (*codec_params).extradata_size = encoded_frame.data.len() as c_int;

            (*ffmpeg.avcodec_parameters_to_context)(self.context, codec_params);

            (ffmpeg.avcodec_open2)(self.context, self.decoder, std::ptr::null());
        }

        self.is_opened = true;
        Ok(FrameDependency::None)
    }

    fn decode_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<DecodedFrame, Error> {
        println!("Decoding frame");
        assert_eq!(self.is_opened, true);
        let ffmpeg = Ffmpeg::new();
        unsafe {
            let l = (encoded_frame.data.len()) as u32;
            let lp = l + 4;

            if ((*self.packet).size as usize) < (lp as usize) {
                let ret = (ffmpeg.av_grow_packet)(self.packet, lp as c_int - (*self.packet).size);

                if ret != 0 {
                    return Err(Error::DecoderError(
                        format!("av_grow_packet returned: {}", ret).into(),
                    ));
                }
            }

            if ((*self.packet).size as usize) > (lp as usize) {
                let ret = (ffmpeg.av_shrink_packet)(self.packet, lp as c_int);
            }

            (*self.packet).data.add(0).write(0);
            (*self.packet).data.add(1).write((l >> 16) as u8);
            (*self.packet).data.add(2).write((l >> 8) as u8);
            (*self.packet).data.add(3).write(l as u8);
            for i in 0..encoded_frame.data.len() {
                (*self.packet).data.add(i + 4).write(encoded_frame.data[i]);
            }

            let ret = (ffmpeg.avcodec_send_packet)(self.context, self.packet);

            if ret != 0 {
                return Err(Error::DecoderError(
                    format!("avcodec_send_packet returned: {}", ret).into(),
                ));
            }

            let ret = (ffmpeg.avcodec_receive_frame)(self.context, self.yuv_frame);

            if ret != 0 {
                return Err(Error::DecoderError(
                    format!("avcodec_receive_frame returned: {}", ret).into(),
                ));
            }

            Ok(DecodedFrame::new(
                1,
                1,
                BitmapFormat::Rgb,
                vec![255, 0, 255],
            ))
        }
    }
}

impl Default for H264Decoder {
    fn default() -> Self {
        Self::new()
    }
}
