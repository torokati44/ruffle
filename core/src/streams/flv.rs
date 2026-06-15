//! FLV-specific NetStream processing

use crate::avm1::{
    Activation as Avm1Activation, ActivationIdentifier as Avm1ActivationIdentifier,
    ExecutionReason as Avm1ExecutionReason, FlvValueAvm1Ext,
};
use crate::avm2::{
    Activation as Avm2Activation, Error as Avm2Error, FlvValueAvm2Ext, FunctionArgs,
    Value as Avm2Value,
};
use crate::backend::audio::{SoundStreamInfo, SoundStreamWrapping};
use crate::context::UpdateContext;
use crate::display_object::TDisplayObject;
use crate::string::AvmString;
use flv_rs::{
    AudioData as FlvAudioData, AudioDataType as FlvAudioDataType, ScriptData as FlvScriptData,
    SoundFormat as FlvSoundFormat, SoundRate as FlvSoundRate, SoundSize as FlvSoundSize,
    SoundType as FlvSoundType, Value as FlvValue, VideoData as FlvVideoData,
    VideoPacket as FlvVideoPacket,
};
use ruffle_common::buffer::{Slice, Substream};
use ruffle_video::frame::EncodedFrame;
use swf::{AudioCompression, SoundFormat, VideoCodec, VideoDeblocking};

use super::{NetStream, NetStreamKind, NetStreamType, NetstreamError};

impl<'gc> NetStream<'gc> {
    /// Process a parsed FLV audio tag.
    ///
    /// `slice` must reference the underlying backing buffer.
    pub(super) fn flv_audio_tag(
        self,
        slice: &Slice,
        audio_data: FlvAudioData<'_>,
    ) -> Result<(), NetstreamError> {
        let data = match audio_data.data {
            FlvAudioDataType::Raw(data)
            | FlvAudioDataType::AacSequenceHeader(data)
            | FlvAudioDataType::AacRaw(data) => slice.to_subslice(data),
        };
        let source = self.source();
        let audio_stream = &mut *source.audio_stream.borrow_mut();
        let substream = match audio_stream {
            Some((substream, _sound_stream_info)) => {
                if substream
                    .last_chunk()
                    .map(|lc| lc.end() > data.start())
                    .unwrap_or(false)
                {
                    // Reject repeats of existing tags.
                    // We need to do this because of lookahead - we will
                    // encounter the same audio tag multiple times as we buffer
                    // a few ahead for the audio backend.
                    // This assumes that tags are processed in-order - which
                    // should always be the case. Seeks should cancel the audio
                    // stream before processing new tags.
                    return Ok(());
                }

                substream
            }
            audio_stream => {
                // None
                let substream = Substream::new(slice.buffer().clone());
                let swf_format = SoundFormat {
                    compression: match audio_data.format {
                        FlvSoundFormat::LinearPCMPlatformEndian => {
                            AudioCompression::UncompressedUnknownEndian
                        }
                        FlvSoundFormat::Adpcm => AudioCompression::Adpcm,
                        FlvSoundFormat::MP3 => AudioCompression::Mp3,
                        FlvSoundFormat::LinearPCMLittleEndian => AudioCompression::Uncompressed,
                        FlvSoundFormat::Nellymoser16kHz => AudioCompression::Nellymoser16Khz,
                        FlvSoundFormat::Nellymoser8kHz => AudioCompression::Nellymoser8Khz,
                        FlvSoundFormat::Nellymoser => AudioCompression::Nellymoser,
                        FlvSoundFormat::G711ALawPCM => AudioCompression::G711ALawPCM,
                        FlvSoundFormat::G711MuLawPCM => AudioCompression::G711MuLawPCM,
                        FlvSoundFormat::Aac => AudioCompression::Aac,
                        FlvSoundFormat::Speex => AudioCompression::Speex,
                        FlvSoundFormat::MP38kHz => AudioCompression::Mp3,
                        FlvSoundFormat::DeviceSpecific => return Err(NetstreamError::UnknownCodec),
                    },
                    sample_rate: match (audio_data.format, audio_data.rate) {
                        (FlvSoundFormat::G711ALawPCM, _)
                        | (FlvSoundFormat::G711MuLawPCM, _)
                        | (FlvSoundFormat::MP38kHz, _) => 8_000,
                        (_, FlvSoundRate::R5_500) => 5_500,
                        (_, FlvSoundRate::R11_000) => 11_000,
                        (_, FlvSoundRate::R22_000) => 22_000,
                        (_, FlvSoundRate::R44_000) => 44_000,
                    },
                    is_stereo: match audio_data.sound_type {
                        FlvSoundType::Mono => false,
                        FlvSoundType::Stereo => true,
                    },
                    is_16_bit: match audio_data.size {
                        FlvSoundSize::Bits8 => false,
                        FlvSoundSize::Bits16 => true,
                    },
                };

                let sound_stream_head = SoundStreamInfo {
                    wrapping: SoundStreamWrapping::Unwrapped,
                    stream_format: swf_format,
                    num_samples_per_block: 0,
                    latency_seek: 0,
                };

                *audio_stream = Some((substream, sound_stream_head));

                &mut audio_stream.as_mut().unwrap().0
            }
        };

        Ok(substream.append(data)?)
    }

    /// Process a parsed FLV video tag.
    ///
    /// `slice` must reference the underlying backing buffer.
    ///
    /// `tag_needs_preloading` indicates that this video tag has not been
    /// encountered before.
    pub(super) fn flv_video_tag(
        self,
        context: &mut UpdateContext<'gc>,
        slice: &Slice,
        video_data: FlvVideoData<'_>,
        tag_needs_preloading: bool,
    ) {
        let source = self.source();
        let (video_handle, frame_id) = match *source.stream_type.borrow() {
            Some(NetStreamType::Flv {
                video_stream,
                frame_id,
                ..
            }) => (video_stream, frame_id),
            _ => unreachable!(),
        };
        let codec = VideoCodec::from_u8(video_data.codec_id as u8);
        let buffer = slice.data();

        match (video_handle, codec, video_data.data) {
            (
                maybe_video_handle,
                Some(codec),
                FlvVideoPacket::Data(mut data)
                | FlvVideoPacket::Vp6Data {
                    hadjust: _,
                    vadjust: _,
                    mut data,
                },
            ) => {
                //Some movies don't actually have metadata, so let's register a
                //dummy stream just in case. All the actual data in the registration
                //is lies, of course.
                let video_handle = match maybe_video_handle {
                    Some(stream) => stream,
                    None => {
                        match context.video.register_video_stream(
                            1,
                            (8, 8),
                            codec,
                            VideoDeblocking::UseVideoPacketValue,
                        ) {
                            Ok(new_handle) => {
                                match &mut *source.stream_type.borrow_mut() {
                                    Some(NetStreamType::Flv { video_stream, .. }) => {
                                        *video_stream = Some(new_handle)
                                    }
                                    _ => unreachable!(),
                                }

                                new_handle
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Got error when registering FLV video stream: {}",
                                    e
                                );
                                return; //TODO: This originally breaks and halts tag processing
                            }
                        }
                    }
                };

                if codec == VideoCodec::ScreenVideo || codec == VideoCodec::ScreenVideoV2 {
                    // ScreenVideo streams consider the FLV
                    // video data byte to be integral to their
                    // own bitstream.
                    let offset = data.as_ptr() as usize - buffer.as_ptr() as usize;
                    let len = data.len();
                    data = buffer
                        .get(offset - 1..offset + len)
                        .expect("screenvideo flvs have video data bytes");
                }

                // NOTE: Currently, no implementation of the decoder backend actually requires
                if tag_needs_preloading {
                    let encoded_frame = EncodedFrame {
                        codec,
                        data, //TODO: ScreenVideo's decoder wants the FLV header bytes
                        frame_id,
                    };

                    if let Err(e) = context
                        .video
                        .preload_video_stream_frame(video_handle, encoded_frame)
                    {
                        tracing::error!("Preloading video frame {} failed: {}", frame_id, e);
                    }
                }

                let encoded_frame = EncodedFrame {
                    codec,
                    data, //TODO: ScreenVideo's decoder wants the FLV header bytes
                    frame_id,
                };

                match context.video.decode_video_stream_frame(
                    video_handle,
                    encoded_frame,
                    context.renderer,
                ) {
                    Ok(bitmap_info) => {
                        self.0.last_decoded_bitmap.replace(Some(bitmap_info));
                        if let Some(mc) = self.0.attached_to.get() {
                            mc.invalidate_cached_bitmap();
                            *context.needs_render = true;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Decoding video frame {} failed: {}", frame_id, e);
                    }
                }
            }
            (_, _, FlvVideoPacket::CommandFrame(_command)) => {
                tracing::warn!("Stub: FLV command frame processing")
            }
            (Some(video_handle), _, FlvVideoPacket::AvcSequenceHeader(data)) => {
                match context
                    .video
                    .configure_video_stream_decoder(video_handle, data)
                {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!("Configuring video decoder {} failed: {}", frame_id, e);
                    }
                }
            }
            (
                Some(video_handle),
                Some(codec),
                FlvVideoPacket::AvcNalu {
                    composition_time_offset: _,
                    data,
                },
            ) => {
                let encoded_frame = EncodedFrame {
                    codec,
                    data,
                    frame_id,
                };

                match context.video.decode_video_stream_frame(
                    video_handle,
                    encoded_frame,
                    context.renderer,
                ) {
                    Ok(bitmap_info) => {
                        self.0.last_decoded_bitmap.replace(Some(bitmap_info));
                        if let Some(mc) = self.0.attached_to.get() {
                            mc.invalidate_cached_bitmap();
                            *context.needs_render = true;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Decoding video frame {} failed: {}", frame_id, e);
                    }
                }
            }
            (_, _, FlvVideoPacket::AvcEndOfSequence) => {
                tracing::warn!("Stub: FLV AVC/H.264 End of Sequence processing")
            }
            (_, None, _) => {
                tracing::error!(
                    "FLV video tag has invalid codec id {}",
                    video_data.codec_id as u8
                )
            }
            (None, _, _) => {
                tracing::error!("No video handle")
            }
        }

        match &mut *source.stream_type.borrow_mut() {
            Some(NetStreamType::Flv { frame_id, .. }) => *frame_id += 1,
            _ => unreachable!(),
        };
    }

    /// Process a parsed FLV script tag.
    ///
    /// This function attempts to borrow the current `NetStream`, you must drop
    /// any existing borrows and pick them back up when you're done.
    ///
    /// `tag_needs_preloading` indicates that this script tag has not been
    /// encountered before.
    pub(super) fn flv_script_tag(
        self,
        context: &mut UpdateContext<'gc>,
        script_data: FlvScriptData<'_>,
        tag_needs_preloading: bool,
    ) {
        let source = self.source();
        let has_stream_already = match &*source.stream_type.borrow() {
            Some(NetStreamType::Flv { video_stream, .. }) => video_stream.is_some(),
            _ => unreachable!(),
        };

        let mut width = None;
        let mut height = None;
        let mut video_codec_id = None;
        let mut frame_rate = None;
        let mut duration = None;

        for var in script_data.0 {
            if var.name == b"onMetaData" && !has_stream_already {
                match var.data.clone() {
                    FlvValue::Object(subvars) | FlvValue::EcmaArray(subvars) => {
                        for subvar in subvars {
                            match (subvar.name, subvar.data) {
                                (b"width", FlvValue::Number(val)) => width = Some(val),
                                (b"height", FlvValue::Number(val)) => height = Some(val),
                                (b"videocodecid", FlvValue::Number(val)) => {
                                    video_codec_id = Some(val)
                                }
                                (b"framerate", FlvValue::Number(val)) => frame_rate = Some(val),
                                (b"duration", FlvValue::Number(val)) => duration = Some(val),
                                _ => {}
                            }
                        }
                    }
                    _ => tracing::error!("Invalid FLV metadata tag!"),
                }
            }
            let avm_object = self.0.avm_object.get();
            // This is necessary because the script callback functions can call back into
            // these methods, (e.g. NetStream::play), so we need to avoid holding a borrow
            // while the script data is being handled.
            let _ = self.handle_script_data(avm_object, context, var.name, var.data);
            // Any errors while trying to lookup or call AVM2 properties are silently swallowed.
        }

        if tag_needs_preloading
            && let (
                Some(width),
                Some(height),
                Some(video_codec_id),
                Some(frame_rate),
                Some(duration),
            ) = (width, height, video_codec_id, frame_rate, duration)
        {
            let num_frames = frame_rate * duration;
            if let Some(video_codec) = VideoCodec::from_u8(video_codec_id as u8) {
                match context.video.register_video_stream(
                    num_frames as u32,
                    (width as u16, height as u16),
                    video_codec,
                    VideoDeblocking::UseVideoPacketValue,
                ) {
                    Ok(stream_handle) => match &mut *source.stream_type.borrow_mut() {
                        Some(NetStreamType::Flv { video_stream, .. }) => {
                            *video_stream = Some(stream_handle)
                        }
                        _ => unreachable!(),
                    },
                    Err(e) => {
                        tracing::error!("Got error when registering FLV video stream: {}", e)
                    }
                }
            } else {
                tracing::error!("FLV video stream has invalid codec ID {}", video_codec_id);
            }
        }
    }

    fn handle_script_data(
        self,
        avm_object: Option<NetStreamKind<'gc>>,
        context: &mut UpdateContext<'gc>,
        variable_name: &[u8],
        variable_data: FlvValue,
    ) -> Result<(), Avm2Error<'gc>> {
        match avm_object {
            Some(NetStreamKind::Avm1(object)) => {
                let avm_string_name = AvmString::new_utf8_bytes(context.gc(), variable_name);
                let activation_name = format!("[FLV {avm_string_name}]");

                let root = context.stage.root_clip().expect("root");
                let mut activation = Avm1Activation::from_nothing(
                    context,
                    Avm1ActivationIdentifier::root(&activation_name),
                    root,
                );

                let avm1_object_value = variable_data.to_avm1_value(&mut activation);

                if let Err(e) = object.call_method(
                    avm_string_name,
                    &[avm1_object_value],
                    &mut activation,
                    Avm1ExecutionReason::Special,
                ) {
                    tracing::error!(
                        "Got error when dispatching AVM1 {} script data handler from NetStream: {}",
                        avm_string_name,
                        e,
                    );
                }
            }
            Some(NetStreamKind::Avm2(_)) => {
                let mut activation = Avm2Activation::from_nothing(context);
                let client_object = self
                    .client()
                    .expect("Client should be initialized if script data is being accessed");

                let data_object = variable_data.to_avm2_value(activation.context);
                let args = &[data_object];

                Avm2Value::from(client_object).call_public_property(
                    AvmString::new_utf8_bytes(activation.gc(), variable_name),
                    FunctionArgs::from_slice(args),
                    &mut activation,
                )?;
            }
            None => {}
        };

        Ok(())
    }
}
