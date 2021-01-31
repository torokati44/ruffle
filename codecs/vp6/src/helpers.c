// The wasm32-unknown-unknown ABI is not always in perfect match with the
// Rust one (even with it trying to be C-like), mostly regarding struct data layout.
// These little helpers are used to avoid having to pass structures by value across
// the FFI boundary, or having to access any C struct members from Rust.
// Trivial types like integers, floating point numbers, and pointers, should be fine.

#include "libavutil/common.h"
#include "libavcodec/avcodec.h"
#include "libswscale/swscale.h"

extern AVCodec ff_vp6f_decoder;
AVCodec *ff_vp6f_decoder_ptr = &ff_vp6f_decoder;

void packet_set_size(AVPacket *p, int size) {
    if (p->size < size) {
        av_grow_packet(p, size - p->size);
    }
    else if (p->size > size) {
        av_shrink_packet(p, size);
    }
}

unsigned char *packet_data(AVPacket *p) {
    return p->data;
}

int frame_width(AVFrame *f) {
    return f->width;
}
int frame_height(AVFrame *f) {
    return f->height;
}

typedef struct SwsContext SwsContext;

SwsContext *make_converter_context(AVFrame *yuv_frame) {
    return sws_getContext (
        yuv_frame->width, yuv_frame->height, AV_PIX_FMT_YUV420P,
        yuv_frame->width, yuv_frame->height, AV_PIX_FMT_RGBA,
        SWS_BICUBIC, NULL, NULL, NULL );
}

void convert_yuv_to_rgba(SwsContext *context, AVFrame *yuv_frame, uint8_t *rgba_data) {
    int linesize = yuv_frame->linesize[0] * 4;
    sws_scale (context, yuv_frame->data, yuv_frame->linesize, 0,
        yuv_frame->height, &rgba_data, &linesize);
}