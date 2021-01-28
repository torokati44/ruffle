

#include "libavutil/common.h"
#include "libavcodec/avcodec.h"


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
