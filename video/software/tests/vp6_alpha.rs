#![cfg(feature = "vp6")]

use flv_rs::{CodecId, FlvReader, Header, Tag, TagData, VideoPacket};
use ruffle_render::bitmap::BitmapFormat;
use ruffle_video::frame::EncodedFrame;
use ruffle_video_software::decoder::vp6::Vp6Decoder;
use ruffle_video_software::decoder::VideoDecoder;
use swf::VideoCodec;

const FIXTURE: &[u8] = include_bytes!("fixtures/vp6_alpha_one_frame.flv");

#[test]
fn decodes_vp6_alpha_keyframe() {
    let mut reader = FlvReader::from_source(FIXTURE);
    Header::parse(&mut reader).unwrap();

    let tag = Tag::parse(&mut reader).unwrap();
    let TagData::Video(video) = tag.data else { panic!() };
    assert_eq!(video.codec_id, CodecId::On2Vp6Alpha);
    let VideoPacket::Vp6Data { data, .. } = video.data else { panic!() };

    let mut decoder = Vp6Decoder::new(true, (976, 400));
    let frame = decoder
        .decode_frame(EncodedFrame {
            codec: VideoCodec::Vp6WithAlpha,
            data,
            frame_id: 0,
        })
        .unwrap();

    assert_eq!(frame.width(), 976);
    assert_eq!(frame.height(), 400);
    assert_eq!(frame.format(), BitmapFormat::Yuva420p);

    let luma_len = 976 * 400;
    let chroma_len = (976 / 2) * (400 / 2);
    assert_eq!(frame.data().len(), 2 * luma_len + 2 * chroma_len);

    let a_plane = &frame.data()[luma_len + 2 * chroma_len..];
    assert!(a_plane.iter().any(|&v| v > 0));
}
