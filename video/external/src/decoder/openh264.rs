
use std::ffi::{c_uchar, c_int};
use std::ptr;

use crate::decoder::VideoDecoder;
use crate::decoder::openh264_sys::{self, OpenH264, ISVCDecoder, ISVCDecoderVtbl};

use ruffle_render::bitmap::BitmapFormat;
use ruffle_video::error::Error;
use ruffle_video::frame::{DecodedFrame, EncodedFrame, FrameDependency};




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
    length_size: u8, // how many bytes are used to store the length of the NALU (1, 2, 3, or 4)

    openh264: OpenH264,
    decoder: *mut ISVCDecoder,
}


impl H264Decoder {
    /// `extradata` should hold "AVCC (MP4) format" decoder configuration, including PPS and SPS.
    /// Make sure it has any start code emulation prevention "three bytes" removed.
    pub fn new(/*extradata: &[u8]*/) -> Self {

        let extradata = [1, 1, 1, 1, 3];

        assert!(extradata[0] == 1); // configuration version, always 1
        // extradata[1]: profile
        // extradata[2]: compatibility
        // extradata[3]: level
        // extradata[4]: 6 reserved bits | NALU length size - 1
        let length_size = (extradata[4] & 0b0000_0011) + 1;



        let mut decoder: *mut ISVCDecoder = ptr::null_mut();
        unsafe {
            let openh264 = OpenH264::new("./libopenh264-2.4.0-linux64.7.so").unwrap();


            openh264.WelsCreateDecoder(&mut decoder);


            let decoder_vtbl = (*decoder as *const ISVCDecoderVtbl)
            .as_ref()
            .unwrap();


            let mut dec_param: openh264_sys::SDecodingParam = std::mem::zeroed();
            dec_param.sVideoProperty.eVideoBsType = openh264_sys::VIDEO_BITSTREAM_AVC;

            (decoder_vtbl.Initialize.unwrap())(decoder, &dec_param);


            Self {
                length_size,
                openh264,
                decoder,
            }
        }
    }
}

impl Drop for H264Decoder {
    fn drop(&mut self) {
        unsafe {
            let decoder_vtbl = (*self.decoder as *const ISVCDecoderVtbl)
                .as_ref()
                .unwrap();

            (decoder_vtbl.Uninitialize.unwrap())(self.decoder);
            self.openh264.WelsDestroyDecoder(self.decoder);
        }
    }
}

impl VideoDecoder for H264Decoder {
    fn preload_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<FrameDependency, Error> {
        println!("Preloading frame");

        dbg!(encoded_frame.data);
        unsafe {
            let decoder_vtbl = (*self.decoder as *const ISVCDecoderVtbl)
            .as_ref()
            .unwrap();

            //input: encoded bitstream start position; should include start code prefix
            let mut buffer: Vec<c_uchar> = Vec::new();

            buffer.extend_from_slice(&[0, 0, 0, 1]);

            let sps_length = encoded_frame.data[6] as usize * 256 + encoded_frame.data[7] as usize;

            dbg!(sps_length);

            for i in 0..sps_length {
                buffer.push(encoded_frame.data[8 + i]);
            }

            let num_pps = encoded_frame.data[8 + sps_length] as usize;

            assert!(num_pps == 1);

            buffer.extend_from_slice(&[0, 0, 0, 1]);

            let pps_length = encoded_frame.data[8 + sps_length + 1] as usize * 256 + encoded_frame.data[8 + sps_length + 2] as usize;

            dbg!(pps_length);

            for i in 0..pps_length {
                buffer.push(encoded_frame.data[8 + sps_length + 3 + i]);
            }

            dbg!(&buffer);


            //output: [0~2] for Y,U,V buffer for Decoding only
            let mut output = [ptr::null_mut() as *mut c_uchar; 3];
            //in-out: for Decoding only: declare and initialize the output buffer info
            let mut dest_buf_info: openh264_sys::SBufferInfo = std::mem::zeroed();

            let ret = decoder_vtbl.DecodeFrameNoDelay.unwrap()(
                self.decoder,
                buffer.as_mut_ptr(),
                buffer.len() as c_int,
                output.as_mut_ptr(),
                &mut dest_buf_info as *mut openh264_sys::SBufferInfo,
            );

            dbg!(ret);

        }

        Ok(FrameDependency::None)

        /*


        let nal_unit_type = encoded_frame.data[self.length_size as usize] & 0b0001_1111;

        if nal_unit_type == openh264_sys::NAL_SLICE_IDR as u8 { // 5
            Ok(FrameDependency::None)
        }
        else {
            Ok(FrameDependency::Past)
        }*/
    }

    fn decode_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<DecodedFrame, Error> {
        println!("Decoding frame");


        unsafe {
            let decoder_vtbl = (*self.decoder as *const ISVCDecoderVtbl)
                .as_ref()
                .unwrap();


            println!("{}, {:?}", encoded_frame.data.len(), &encoded_frame.data[0..10]);



        //input: encoded bitstream start position; should include start code prefix
        let mut buffer: Vec<c_uchar> = Vec::with_capacity(encoded_frame.data.len() - self.length_size as usize + 4);

        buffer.extend_from_slice(&[0, 0, 0, 1]);
        buffer.extend_from_slice(&encoded_frame.data[self.length_size as usize..]);

        //output: [0~2] for Y,U,V buffer for Decoding only
        let mut output = [ptr::null_mut() as *mut c_uchar; 3];
        //in-out: for Decoding only: declare and initialize the output buffer info
        let mut dest_buf_info: openh264_sys::SBufferInfo = std::mem::zeroed();

        let ret = decoder_vtbl.DecodeFrameNoDelay.unwrap()(
            self.decoder,
            buffer.as_mut_ptr(),
            buffer.len() as c_int,
            output.as_mut_ptr(),
            &mut dest_buf_info as *mut openh264_sys::SBufferInfo,
        );

        dbg!(ret);
        //decode failed
        if ret != 0 {
            //RequestIDR or something like that.
            return Ok(DecodedFrame::new(
                1,
                1,
                BitmapFormat::Rgb,
                vec![255, 0, 255],
            ));
        }
        //for Decoding only, pData can be used for render.
        if dest_buf_info.iBufferStatus == 1 {
            //output pData[0], pData[1], pData[2];
        }
        let buffer_info = dest_buf_info.UsrData.sSystemBuffer;
        dbg!(buffer_info);

        let mut yuv: Vec<u8> = Vec::with_capacity(buffer_info.iWidth as usize * buffer_info.iHeight as usize * 3 / 2);

        for i in 0..buffer_info.iHeight {
            for j in 0..buffer_info.iWidth {
                yuv.push(*output[0].offset((i * buffer_info.iStride[0] + j) as isize));
            }
        }

        for i in 0..buffer_info.iHeight / 2 {
            for j in 0..buffer_info.iWidth / 2 {
                yuv.push(*output[1].offset((i * buffer_info.iStride[1] + j) as isize));
            }
        }

        for i in 0..buffer_info.iHeight / 2 {
            for j in 0..buffer_info.iWidth / 2 {
                yuv.push(*output[2].offset((i * buffer_info.iStride[1] + j) as isize));
            }
        }


            Ok(DecodedFrame::new(
                buffer_info.iWidth as u32,
                buffer_info.iHeight as u32,
                BitmapFormat::Yuv420p,
                yuv,
            ))
        }
    }
}
