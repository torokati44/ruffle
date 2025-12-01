use std::sync::Arc;

use crate::decoder::VideoDecoder;
#[cfg(feature = "openh264")]
use crate::decoder::openh264::OpenH264Codec;

use ruffle_render::backend::RenderBackend;
use ruffle_render::bitmap::{BitmapHandle, BitmapInfo, PixelRegion};
use ruffle_video::VideoStreamHandle;
use ruffle_video::backend::VideoBackend;
use ruffle_video::error::Error;
use ruffle_video::frame::{EncodedFrame, FrameDependency};
use ruffle_video_software::backend::SoftwareVideoBackend;
use slotmap::SlotMap;

use swf::{VideoCodec, VideoDeblocking};

// Just to avoid the several conditional imports.
#[cfg(feature = "webcodecs")]
type LogSubscriberArc = std::sync::Arc<
    tracing_subscriber::layer::Layered<tracing_wasm::WASMLayer, tracing_subscriber::Registry>,
>;

enum ProxyOrStream {
    /// These streams are passed through to the wrapped software
    /// backend, accessed using the stored ("inner") handle,
    /// which is completely internal to this backend.
    Proxied(VideoStreamHandle),

    /// These streams are handled by this backend directly.
    Owned(VideoStream),
}

/// A video backend that falls back to the software backend for most codecs,
/// except for H.264, for which it uses an external decoder.
pub struct ExternalVideoBackend {
    streams: SlotMap<VideoStreamHandle, ProxyOrStream>,
    #[cfg(feature = "openh264")]
    openh264_codec: Option<OpenH264Codec>,
    // Needed for the callbacks in the `webcodecs` backend.
    #[cfg(feature = "webcodecs")]
    log_subscriber: Option<LogSubscriberArc>,
    software: SoftwareVideoBackend,
}

impl Default for ExternalVideoBackend {
    fn default() -> Self {
        Self::new()
    }
}

use hwdec_vaapi::{HWDecoder, HWWrapper, VaapiH264Decoder};
use nihav_core::codecs::DecoderResult;
use nihav_core::frame::{NACodecInfo, NACodecInfoRef, NAPacket, NAStream, NAStreamRef, NATimeInfo, NAVideoInfo, YUV420_FORMAT};

impl ExternalVideoBackend {



    fn decode_h264() -> DecoderResult<()> {

        let mut dec = VaapiH264Decoder::new();

        //dec.init(stream.get_info()).expect("inited");
        //vstream_id = stream.get_id();

        let mut hw = HWWrapper::new();
        let videoinfo = NAVideoInfo::new(1920, 1080, false, YUV420_FORMAT);
        let info = NACodecInfo::new("h264", nihav_core::frame::NACodecTypeInfo::Video(videoinfo), Some(vec![]));
        let pkt = NAPacket::new(NAStreamRef::new(NAStream::new(nihav_core::frame::StreamType::Video, 0, info.clone(), 0, 0, 0)), NATimeInfo::new(None, None, None, 0, 0), false, vec![]);
        hw.init(NACodecInfoRef::new(info));
        hw.queue_pkt(&pkt);
        //hw.get_frame().expect("got frame");
        //hw.get_last_frames().expect("got last frames");
/*
        let frm = hw.get_last_frames().expect("get frame");
        let timestamp = frm.get_dts().unwrap_or_else(|| frm.get_pts().unwrap_or(0));

        let pic = frm.get_buffer().get_vbuf().expect("got picture");

        //let nname = format!("assets/test_out/{}{:06}_{}.pgm", opfx, timestamp, frameno);
        //frameno += 1;
        //let mut file = std::fs::File::create(&nname).expect("create file");
        let (w, h) = pic.get_dimensions(0);
        //file.write_all(format!("P5\n{} {}\n255\n", w, h * 3 / 2).as_bytes()).expect("header written");
        let data = pic.get_data();
        for yline in data.chunks(pic.get_stride(0)).take(h) {
            //file.write_all(&yline[..w]).expect("Y line written");
        }
        for (uline, vline) in data[pic.get_offset(1)..].chunks(pic.get_stride(1))
                .zip(data[pic.get_offset(2)..].chunks(pic.get_stride(2))).take(h / 2) {
            //file.write_all(&uline[..w / 2]).expect("U line written");
            //file.write_all(&vline[..w / 2]).expect("V line written");
        }
*/
        Ok(())
    }



    fn make_decoder(&mut self) -> Result<Box<dyn VideoDecoder>, Error> {

        #[cfg(feature = "openh264")]
        if let Some(h264_codec) = self.openh264_codec.as_ref() {
            let decoder = Box::new(crate::decoder::openh264::H264Decoder::new(h264_codec));
            return Ok(decoder);
        }

        #[cfg(feature = "webcodecs")]
        {
            let log_subscriber = self
                .log_subscriber
                .clone()
                .ok_or(Error::DecoderError("log subscriber not set".into()))?;
            let decoder = crate::decoder::webcodecs::H264Decoder::new(log_subscriber);
            return decoder.map(|decoder| Box::new(decoder) as Box<dyn VideoDecoder>);
        }

        #[allow(unreachable_code)]
        Err(Error::DecoderError("No H.264 decoder available".into()))
    }

    // Neither the `openh264` nor the `webcodecs` backend will be available.
    pub fn new() -> Self {
        Self::decode_h264().expect("decoded");
        Self {
            streams: SlotMap::with_key(),
            #[cfg(feature = "openh264")]
            openh264_codec: None,
            #[cfg(feature = "webcodecs")]
            log_subscriber: None,
            software: SoftwareVideoBackend::new(),
        }
    }

    #[cfg(feature = "openh264")]
    pub fn new_with_openh264(openh264_codec: OpenH264Codec) -> Self {
        Self::decode_h264().expect("decoded");
        Self {
            streams: SlotMap::with_key(),
            openh264_codec: Some(openh264_codec),
            #[cfg(feature = "webcodecs")]
            log_subscriber: None,
            software: SoftwareVideoBackend::new(),
        }
    }

    #[cfg(feature = "webcodecs")]
    pub fn new_with_webcodecs(log_subscriber: LogSubscriberArc) -> Self {
        Self {
            streams: SlotMap::with_key(),
            #[cfg(feature = "openh264")]
            openh264_codec: None,
            log_subscriber: Some(log_subscriber),
            software: SoftwareVideoBackend::new(),
        }
    }
}

// NOTE: The stream handles coming in through this API must not be
// conflated with the ones stored in `streams` as `Proxied`.
impl VideoBackend for ExternalVideoBackend {
    fn register_video_stream(
        &mut self,
        num_frames: u32,
        size: (u16, u16),
        codec: VideoCodec,
        filter: VideoDeblocking,
    ) -> Result<VideoStreamHandle, Error> {
        let proxy_or_stream = if codec == VideoCodec::H264 {
            let decoder = self.make_decoder()?;
            let stream = VideoStream::new(decoder);
            ProxyOrStream::Owned(stream)
        } else {
            ProxyOrStream::Proxied(
                self.software
                    .register_video_stream(num_frames, size, codec, filter)?,
            )
        };

        Ok(self.streams.insert(proxy_or_stream))
    }

    fn configure_video_stream_decoder(
        &mut self,
        stream: VideoStreamHandle,
        configuration_data: &[u8],
    ) -> Result<(), Error> {
        let stream = self
            .streams
            .get_mut(stream)
            .ok_or(Error::VideoStreamIsNotRegistered)?;

        match stream {
            ProxyOrStream::Proxied(handle) => self
                .software
                .configure_video_stream_decoder(*handle, configuration_data),
            ProxyOrStream::Owned(stream) => stream.decoder.configure_decoder(configuration_data),
        }
    }

    fn preload_video_stream_frame(
        &mut self,
        stream: VideoStreamHandle,
        encoded_frame: EncodedFrame<'_>,
    ) -> Result<FrameDependency, Error> {
        let stream = self
            .streams
            .get_mut(stream)
            .ok_or(Error::VideoStreamIsNotRegistered)?;

        match stream {
            ProxyOrStream::Proxied(handle) => self
                .software
                .preload_video_stream_frame(*handle, encoded_frame),
            ProxyOrStream::Owned(stream) => stream.decoder.preload_frame(encoded_frame),
        }
    }

    fn decode_video_stream_frame(
        &mut self,
        stream: VideoStreamHandle,
        encoded_frame: EncodedFrame<'_>,
        renderer: &mut dyn RenderBackend,
    ) -> Result<BitmapInfo, Error> {
        let stream = self
            .streams
            .get_mut(stream)
            .ok_or(Error::VideoStreamIsNotRegistered)?;

        match stream {
            ProxyOrStream::Proxied(handle) => {
                self.software
                    .decode_video_stream_frame(*handle, encoded_frame, renderer)
            }
            ProxyOrStream::Owned(stream) => {
                let frame = stream.decoder.decode_frame(encoded_frame)?;

                let width = frame.width();
                let height = frame.height();

                let handle = if let Some(bitmap) = stream.bitmap.clone() {
                    renderer.update_texture(
                        &bitmap,
                        frame,
                        PixelRegion::for_whole_size(width, height),
                    )?;
                    bitmap
                } else {
                    renderer.register_bitmap(frame)?
                };
                stream.bitmap = Some(handle.clone());

                Ok(BitmapInfo {
                    handle,
                    width,
                    height,
                })
            }
        }
    }
}

/// A single preloaded video stream.
pub struct VideoStream {
    bitmap: Option<BitmapHandle>,
    decoder: Box<dyn VideoDecoder>,
}

impl VideoStream {
    fn new(decoder: Box<dyn VideoDecoder>) -> Self {
        Self {
            decoder,
            bitmap: None,
        }
    }
}
