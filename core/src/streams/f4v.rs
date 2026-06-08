//! F4V/MP4 stream processing for NetStream.

use crate::backend::audio::{SoundStreamInfo, SoundStreamWrapping};
use crate::context::UpdateContext;
use crate::display_object::TDisplayObject;
use crate::streams::{NetStream, NetStreamType};
use ruffle_common::buffer::{Buffer, Substream};
use ruffle_video::frame::EncodedFrame;
use std::rc::Rc;
use swf::{AudioCompression, SoundFormat, VideoCodec, VideoDeblocking};

impl<'gc> NetStream<'gc> {
    /// Seek to a position within an F4V/MP4 stream.
    ///
    /// Mutates `next_frame`, `next_audio_sample`, `audio_buffer`, and
    /// `source.stream_time` to reflect the nearest keyframe at or before
    /// `offset` (ms).
    pub(super) fn f4v_seek(self, offset: f64) {
        let source = self.source();

        // Extract the context and track indices without holding the borrow alive,
        // so we can later take a mutable borrow to update next_frame / next_audio_sample.
        let (mp4_context, video_track_index, audio_track_index) = {
            let st = source.stream_type.borrow();
            match &*st {
                Some(NetStreamType::F4v {
                    context,
                    video_track_index,
                    audio_track_index,
                    ..
                }) => (context.clone(), *video_track_index, *audio_track_index),
                _ => unreachable!(),
            }
        };

        // Find the last sync (keyframe) sample whose decode timestamp is at or
        // before the seek target. `offset` is in milliseconds.
        let mut seek_frame: Option<u32> = None;
        let mut seek_time_ms: f64 = 0.0;
        if let Some(mp4) = &mp4_context {
            if let Some(vti) = video_track_index {
                if let Some(trk) = mp4.tracks().get(&vti) {
                    for (idx, sample) in trk.samples.iter().enumerate() {
                        let sample_time_ms =
                            sample.decode_timestamp as f64 * 1000.0 / sample.timescale as f64;
                        if sample_time_ms > offset {
                            break;
                        }
                        if sample.is_sync {
                            seek_frame = Some(idx as u32);
                            seek_time_ms = sample_time_ms;
                        }
                    }
                }
            }
        }

        // Find the first audio sample at or after the seek target time.
        let mut seek_audio_sample: u32 = 0;
        if let Some(mp4) = &mp4_context {
            if let Some(ati) = audio_track_index {
                if let Some(trk) = mp4.tracks().get(&ati) {
                    for (idx, sample) in trk.samples.iter().enumerate() {
                        let sample_time_ms =
                            sample.decode_timestamp as f64 * 1000.0 / sample.timescale as f64;
                        if sample_time_ms >= seek_time_ms {
                            seek_audio_sample = idx as u32;
                            break;
                        }
                    }
                }
            }
        }

        if let Some(NetStreamType::F4v {
            next_frame,
            next_audio_sample,
            audio_buffer,
            ..
        }) = &mut *source.stream_type.borrow_mut()
        {
            *next_frame = seek_frame.unwrap_or(0);
            *next_audio_sample = seek_audio_sample;
            *audio_buffer = Buffer::new();
        }
        source.stream_time.set(seek_time_ms);
    }

    /// Process the F4V/MP4 portion of a stream tick.
    ///
    /// Returns `(buffer_underrun,)` — there is no separate `error` flag for
    /// F4V because decode errors are soft warnings (H.264 startup delay).
    pub(super) fn f4v_tick(
        self,
        context: &mut UpdateContext<'gc>,
        max_time: f64,
    ) -> bool {
        let source = self.source();
        let slice = source.buffer.borrow().to_full_slice();
        let buffer = slice.data();

        let mut video_exhausted = false;
        let mut audio_exhausted = false;

        let mut stream_type = source.stream_type.borrow_mut();
        let Some(NetStreamType::F4v {
            context: media_context,
            video_track_index,
            audio_track_index,
            next_frame,
            next_audio_sample,
            audio_buffer,
            video_stream,
        }) = &mut *stream_type
        else {
            return false;
        };

        if media_context.is_none() && buffer.len() > source.preload_offset.get() {
            match re_mp4::Mp4::read_bytes(&buffer) {
                Ok(mp4) => {
                    for (i, trak) in mp4.tracks().iter() {
                        match trak.kind {
                            Some(re_mp4::TrackKind::Video) if video_track_index.is_none() => {
                                // Only pick video tracks with a codec Ruffle can decode (H.264/avc1).
                                let stsd = &trak.trak(&mp4).mdia.minf.stbl.stsd.contents;
                                if matches!(stsd, re_mp4::StsdBoxContent::Avc1(_)) {
                                    *video_track_index = Some(*i);
                                } else {
                                    tracing::warn!(
                                        "F4V video track uses unsupported codec {:?}; only H.264 (avc1) is supported",
                                        trak.codec_string(&mp4)
                                    );
                                }
                            }
                            Some(re_mp4::TrackKind::Audio) if audio_track_index.is_none() => {
                                *audio_track_index = Some(*i);
                            }
                            _ => {}
                        }
                    }

                    media_context.replace(Rc::new(mp4));
                }
                Err(_) => {
                    // moov box not yet fully buffered; skip until more data arrives.
                    source.preload_offset.set(buffer.len());
                    return false;
                }
            }
        }

        if media_context.is_none() {
            return false;
        }

        let media_context = media_context.as_ref().unwrap().clone();

        if let Some(vti) = *video_track_index {
            let trk = media_context.tracks().get(&vti).unwrap();

            // Ensure the video stream is registered before the decode loop.
            if video_stream.is_none() {
                match context.video.register_video_stream(
                    1,
                    (8, 8),
                    VideoCodec::H264,
                    VideoDeblocking::UseVideoPacketValue,
                ) {
                    Ok(new_handle) => {
                        *video_stream = Some(new_handle);
                        let ccfg = trk.raw_codec_config(&media_context).unwrap();
                        if let Err(e) = context
                            .video
                            .configure_video_stream_decoder(new_handle, &ccfg)
                        {
                            tracing::error!("Failed to configure H.264 decoder: {}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Got error when registering F4V video stream: {}", e);
                    }
                }
            }

            if let Some(video_handle) = *video_stream {
                // When lagging by more than 500 ms, jump to the latest keyframe within
                // the current tick window rather than decoding every intermediate frame.
                // H.264 frames between keyframes cannot be safely skipped without
                // resetting decoder state, so we seek to the nearest sync sample instead.
                if let Some(first_smpl) = trk.samples.get(*next_frame as usize) {
                    let first_time_ms = first_smpl.decode_timestamp as f64 * 1000.0
                        / first_smpl.timescale as f64;
                    if first_time_ms + 500.0 < max_time {
                        let skip_to = trk
                            .samples
                            .iter()
                            .enumerate()
                            .skip(*next_frame as usize)
                            .filter(|(_, s)| {
                                let t =
                                    s.decode_timestamp as f64 * 1000.0 / s.timescale as f64;
                                s.is_sync && t <= max_time
                            })
                            .map(|(i, _)| i)
                            .last();
                        if let Some(idx) = skip_to {
                            *next_frame = idx as u32;
                        }
                    }
                }

                loop {
                    let sample_id = *next_frame;
                    let smpl = match trk.samples.get(sample_id as usize) {
                        Some(smpl) => smpl,
                        None => {
                            video_exhausted = true;
                            break;
                        }
                    };

                    let sample_time_ms =
                        smpl.decode_timestamp as f64 * 1000.0 / smpl.timescale as f64;
                    if sample_time_ms > max_time {
                        break;
                    }
                    *next_frame += 1;

                    let offs = smpl.offset as usize;
                    let siz = smpl.size as usize;

                    if buffer.len() < offs + siz {
                        tracing::error!("Buffer too small for F4V video frame");
                        *next_frame = sample_id;
                        break;
                    }

                    let encoded_frame = EncodedFrame {
                        codec: VideoCodec::H264,
                        data: buffer[offs..offs + siz].as_ref(),
                        frame_id: sample_id,
                    };

                    match context.video.decode_video_stream_frame(
                        video_handle,
                        encoded_frame,
                        context.renderer,
                    ) {
                        Ok(frame) => {
                            self.0.last_decoded_bitmap.replace(Some(frame));
                            if let Some(mc) = self.0.attached_to.get() {
                                mc.invalidate_cached_bitmap();
                                *context.needs_render = true;
                            }
                        }
                        Err(e) => {
                            // H.264 decoders commonly have a startup delay of
                            // a few frames before producing output. This is
                            // expected and not a real error.
                            tracing::warn!(
                                "F4V video decode produced no frame (may be startup delay): {}",
                                e
                            );
                        }
                    }
                } // video loop
            }
        } // if let Some(vti)

        // AAC audio track processing.
        if let Some(ati) = *audio_track_index {
            let audio_trk = media_context.tracks().get(&ati).unwrap();

            // Initialize audio stream on first use.
            if source.audio_stream.borrow().is_none() {
                let stsd = &audio_trk.trak(&media_context).mdia.minf.stbl.stsd.contents;
                if let re_mp4::StsdBoxContent::Mp4a(mp4a) = stsd {
                    if let Some(esds) = &mp4a.esds {
                        let ds = &esds.es_desc.dec_config.dec_specific;
                        let asc = [
                            (ds.profile << 3) | (ds.freq_index >> 1),
                            ((ds.freq_index & 1) << 7) | (ds.chan_conf << 3),
                        ];
                        // Chunk type 0x00 = AacSequenceHeader (required by AacSubstreamDecoder)
                        audio_buffer.extend_from_slice(&[0x00, asc[0], asc[1]]);
                        let config_slice = audio_buffer.to_full_slice();
                        let swf_format = SoundFormat {
                            compression: AudioCompression::Aac,
                            sample_rate: mp4a.samplerate.value(),
                            is_stereo: mp4a.channelcount == 2,
                            is_16_bit: true,
                        };
                        let sound_stream_info = SoundStreamInfo {
                            wrapping: SoundStreamWrapping::Unwrapped,
                            stream_format: swf_format,
                            num_samples_per_block: 0,
                            latency_seek: 0,
                        };
                        let mut substream = Substream::new(audio_buffer.clone());
                        substream.append(config_slice).expect("same buffer");
                        *source.audio_stream.borrow_mut() = Some((substream, sound_stream_info));
                    }
                }
            }

            // Feed audio samples up to max_time.
            loop {
                if source.audio_stream.borrow().is_none() {
                    break;
                }
                let smpl = match audio_trk.samples.get(*next_audio_sample as usize) {
                    Some(s) => s,
                    None => {
                        audio_exhausted = true;
                        break;
                    }
                };
                let sample_time_ms =
                    smpl.decode_timestamp as f64 * 1000.0 / smpl.timescale as f64;
                if sample_time_ms > max_time {
                    break;
                }
                *next_audio_sample += 1;
                let offs = smpl.offset as usize;
                let siz = smpl.size as usize;
                if buffer.len() >= offs + siz {
                    let audio_start = audio_buffer.len();
                    // Chunk type 0x01 = AacRaw (required by AacSubstreamDecoder)
                    audio_buffer.extend_from_slice(&[0x01]);
                    audio_buffer.extend_from_slice(&buffer[offs..offs + siz]);
                    if let Some(audio_slice) = audio_buffer.get(audio_start..) {
                        if let Some((substream, _)) = &mut *source.audio_stream.borrow_mut() {
                            let _ = substream.append(audio_slice);
                        }
                    }
                }
            }
        } // if let Some(ati)

        // Signal buffer underrun only when every present track is exhausted,
        // so audio-only or video-only completion doesn't cut the other track short.
        let video_done = video_track_index.is_none() || video_exhausted;
        let audio_done = audio_track_index.is_none() || audio_exhausted;
        video_done && audio_done
    }
}
