extern crate cc;

fn main() {
    let mut build = cc::Build::new();

    build.files(&[
        "extern/libavcodec/avpacket.c",
        "extern/libavcodec/bitstream_filters.c",
        "extern/libavcodec/bitstream.c",
        "extern/libavcodec/bsf.c",
        "extern/libavcodec/codec_desc.c",
        "extern/libavcodec/decode.c",
        "extern/libavcodec/h264chroma.c",
        "extern/libavcodec/hpeldsp.c",
        "extern/libavcodec/huffman.c",
        "extern/libavcodec/mathtables.c",
        "extern/libavcodec/me_cmp.c",
        "extern/libavcodec/null_bsf.c",
        "extern/libavcodec/options.c",
        "extern/libavcodec/profiles.c",
        "extern/libavcodec/simple_idct.c",
        "extern/libavcodec/utils.c",
        "extern/libavcodec/videodsp.c",
        "extern/libavcodec/vp3dsp.c",
        "extern/libavcodec/vp56.c",
        "extern/libavcodec/vp56data.c",
        "extern/libavcodec/vp56dsp.c",
        "extern/libavcodec/vp56rac.c",
        "extern/libavcodec/vp6.c",
        "extern/libavcodec/vp6dsp.c",
        "extern/libavutil/avstring.c",
        "extern/libavutil/buffer.c",
        "extern/libavutil/channel_layout.c",
        "extern/libavutil/cpu.c",
        "extern/libavutil/dict.c",
        "extern/libavutil/eval.c",
        "extern/libavutil/frame.c",
        "extern/libavutil/hwcontext.c",
        "extern/libavutil/imgutils.c",
        "extern/libavutil/intmath.c",
        "extern/libavutil/log.c",
        "extern/libavutil/mathematics.c",
        "extern/libavutil/mem.c",
        "extern/libavutil/opt.c",
        "extern/libavutil/pixdesc.c",
        "extern/libavutil/rational.c",
        "extern/libavutil/samplefmt.c",
        "extern/libswscale/input.c",
        "extern/libswscale/options.c",
        "extern/libswscale/output.c",
        "extern/libswscale/rgb2rgb.c",
        "extern/libswscale/swscale_unscaled.c",
        "extern/libswscale/swscale.c",
        "extern/libswscale/utils.c",
        "extern/libswscale/yuv2rgb.c",
        "src/helpers.c",
    ]);

    if std::env::var("TARGET").unwrap() == "wasm32-unknown-unknown" {
        build
            .define("MALLOC_PREFIX", "vp6_custom_")
            .include("extern/config-web")
            .include("src/fakelibc")
            .file("src/fakelibc/impl.c");
    } else {
        build.include("extern/config-desktop");
    }

    build
        .define("HAVE_AV_CONFIG_H", None)
        .includes(&["extern", "extern/libavutil", "extern/libavcodec"])
        .warnings(false)
        .extra_warnings(false)
        .flag_if_supported("-Wno-attributes")
        .flag_if_supported("-Wno-discarded-qualifiers")
        .compile("vp6");
}
