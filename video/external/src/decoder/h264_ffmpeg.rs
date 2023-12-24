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
pub struct AVFrame {
    pub data: [*mut u8; 8],
    pub linesize: [c_int; 8],
    pub extended_data: *mut *mut u8,
    pub width: c_int,
    pub height: c_int,
    pub nb_samples: c_int,
    pub format: c_int,
    pub key_frame: c_int,
    pub pict_type: u32,
    pub sample_aspect_ratio: [c_int; 2],
    pub pts: i64,
    pub pkt_pts: i64,
    pub pkt_dts: i64,
    pub coded_picture_number: c_int,
    pub display_picture_number: c_int,
    pub quality: c_int,
    pub opaque: *mut c_void,
    pub error: [u64; 8],
    pub repeat_pict: c_int,
    pub interlaced_frame: c_int,
    pub top_field_first: c_int,
    pub palette_has_changed: c_int,
    pub reordered_opaque: i64,
    pub sample_rate: c_int,
    pub channel_layout: u64,
    pub buf: [*mut c_void; 8],
    pub extended_buf: *mut *mut c_void,
    pub nb_extended_buf: c_int,
    pub side_data: *mut *mut c_void,
    pub nb_side_data: c_int,
    pub flags: c_int,
    pub color_range: u32,
    pub color_primaries: u32,
    pub color_trc: u32,
    pub colorspace: u32,
    pub chroma_location: u32,
    pub best_effort_timestamp: i64,
    pub pkt_pos: i64,
    pub pkt_duration: i64,
    pub metadata: *mut c_void,
    pub decode_error_flags: c_int,
    pub channels: c_int,
    pub pkt_size: c_int,
    pub qscale_table: *mut i8,
    pub qstride: c_int,
    pub qscale_type: c_int,
    pub qp_table_buf: *mut c_void,
    pub hw_frames_ctx: *mut c_void,
    pub opaque_ref: *mut c_void,
    pub crop_top: usize,
    pub crop_bottom: usize,
    pub crop_left: usize,
    pub crop_right: usize,
    pub private_ref: *mut c_void,
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
    yuv_frame: *const AVFrame,
    sws_context: *mut c_void,
}

use std::sync::OnceLock;

static LIBAVCODEC: OnceLock<libloading::Library> = OnceLock::new();
static LIBSWSCALE: OnceLock<libloading::Library> = OnceLock::new();

struct Ffmpeg {
    av_malloc: libloading::Symbol<'static, unsafe extern "C" fn(usize) -> *mut c_uchar>,
    av_log_set_level: libloading::Symbol<'static, unsafe extern "C" fn(ffi::c_int) -> ffi::c_int>,
    avcodec_find_decoder_by_name:
        libloading::Symbol<'static, unsafe extern "C" fn(*const c_char) -> *const c_void>,
    avcodec_alloc_context3:
        libloading::Symbol<'static, unsafe extern "C" fn(*const c_void) -> *const c_void>,
    avcodec_open2: libloading::Symbol<
        'static,
        unsafe extern "C" fn(*const c_void, *const c_void, *const c_void) -> *const c_void,
    >,
    av_packet_alloc: libloading::Symbol<'static, unsafe extern "C" fn() -> *mut AVPacket>,
    av_frame_alloc: libloading::Symbol<'static, unsafe extern "C" fn() -> *const AVFrame>,
    avcodec_send_packet: libloading::Symbol<
        'static,
        unsafe extern "C" fn(*const c_void, *mut AVPacket) -> ffi::c_int,
    >,
    avcodec_receive_frame: libloading::Symbol<
        'static,
        unsafe extern "C" fn(*const c_void, *const AVFrame) -> ffi::c_int,
    >,
    av_grow_packet:
        libloading::Symbol<'static, unsafe extern "C" fn(*mut AVPacket, c_int) -> ffi::c_int>,
    av_shrink_packet:
        libloading::Symbol<'static, unsafe extern "C" fn(*mut AVPacket, c_int) -> ffi::c_void>,
    avcodec_parameters_alloc:
        libloading::Symbol<'static, unsafe extern "C" fn() -> *mut AVCodecParameters>,
    avcodec_parameters_to_context: libloading::Symbol<
        'static,
        unsafe extern "C" fn(*const c_void, *const AVCodecParameters) -> ffi::c_int,
    >,
    #[allow(non_snake_case)]
    sws_getContext: libloading::Symbol<
        'static,
        unsafe extern "C" fn(
            c_int,
            c_int,
            c_int,
            c_int,
            c_int,
            c_int,
            c_int,
            *const c_void,
            *const c_void,
            *const c_void,
        ) -> *mut c_void,
    >,
    sws_scale: libloading::Symbol::<
        'static,
        unsafe extern "C" fn(
            *mut c_void,
            *const *const u8,
            *const c_int,
            c_int,
            c_int,
            *mut *mut u8,
            *const c_int,
        ) -> c_int,
    >,
}

impl Ffmpeg {
    fn new() -> Ffmpeg {
        unsafe {
            let libavcodec: &libloading::Library =
                LIBAVCODEC.get_or_init(|| libloading::Library::new("libavcodec.so").unwrap());

            let libswscale: &libloading::Library =
                LIBSWSCALE.get_or_init(|| libloading::Library::new("libswscale.so").unwrap());

            let av_malloc: libloading::Symbol<unsafe extern "C" fn(usize) -> *mut c_uchar> =
                libavcodec.get(b"av_malloc").unwrap();
            let av_log_set_level: libloading::Symbol<
                unsafe extern "C" fn(ffi::c_int) -> ffi::c_int,
            > = libavcodec.get(b"av_log_set_level").unwrap();
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
            let av_frame_alloc: libloading::Symbol<unsafe extern "C" fn() -> *const AVFrame> =
                libavcodec.get(b"av_frame_alloc").unwrap();
            let avcodec_send_packet: libloading::Symbol<
                unsafe extern "C" fn(*const c_void, *mut AVPacket) -> ffi::c_int,
            > = libavcodec.get(b"avcodec_send_packet").unwrap();
            let avcodec_receive_frame: libloading::Symbol<
                unsafe extern "C" fn(*const c_void, *const AVFrame) -> ffi::c_int,
            > = libavcodec.get(b"avcodec_receive_frame").unwrap();
            let av_grow_packet: libloading::Symbol<
                unsafe extern "C" fn(*mut AVPacket, c_int) -> ffi::c_int,
            > = libavcodec.get(b"av_grow_packet").unwrap();
            let av_shrink_packet: libloading::Symbol<
                unsafe extern "C" fn(*mut AVPacket, c_int) -> ffi::c_void,
            > = libavcodec.get(b"av_shrink_packet").unwrap();
            let avcodec_parameters_alloc: libloading::Symbol<
                unsafe extern "C" fn() -> *mut AVCodecParameters,
            > = libavcodec.get(b"avcodec_parameters_alloc").unwrap();
            let avcodec_parameters_to_context: libloading::Symbol<
                unsafe extern "C" fn(*const c_void, *const AVCodecParameters) -> ffi::c_int,
            > = libavcodec.get(b"avcodec_parameters_to_context").unwrap();

            #[allow(non_snake_case)]
            let sws_getContext: libloading::Symbol<
                unsafe extern "C" fn(
                    c_int,
                    c_int,
                    c_int,
                    c_int,
                    c_int,
                    c_int,
                    c_int,
                    *const c_void,
                    *const c_void,
                    *const c_void,
                ) -> *mut c_void,
            > = libswscale.get(b"sws_getContext").unwrap();

            let sws_scale: libloading::Symbol::<
                unsafe extern "C" fn(
                    *mut c_void,
                    *const *const u8,
                    *const c_int,
                    c_int,
                    c_int,
                    *mut *mut u8,
                    *const c_int,
                ) -> c_int,
            > = libswscale.get(b"sws_scale").unwrap();


            Ffmpeg {
                av_malloc,
                av_log_set_level,
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
                sws_getContext,
                sws_scale,
            }
        }
    }
}

impl H264Decoder {
    pub fn new() -> Self {
        println!("Creating H264 decoder");

        unsafe {
            let ffmpeg = Ffmpeg::new();

            let h264: CString = CString::new("libopenh264").unwrap();
            let h264_decoder = (ffmpeg.avcodec_find_decoder_by_name)(h264.as_ptr());

            println!("{:#?}", h264_decoder);
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
                sws_context: std::ptr::null_mut(),
            }
        }
    }
}

impl VideoDecoder for H264Decoder {
    fn preload_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<FrameDependency, Error> {
        println!("Preloading frame");

        assert!(!self.is_opened);

        let ffmpeg = Ffmpeg::new();

        unsafe {
            (ffmpeg.av_log_set_level)(56);

            // Create codec parameters and copy the avcC box as extradata
            let codec_params = (ffmpeg.avcodec_parameters_alloc)();

            println!("params alloc'd, copying");

            (*codec_params).extradata = (ffmpeg.av_malloc)(encoded_frame.data.len());

            for i in 0..encoded_frame.data.len() {
                (*codec_params)
                    .extradata
                    .add(i)
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
        assert!(self.is_opened);
        let ffmpeg = Ffmpeg::new();
        unsafe {
            let l = (encoded_frame.data.len()) as u32;

            println!("{}, {:?}", l, &encoded_frame.data[0..10]);

            if ((*self.packet).size as usize) < (l as usize) {
                let ret = (ffmpeg.av_grow_packet)(self.packet, l as c_int - (*self.packet).size);

                if ret != 0 {
                    return Err(Error::DecoderError(
                        format!("av_grow_packet returned: {}", ret).into(),
                    ));
                }
            }

            if ((*self.packet).size as usize) > (l as usize) {
                (ffmpeg.av_shrink_packet)(self.packet, l as c_int);
            }

            for i in 0..encoded_frame.data.len() {
                (*self.packet).data.add(i).write(encoded_frame.data[i]);
            }

            let ret = (ffmpeg.avcodec_send_packet)(self.context, self.packet);

            if ret != 0 {
                return Err(Error::DecoderError(
                    format!("avcodec_send_packet returned: {}", ret).into(),
                ));
            }

            let ret = (ffmpeg.avcodec_receive_frame)(self.context, self.yuv_frame);

            if ret != 0 {
                println!("avcodec_receive_frame returned: {}", ret);
                return Ok(DecodedFrame::new(
                    320,
                    240,
                    BitmapFormat::Rgb,
                    vec![127; 320 * 240 * 3],
                ));
            }

            println!("getting sizes");

            let h = (*self.yuv_frame).height as usize;
            let w = (*self.yuv_frame).width as usize;
            let ls = (*self.yuv_frame).linesize[0] as usize;

            const AV_PIX_FMT_YUV420P: c_int = 0;
            const AV_PIX_FMT_RGB24: c_int = 2;
            const SWS_BICUBIC: c_int = 4;

            if self.sws_context == std::ptr::null_mut() {
                println!("creating sws context");
                self.sws_context = (ffmpeg.sws_getContext)(
                    w as c_int,
                    h as c_int,
                    AV_PIX_FMT_YUV420P,
                    w as c_int,
                    h as c_int,
                    AV_PIX_FMT_RGB24,
                    SWS_BICUBIC,
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                );
                println!("created sws context");
            }


            let mut ls2: c_int = 320*4;
            let mut rgb_data = vec![0u8; ls * h * 3];
            let mut rgb_data_ptr = rgb_data.as_ptr();

            let mut ls2s = vec![ls2; 1];
            let mut rgb_data_ptrs = vec![rgb_data_ptr; 1];

            /*
            println!("scaling");

            (ffmpeg.sws_scale)(
                self.sws_context,
                std::mem::transmute((*self.yuv_frame).data.as_ptr()),
                (*self.yuv_frame).linesize.as_ptr(),
                0,
                h as c_int,
                std::mem::transmute(rgb_data_ptrs.as_mut_ptr()),
                ls2s.as_ptr(),
            );
            println!("scaled");
            */


            let c_w = (w+1) / 2;
            let c_h = (h+1) / 2;

            let mut data = Vec::with_capacity(w * h + 2 * c_w * c_h);

            for y in 0..h {
                for x in 0..w {
                    let i = y * (*self.yuv_frame).linesize[0] as usize + x;
                    data.push(*(*self.yuv_frame).data[0].add(i));
                    //data.push(*(*self.yuv_frame).data[0].add(i));
                    //data.push(*(*self.yuv_frame).data[0].add(i));
                    //data.push(*(*self.yuv_frame).data[0].add(i));
                }
            }

            for y in 0..c_h {
                for x in 0..c_w {
                    let i = y * (*self.yuv_frame).linesize[1] as usize + x;
                    data.push(*(*self.yuv_frame).data[1].add(i
                    ));
                }
            }

            for y in 0..c_h {
                for x in 0..c_w {
                    let i = y * (*self.yuv_frame).linesize[2] as usize + x;
                    data.push(*(*self.yuv_frame).data[2].add(i));
                }
            }

            Ok(DecodedFrame::new(
                w as u32,
                h as u32,
                BitmapFormat::Yuv420p,
                data,
            ))
        }
    }
}

impl Default for H264Decoder {
    fn default() -> Self {
        Self::new()
    }
}
