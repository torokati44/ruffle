

#include "libavutil/common.h"
#include "libavcodec/avcodec.h"
#include "libswscale/swscale.h"

extern AVCodec ff_vp6f_decoder;

AVCodec *ff_vp6f_decoder_ptr = &ff_vp6f_decoder;


int frame_width(AVFrame *f) {
    return f->width;
}
int frame_height(AVFrame *f) {
    return f->height;
}
unsigned char *frame_data(AVFrame *f, int i) {
    return f->data[i];
}
int frame_linesize(AVFrame *f, int i) {
    return f->linesize[i];
}

AVFrame *alloc_rgba_frame(AVFrame *yuv_frame) {

    AVFrame* frame = avcodec_alloc_frame();
    frame->width = yuv_frame->width;
    frame->height = yuv_frame->height;
    frame->format = AV_PIX_FMT_RGBA;

    // Allocate a buffer large enough for all data
    int size = 4 * yuv_frame->width * yuv_frame->height;
    uint8_t* buffer = (uint8_t*)av_malloc(size);

    // Initialize frame->linesize and frame->data pointers
    avpicture_fill((AVPicture*)frame, buffer, frame->format, frame->width, frame->height);

    return frame;
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