extern crate cc;

use std::env;

use env::var;

fn main() {
    let mut build = cc::Build::new();

    build.files(&[
        "extern/libavutil/frame.c",
        "extern/libavcodec/vp56.c",
        "extern/libavcodec/vp56data.c",
        "extern/libavcodec/vp56dsp.c",
        "extern/libavcodec/vp56rac.c",
        "extern/libavcodec/vp6.c",
        "extern/libavcodec/h264chroma.c",
        "extern/libavcodec/mathtables.c",
        "extern/libavcodec/utils.c",
        "extern/libavutil/samplefmt.c",
        "extern/libavutil/channel_layout.c",
        "extern/libavutil/imgutils.c",
        "extern/libavutil/buffer.c",
        "extern/libavutil/dict.c",
        "extern/libavutil/mem.c",
        "extern/libavutil/pixdesc.c",
        "extern/libavcodec/videodsp.c",
        "extern/libavcodec/hpeldsp.c",
        "extern/libavcodec/vp3dsp.c",
        "extern/libavcodec/vp6dsp.c",
        "extern/libavcodec/bitstream.c",
        "extern/libavcodec/huffman.c",
        "extern/libavcodec/me_cmp.c",
        "extern/libavutil/intmath.c",
        "extern/libavutil/cpu.c",
        "extern/libavcodec/avpacket.c",
        "extern/libavutil/opt.c",
        "extern/libavutil/log.c",
        "extern/libavcodec/codec_desc.c",
        "extern/libavutil/avstring.c",
        "extern/libavutil/mathematics.c",
        "extern/libavutil/rational.c",
        "extern/libavutil/hwcontext.c",
        "extern/libavcodec/profiles.c",
        "extern/libavcodec/simple_idct.c",
        "extern/libavcodec/decode.c",
        "extern/libavcodec/bsf.c",
        "extern/libavcodec/bitstream_filters.c",
        "extern/libavutil/eval.c",
        "extern/libavcodec/options.c",
        "extern/libavcodec/null_bsf.c",
        "extern/libswscale/swscale.c",
        "extern/libswscale/swscale_unscaled.c",
        "extern/libswscale/utils.c",
        "extern/libswscale/rgb2rgb.c",
        "extern/libswscale/yuv2rgb.c",
        "extern/libswscale/output.c",
        "extern/libswscale/options.c",
        "extern/libswscale/input.c",
        "src/helpers.c",
    ]);

    if std::env::var("TARGET").unwrap() == "wasm32-unknown-unknown" {
        build.include("extern/config-web");
        build.include("src/fakelibc");
        build.file("src/fakelibc/impl.c");
        build.define("MALLOC_PREFIX", "vp6_custom_");
    } else {
        build.include("extern/config-desktop");
    }

    build
        .define("HAVE_AV_CONFIG_H", None)
        .includes(&["extern", "extern/libavutil", "extern/libavcodec"])
        .warnings(false)
        .extra_warnings(false)
        .compile("vp6");
}
