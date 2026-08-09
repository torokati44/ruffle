//! NetStream implementation

use crate::avm1::{
    Activation as Avm1Activation, ActivationIdentifier as Avm1ActivationIdentifier,
    ExecutionReason as Avm1ExecutionReason, FlvValueAvm1Ext, Object as Avm1Object,
};
use crate::avm2::object::NetStreamObject;
use crate::avm2::{
    Activation as Avm2Activation, Avm2, Error as Avm2Error, EventObject as Avm2EventObject,
    FlvValueAvm2Ext, FunctionArgs, Object as Avm2Object, Value as Avm2Value,
};
use crate::backend::audio::{
    DecodeError, SoundInstanceHandle, SoundStreamInfo, SoundStreamWrapping,
};
use crate::backend::navigator::Request;
use crate::context::UpdateContext;
use crate::display_object::{MovieClip, TDisplayObject};
use crate::loader::Error;
use crate::string::AvmString;
use flv_rs::{
    AudioData as FlvAudioData, AudioDataType as FlvAudioDataType, Error as FlvError, FlvReader,
    FrameType as FlvFrameType, Header as FlvHeader, ScriptData as FlvScriptData,
    SoundFormat as FlvSoundFormat, SoundRate as FlvSoundRate, SoundSize as FlvSoundSize,
    SoundType as FlvSoundType, Tag as FlvTag, TagData as FlvTagData, Value as FlvValue,
    VideoData as FlvVideoData, VideoPacket as FlvVideoPacket,
};
use gc_arena::barrier::unlock;
use gc_arena::{Collect, DynamicRoot, Gc, Lock, Mutation, Rootable};
use ruffle_common::buffer::{Buffer, Slice, Substream, SubstreamError};
use ruffle_common::duration::FloatDuration;
use ruffle_macros::istr;
use ruffle_render::bitmap::BitmapInfo;
use ruffle_video::VideoStreamHandle;
use ruffle_video::frame::{EncodedFrame, PresentationTime};
use ruffle_video::queue::Presentation;
use std::cell::{Cell, RefCell};
use std::cmp::max;
use std::io::{Seek, SeekFrom};
use swf::{AudioCompression, SoundFormat, VideoCodec, VideoDeblocking};
use thiserror::Error;
use url::Url;

/// How far past the playhead the feed cursor runs, in milliseconds.
///
/// The audio backend is fed from the same cursor as the video decoders, and
/// wants a little more than the current tick's worth of samples so that it does
/// not run dry in between. This replaces what used to be a count of five audio
/// tags, which could not bound anything at all on a stream with no audio track.
const AUDIO_LOOKAHEAD_MS: f64 = 100.0;

/// Why a cursor stopped walking the tag stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CursorStop {
    /// It reached the point it was asked to reach.
    CaughtUp,

    /// The stream has no more data.
    OutOfData,

    /// The tag stream is corrupt.
    Corrupt,

    /// A script callback replaced the stream's source out from under it.
    SourceReplaced,
}

#[derive(Debug, Error)]
enum NetstreamError {
    #[error("Decoding failed because {0}")]
    DecodeError(DecodeError),

    #[error("Substream management error {0}")]
    SubstreamError(SubstreamError),

    #[error("Unknown codec")]
    UnknownCodec,
}

impl From<DecodeError> for NetstreamError {
    fn from(err: DecodeError) -> NetstreamError {
        NetstreamError::DecodeError(err)
    }
}

impl From<SubstreamError> for NetstreamError {
    fn from(err: SubstreamError) -> NetstreamError {
        NetstreamError::SubstreamError(err)
    }
}

/// Manager for all media streams.
///
/// This does *not* handle data transport; which is delegated to `LoadManager`.
/// `StreamManager` *only* handles decoding or encoding of relevant media
/// streams.
#[derive(Collect)]
#[collect(no_drop)]
pub struct StreamManager<'gc> {
    /// List of streams that need tick processing.
    ///
    /// This is not the total list of all created NetStreams; only the ones
    /// that have been configured to play media.
    ///
    /// A stream becomes active if it is either playing streaming media or is
    /// doing other tick-time processing such as seeking.
    active_streams: Vec<NetStream<'gc>>,
}

impl Default for StreamManager<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'gc> StreamManager<'gc> {
    pub fn new() -> Self {
        StreamManager {
            active_streams: Vec::new(),
        }
    }

    /// Activate a `NetStream`.
    ///
    /// This can be called at any time to flag that a `NetStream` has work to
    /// do, and called multiple times. The `NetStream` will determine what to
    /// do at tick time.
    pub fn activate(context: &mut UpdateContext<'gc>, stream: NetStream<'gc>) {
        if !context.stream_manager.active_streams.contains(&stream) {
            context.stream_manager.active_streams.push(stream);
        }
    }

    /// Deactivate a `NetStream`.
    ///
    /// This should only ever be called at tick time if the stream itself has
    /// determined there is no future work for it to do.
    pub fn deactivate(context: &mut UpdateContext<'gc>, stream: NetStream<'gc>) {
        let index = context
            .stream_manager
            .active_streams
            .iter()
            .position(|x| *x == stream);
        if let Some(index) = index {
            context.stream_manager.active_streams.remove(index);
        }
    }

    /// Process all active media streams.
    ///
    /// This is an unlocked timestep; the `dt` parameter indicates how many
    /// milliseconds have elapsed since the last tick. This is intended to
    /// support video framerates separate from the Stage frame rate.
    ///
    /// This does not borrow `&mut self` as we need the `UpdateContext`, too.
    pub fn tick(context: &mut UpdateContext<'gc>, dt: FloatDuration) {
        let streams = context.stream_manager.active_streams.clone();
        for stream in streams {
            stream.tick(context, dt)
        }
    }
}

#[derive(Copy, Clone, Collect, Debug)]
#[collect(no_drop)]
enum NetStreamKind<'gc> {
    Avm2(NetStreamObject<'gc>),
    Avm1(Avm1Object<'gc>),
}

/// A stream representing download of some (audiovisual) data.
///
/// `NetStream` interacts with several different parts of player
/// infrastructure:
///
///  * `LoadManager` fills individual `NetStream` buffers with data (or, in the
///    future, empties them out for media upload)
///  * `StreamManager` processes media data in the `NetStream` buffer (in the
///    future, sending it to the audio backend or `SoundManager`)
///  * `Video` display objects linked to this `NetStream` display the latest
///    decoded frame.
///
/// It corresponds directly to the AVM1 and AVM2 `NetStream` classes; it's API
/// is intended to be a VM-agnostic version of those.
#[derive(Clone, Debug, Collect, Copy)]
#[collect(no_drop)]
pub struct NetStream<'gc>(Gc<'gc, NetStreamData<'gc>>);

impl PartialEq for NetStream<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(Gc::as_ptr(self.0), Gc::as_ptr(other.0))
    }
}

impl Eq for NetStream<'_> {}

#[derive(Clone)]
pub struct NetStreamHandle(DynamicRoot<Rootable![NetStreamData<'_>]>);

impl NetStreamHandle {
    pub fn stash<'gc>(context: &UpdateContext<'gc>, this: NetStream<'gc>) -> Self {
        Self(context.dynamic_root.stash(context.gc(), this.0))
    }

    pub fn fetch<'gc>(&self, context: &UpdateContext<'gc>) -> NetStream<'gc> {
        NetStream(context.dynamic_root.fetch(&self.0))
    }
}

/// The current type of the data in the stream buffer.
#[derive(Clone, Debug)]
pub enum NetStreamType {
    /// The stream is an FLV.
    Flv {
        #[expect(dead_code)] // set but never read
        header: FlvHeader,

        /// The currently playing video track's stream instance.
        video_stream: Option<VideoStreamHandle>,

        /// The index of the last processed frame.
        ///
        /// FLV does not store this information directly and we are not holding
        /// onto a table of data buffers like `Video` does, so we must maintain
        /// frame IDs ourselves for various API related purposes.
        frame_id: u32,

        /// The largest composition time offset seen in the stream so far, in
        /// milliseconds.
        ///
        /// This is how much earlier than its presentation time the stream
        /// expects a frame to be decoded, and so how far ahead of the playhead
        /// the feed cursor has to run for the decoder to have caught up by the
        /// time the frame is due. It stays at zero for every codec without
        /// bidirectional prediction, which is all of them but H.264.
        max_composition_offset: i64,

        /// The gap between the last two video tags, in milliseconds.
        ///
        /// Used as a one-frame margin on top of `max_composition_offset`, for a
        /// decoder that releases a picture a frame later than it strictly has
        /// to.
        video_frame_interval: i64,

        /// The decode timestamp of the last video tag handed to the decoder.
        last_video_dts: i64,
    },
}

#[derive(Clone, Debug, Collect)]
#[collect(no_drop)]
pub struct NetStreamSource {
    /// All data currently loaded in the stream.
    buffer: RefCell<Buffer>,

    /// The buffer position of the presentation cursor.
    ///
    /// Tags before this point have been presented: their script callbacks have
    /// run, and any video frame they carry is either on screen or has been
    /// passed over.
    offset: Cell<usize>,

    /// The buffer position of the feed cursor.
    ///
    /// Tags before this point have been handed to the audio and video decoders.
    /// This runs ahead of `offset`, so that a decoder which cannot produce a
    /// picture the moment it is given a frame still has it ready in time. It is
    /// always greater than or equal to the offset position.
    feed_offset: Cell<usize>,

    /// The expected length of the buffer once downloading is complete.
    ///
    /// `None` indicates that downloading is already complete and that the
    /// length of the associated `Buffer` is the final length.
    expected_length: Cell<Option<usize>>,

    /// The buffer position for processing incoming data.
    ///
    /// This points to the first byte that the stream has *never* processed
    /// before in the buffer. It should always be greater than or equal to the
    /// feed position.
    ///
    /// Certain data, such as the header or metadata of an FLV, should only
    /// ever be processed one time, even if we seek backwards to it later on.
    /// We call this data "preloaded", whether or not there is actually a
    /// separate preload step for that given format.
    preload_offset: Cell<usize>,

    /// The current stream type, if known.
    stream_type: RefCell<Option<NetStreamType>>,

    /// The current seek offset in the stream in milliseconds.
    stream_time: Cell<f64>,

    /// The next queued seek offset in milliseconds.
    ///
    /// Seeks are only executed on the next stream tick.
    queued_seek_time: Cell<Option<f64>>,

    /// The `Substream` associated with the currently playing audio track and
    /// the expected playback format of that audio.
    audio_stream: RefCell<Option<(Substream, SoundStreamInfo)>>,

    /// The currently playing sound stream
    sound_instance: Cell<Option<SoundInstanceHandle>>,
}

#[derive(Clone, Debug, Collect)]
#[collect(no_drop)]
pub struct NetStreamData<'gc> {
    /// Stream source.
    source: Lock<Gc<'gc, NetStreamSource>>,

    /// The number of seconds of video data that should be buffered. This is
    /// currently unsupported and changing it has no effect.
    buffer_time: Cell<f64>,

    /// The last decoded bitmap.
    ///
    /// Any `Video`s on the stage will display the bitmap here when attached to
    /// this `NetStream`.
    last_decoded_bitmap: RefCell<Option<BitmapInfo>>,

    /// The AVM side of this stream.
    avm_object: Lock<Option<NetStreamKind<'gc>>>,

    /// The AVM2 client object, which corresponds to `NetStream.client`.
    avm2_client: Lock<Option<Avm2Object<'gc>>>,

    /// The URL of the requested FLV if one exists.
    url: RefCell<Option<String>>,

    /// The MovieClip this `NetStream` is attached to.
    attached_to: Lock<Option<MovieClip<'gc>>>,

    /// True if the stream should play when ticked.
    playing: Cell<bool>,
}

impl NetStreamSource {
    /// Move both cursors to the same place in the buffer, because the stream
    /// has jumped and nothing between them is wanted any more.
    fn rewind_cursors_to(&self, offset: usize) {
        self.offset.set(offset);
        self.feed_offset.set(offset);
    }
}

impl Default for NetStreamSource {
    fn default() -> Self {
        Self {
            buffer: RefCell::new(Buffer::new()),
            offset: Cell::new(0),
            feed_offset: Cell::new(0),
            expected_length: Cell::new(Some(0)),
            preload_offset: Cell::new(0),
            stream_type: RefCell::new(None),
            stream_time: Cell::new(0.0),
            queued_seek_time: Cell::new(None),
            audio_stream: RefCell::new(None),
            sound_instance: Cell::new(None),
        }
    }
}

impl<'gc> NetStream<'gc> {
    /// Create a `NetStream` for use in AVM1.
    pub fn new_avm1(gc_context: &Mutation<'gc>, avm_object: Avm1Object<'gc>) -> Self {
        Self::new(gc_context, Some(NetStreamKind::Avm1(avm_object)))
    }

    /// Create a `NetStream` for use in AVM2. The caller is expected to initialize
    /// the AVM side of the `NetStream` later, by using `set_avm2_object`.
    pub fn new_avm2(gc_context: &Mutation<'gc>) -> Self {
        Self::new(gc_context, None)
    }

    fn new(gc_context: &Mutation<'gc>, avm_object: Option<NetStreamKind<'gc>>) -> Self {
        // IMPORTANT: When adding new fields consider if they need to be
        //     added here or to NetStreamSource.
        Self(Gc::new(
            gc_context,
            NetStreamData {
                source: Lock::new(Gc::new(gc_context, Default::default())),
                buffer_time: Cell::new(0.1),
                last_decoded_bitmap: RefCell::new(None),
                avm_object: Lock::new(avm_object),
                avm2_client: Lock::new(None),
                url: RefCell::new(None),
                attached_to: Lock::new(None),
                playing: Cell::new(false),
            },
        ))
    }

    fn source(self) -> Gc<'gc, NetStreamSource> {
        self.0.source.get()
    }

    pub fn set_client(self, gc_context: &Mutation<'gc>, new_client: Avm2Object<'gc>) {
        unlock!(Gc::write(gc_context, self.0), NetStreamData, avm2_client).set(Some(new_client));
    }

    pub fn client(self) -> Option<Avm2Object<'gc>> {
        self.0.avm2_client.get()
    }

    pub fn set_avm2_object(self, gc_context: &Mutation<'gc>, object: NetStreamObject<'gc>) {
        let write = Gc::write(gc_context, self.0);
        unlock!(write, NetStreamData, avm_object).set(Some(NetStreamKind::Avm2(object)));
    }

    fn set_attached_to(self, gc_context: &Mutation<'gc>, attached_to: Option<MovieClip<'gc>>) {
        unlock!(Gc::write(gc_context, self.0), NetStreamData, attached_to).set(attached_to);
    }

    /// Reset the `NetStream` buffer to accept new source data.
    ///
    /// This must be done once per source change and should ideally be done
    /// immediately before the first `load_buffer` call for a particular source
    /// file.
    ///
    /// Externally visible AVM state must not be reinitialized here - i.e. the
    /// AS3 `client` doesn't go away because you played a new video file.
    fn reset_buffer(self, context: &mut UpdateContext<'gc>) {
        if let Some(instance) = self.source().sound_instance.get() {
            // We stop the sound twice because sounds may have either been
            // played through the audio manager or through the backend directly
            // depending on the attachment state at the time of first audio
            // playback.
            context.audio.stop_sound(instance);
            context.audio_manager.stop_sound(context.audio, instance);
        }

        unlock!(Gc::write(context.gc(), self.0), NetStreamData, source)
            .set(Gc::new(context.gc(), Default::default()));
    }

    /// Set the total number of bytes expected to be downloaded.
    pub fn set_expected_length(self, expected: usize) {
        let source = self.source();
        let mut buffer = source.buffer.borrow_mut();
        let len = buffer.len();

        // The subtract is to avoid reserving space for already-downloaded data.
        if expected > len {
            buffer.reserve(expected - len);
        }

        source.expected_length.set(Some(expected));
    }

    /// Append data to the `NetStream`'s current internal buffer.
    ///
    /// If you are loading data from a new source, you must first initialize
    /// the buffer, otherwise existing buffer contents will remain and be
    /// incorrectly parsed.
    ///
    /// Buffer loading can be done in chunks but must be done in such a way
    /// that all data is appended in the correct order and that data from
    /// separate streams is not mixed together.
    pub fn load_buffer(self, context: &mut UpdateContext<'gc>, data: &mut Vec<u8>) {
        self.source().buffer.borrow_mut().append(data);

        StreamManager::activate(context, self);

        // NOTE: The onMetaData event triggers before this event in Flash due to its streaming behavior.
        self.trigger_status_event(
            context,
            [("code", "NetStream.Buffer.Full"), ("level", "status")],
        );
    }

    /// Indicate that the buffer has finished loading and that no further data
    /// is expected to be downloaded to it.
    pub fn finish_buffer(self) {
        self.source().expected_length.set(None);
    }

    pub fn report_error(self, _error: Error) {
        // TODO: Report an `asyncError` to AVM1 or 2.
    }

    pub fn bytes_loaded(self) -> usize {
        self.source().buffer.borrow().len()
    }

    pub fn bytes_total(self) -> usize {
        let source = self.source();
        let buflen = source.buffer.borrow().len();
        std::cmp::max(source.expected_length.get().unwrap_or(buflen), buflen)
    }

    pub fn time(self) -> f64 {
        self.source().stream_time.get()
    }

    pub fn buffer_time(self) -> f64 {
        self.0.buffer_time.get()
    }

    pub fn set_buffer_time(self, buffer_time: f64) {
        self.0.buffer_time.set(buffer_time);
    }

    /// Queue a seek to be executed on the next frame tick.
    ///
    /// `offset` is in milliseconds.
    pub fn seek(self, context: &mut UpdateContext<'gc>, offset: f64, notify: bool) {
        self.source().queued_seek_time.set(Some(offset));
        StreamManager::activate(context, self);

        if notify {
            let trigger = format!("Start Seeking {}", offset as u64);
            self.trigger_status_event(
                context,
                [
                    ("description", trigger.as_str()),
                    ("level", "status"),
                    ("code", "NetStream.SeekStart.Notify"),
                ],
            );
        }
    }

    /// Seek to a new position in the stream.
    ///
    /// All existing audio will be paused. The stream offset will be snapped to
    /// either the prior or next keyframe depending on seek direction. If the
    /// stream is playing then new tag processing will occur when the stream
    /// ticks next.
    ///
    /// This always does an in-buffer seek. Seek-driven requests are not
    /// currently supported. When progressive download is implemented this seek
    /// algorithm will need to detect out-of-buffer seeks and trigger fresh
    /// downloads.
    ///
    /// `offset` is in milliseconds.
    ///
    /// This function should be run during stream ticks and *not* called by AVM
    /// code to service seek requests.
    pub fn execute_seek(self, context: &mut UpdateContext<'gc>, offset: f64) {
        self.trigger_status_event(
            context,
            [("code", "NetStream.Seek.Notify"), ("level", "status")],
        );

        let source = self.source();

        // Ensure the container stream type is known before continuing.
        if source.stream_type.borrow().is_none() && !self.sniff_stream_type(context) {
            return;
        }

        if source.stream_time.get() == offset {
            //Don't do anything for no-op seeks.
            return;
        }

        if let Some(sound) = source.sound_instance.get() {
            context.stop_sound(sound);
            context.audio.stop_sound(sound);

            source.sound_instance.set(None);
            source.audio_stream.replace(None);
        }

        if matches!(
            &*source.stream_type.borrow(),
            Some(NetStreamType::Flv { .. })
        ) {
            let slice = source.buffer.borrow().to_full_slice();
            let buffer = slice.data();
            let mut reader = FlvReader::from_parts(&buffer, source.offset.get());
            let skipping_back = source.stream_time.get() > offset;

            loop {
                if skipping_back {
                    let res = FlvTag::skip_back(&mut reader);
                    if matches!(res, Err(FlvError::EndOfData)) {
                        //At start of video, can't skip further back
                        break;
                    }

                    if let Err(e) = res {
                        tracing::error!("FLV tag parsing failed during seek backward: {}", e);
                        break;
                    }
                }

                let old_position = reader
                    .stream_position()
                    .expect("valid stream position when seeking");

                let tag = FlvTag::parse(&mut reader);
                if matches!(tag, Err(FlvError::EndOfData)) {
                    //At end of video, can't skip further forward
                    break;
                }

                if let Err(e) = tag {
                    tracing::error!("FLV tag parsing failed during seek forward: {}", e);
                    break;
                }

                if skipping_back {
                    //Tag position won't actually move backwards if we don't do this.
                    reader
                        .seek(SeekFrom::Start(old_position))
                        .expect("valid backseek position");
                }

                let tag = tag.unwrap();
                let stream_time = tag.timestamp as f64;
                source.stream_time.set(stream_time);

                if skipping_back && stream_time > offset || !skipping_back && stream_time < offset {
                    continue;
                }

                match tag.data {
                    FlvTagData::Video(FlvVideoData {
                        frame_type: FlvFrameType::Keyframe,
                        ..
                    }) => {
                        // If we don't backseek when we find the keyframe,
                        // we will miss the keyframe.
                        reader
                            .seek(SeekFrom::Start(old_position))
                            .expect("valid backseek position");

                        break;
                    }
                    _ => continue,
                }
            }

            let offset = reader
                .stream_position()
                .expect("FLV reader stream position") as usize;
            source.rewind_cursors_to(offset);
        }

        // Anything the decoder is still holding on to belongs to where we just
        // came from, so it must not be allowed to surface after the jump.
        if let Some(NetStreamType::Flv {
            video_stream: Some(video_stream),
            ..
        }) = &*source.stream_type.borrow()
            && let Err(e) = context.video.reset_video_stream(*video_stream)
        {
            tracing::error!("Resetting video stream failed: {}", e);
        }

        if let Some(NetStreamKind::Avm2(_)) = self.0.avm_object.get() {
            self.trigger_status_event(
                context,
                [
                    ("description", "Seek Complete -1"),
                    ("level", "status"),
                    ("code", "NetStream.Seek.Complete"),
                ],
            );
        }
    }

    /// Start playing media from this NetStream.
    ///
    /// If `name` is specified, this will also trigger streaming download of
    /// the given resource. Otherwise, the stream will play whatever data is
    /// available in the buffer.
    pub fn play(self, context: &mut UpdateContext<'gc>, name: Option<AvmString<'gc>>) {
        if let Some(name) = name {
            let request = if let Ok(stream_url) = Url::parse(context.root_swf.url())
                .and_then(|url| url.join(name.to_string().as_str()))
            {
                Request::get(stream_url.to_string())
            } else {
                Request::get(name.to_string())
            };
            self.0.url.replace(Some(request.url().to_string()));
            self.source().preload_offset.set(0);
            self.reset_buffer(context);

            let future = crate::loader::load_netstream(context, self, request);

            context.navigator.spawn_future(future);
        }

        self.0.playing.set(true);
        StreamManager::activate(context, self);

        self.trigger_status_event(
            context,
            [("code", "NetStream.Play.Start"), ("level", "status")],
        );
    }

    /// Pause stream playback.
    pub fn pause(self, context: &mut UpdateContext<'gc>, notify: bool) {
        // NOTE: We do not deactivate the stream here as there may be other
        // work to be done at tick time.
        self.0.playing.set(false);

        if notify {
            self.trigger_status_event(
                context,
                [
                    ("description", "Pausing"),
                    ("level", "status"),
                    ("code", "NetStream.Pause.Notify"),
                ],
            );
        }
    }

    /// Resume stream playback.
    pub fn resume(self, context: &mut UpdateContext<'gc>) {
        self.0.playing.set(true);
        StreamManager::activate(context, self);
    }

    /// Resume stream playback if paused, pause otherwise.
    pub fn toggle_paused(self, context: &mut UpdateContext<'gc>) {
        self.0.playing.set(!self.0.playing.get());

        if self.0.playing.get() {
            StreamManager::activate(context, self);
        }
    }

    /// Indicates that this `NetStream`'s audio was detached from a `MovieClip` (AVM1)
    pub fn was_detached(self, context: &mut UpdateContext<'gc>) {
        let source = self.source();
        if let Some(sound_instance) = source.sound_instance.get() {
            context
                .audio_manager
                .stop_sound(context.audio, sound_instance);
        }

        source.audio_stream.replace(None);
        self.set_attached_to(context.gc(), None);
    }

    /// Indicates that this `NetStream`'s audio was attached to a `MovieClip` (AVM1)
    pub fn was_attached(self, context: &mut UpdateContext<'gc>, clip: MovieClip<'gc>) {
        let source = self.source();

        // A `NetStream` cannot be attached to two `MovieClip`s at once.
        // Stop the old sound; the new one will stream at the next tag read.
        // TODO: Change this to have `audio_manager` just switch the sound
        // transforms around
        if let Some(sound_instance) = source.sound_instance.get() {
            context
                .audio_manager
                .stop_sound(context.audio, sound_instance);
        }

        source.audio_stream.replace(None);
        self.set_attached_to(context.gc(), Some(clip));
    }

    /// Process a parsed FLV audio tag.
    ///
    /// `write` must be an active borrow of the current `NetStream`. `slice`
    /// must reference the underlying backing buffer.
    fn flv_audio_tag(
        self,
        slice: &Slice,
        audio_data: FlvAudioData<'_>,
    ) -> Result<(), NetstreamError> {
        let is_aac_sequence_header =
            matches!(audio_data.data, FlvAudioDataType::AacSequenceHeader(_));
        let data = match audio_data.data {
            FlvAudioDataType::Raw(data)
            | FlvAudioDataType::AacSequenceHeader(data)
            | FlvAudioDataType::AacRaw(data) => slice.to_subslice(data),
        };
        let source = self.source();
        let audio_stream = &mut *source.audio_stream.borrow_mut();
        let (substream, sound_stream_info) = match audio_stream {
            Some(audio_stream) => audio_stream,
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
                    extra_data: None,
                };

                audio_stream.insert((substream, sound_stream_head))
            }
        };

        // An AAC sequence header carries the decoder configuration
        // (`AudioSpecificConfig`), not playable audio. We demux it out-of-band
        // into the stream info instead of appending it to the audio substream,
        // so the decoder only ever sees raw AAC access units and never has to
        // deal with FLV's packet framing.
        //
        // The decoder reads this config once, when it's constructed. A re-sent
        // header just refreshes the field (a no-op for the identical configs FLV
        // actually uses); a genuine mid-stream config *change* is not supported,
        // but doesn't occur in practice.
        if is_aac_sequence_header {
            sound_stream_info.extra_data = Some((*data.data()).into());
            return Ok(());
        }

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

        Ok(substream.append(data)?)
    }

    /// Determine if the given sound is currently playing.
    fn sound_currently_playing(
        context: &mut UpdateContext<'gc>,
        sound: Option<SoundInstanceHandle>,
    ) -> bool {
        sound
            .map(|si| context.audio.is_sound_playing(si))
            .unwrap_or(false)
    }

    /// Clean up after a sound instance that has finished playing.
    ///
    /// Generally speaking, streams are only to be used once. However, the
    /// audio backend will only retain information about sounds that are
    /// currently playing, so if the sound has finished since the last tick, we
    /// need to restart it.
    ///
    /// Intended to be called at the start of tag processing, before any new
    /// audio data has been streamed.
    fn cleanup_sound_stream(self, context: &mut UpdateContext<'gc>) {
        let source = self.source();
        if !Self::sound_currently_playing(context, source.sound_instance.get()) {
            source.audio_stream.replace(None);
            source.sound_instance.set(None);
        }
    }

    /// Ensure that if we have queued up audio into a sound stream, that said
    /// stream gets sent over to the audio backend.
    ///
    /// Intended to be called at the end of tag processing. Audio processing
    /// should occur only after a minimum number of tags have been processed to
    /// avoid audio underruns.
    fn commit_sound_stream(self, context: &mut UpdateContext<'gc>) -> Result<(), NetstreamError> {
        let source = self.source();
        if !Self::sound_currently_playing(context, source.sound_instance.get())
            && let Some((substream, sound_stream_head)) = &mut *source.audio_stream.borrow_mut()
        {
            let sound_instance = if let Some(mc) = self.0.attached_to.get() {
                context.audio_manager.start_substream(
                    context.audio,
                    substream.clone(),
                    mc,
                    sound_stream_head,
                )?
            } else {
                context
                    .audio
                    .start_substream(substream.clone(), sound_stream_head)?
            };
            source.sound_instance.set(Some(sound_instance));
        }

        Ok(())
    }

    /// Attempt to sniff the stream type from data in the buffer.
    ///
    /// Returns true if the stream type was successfully sniffed. False
    /// indicates that there is either not enough data in the buffer, or the
    /// data is of an unrecognized format. This should be used as a signal to
    /// stop stream processing until new data has been retrieved.
    pub fn sniff_stream_type(self, context: &mut UpdateContext<'gc>) -> bool {
        let source = self.source();
        let slice = source.buffer.borrow().to_full_slice();
        let buffer = slice.data();

        // A nonzero preload offset indicates that we tried and failed to
        // sniff the container format, so in that case do not process the
        // stream anymore.
        if source.preload_offset.get() > 0 {
            return false;
        }

        match buffer.get(0..3) {
            Some([0x46, 0x4C, 0x56]) => {
                let mut reader = FlvReader::from_parts(&buffer, source.offset.get());
                match FlvHeader::parse(&mut reader) {
                    Ok(header) => {
                        source.rewind_cursors_to(reader.into_parts().1);
                        source.preload_offset.set(source.offset.get());
                        source.stream_type.replace(Some(NetStreamType::Flv {
                            header,
                            video_stream: None,
                            frame_id: 0,
                            max_composition_offset: 0,
                            video_frame_interval: 0,
                            last_video_dts: 0,
                        }));
                        true
                    }
                    Err(FlvError::EndOfData) => false,
                    Err(e) => {
                        //TODO: Fire an error event to AS & stop playing too
                        tracing::error!("FLV header parsing failed: {}", e);
                        source.preload_offset.set(3);
                        false
                    }
                }
            }
            Some(magic) => {
                //Unrecognized signature
                //TODO: Fire an error event to AS & stop playing too
                tracing::error!("Unrecognized file signature: {:?}", magic);
                source.preload_offset.set(3);
                if let Some(url) = &*self.0.url.borrow() {
                    if url.is_empty() {
                        return false;
                    }
                    let parsed_url = match context.navigator.resolve_url(url) {
                        Ok(parsed_url) => parsed_url,
                        Err(e) => {
                            tracing::error!(
                                "Could not parse URL because of {}, the corrupt URL was: {}",
                                e,
                                url
                            );
                            return false;
                        }
                    };
                    context.ui.display_unsupported_video(parsed_url);
                }
                false
            }
            None => false, //Data not yet loaded
        }
    }

    /// Hand one video frame to the decoder, to be shown at `pts`.
    fn submit_video_frame(
        context: &mut UpdateContext<'gc>,
        video_handle: VideoStreamHandle,
        encoded_frame: EncodedFrame<'_>,
        pts: PresentationTime,
    ) {
        let frame_id = encoded_frame.frame_id;

        if let Err(e) = context
            .video
            .submit_video_stream_frame(video_handle, encoded_frame, pts)
        {
            tracing::error!("Decoding video frame {} failed: {}", frame_id, e);
        }
    }

    /// Put whatever video frame is due at `playhead` on screen.
    fn present_video_frame(self, context: &mut UpdateContext<'gc>, playhead: f64) {
        let video_handle = match &*self.source().stream_type.borrow() {
            Some(NetStreamType::Flv { video_stream, .. }) => *video_stream,
            None => None,
        };
        let Some(video_handle) = video_handle else {
            return;
        };

        match context.video.present_video_stream_frame(
            video_handle,
            playhead as PresentationTime,
            context.renderer,
        ) {
            Ok(Presentation::Changed(bitmap_info)) => {
                self.0.last_decoded_bitmap.replace(Some(bitmap_info));
                if let Some(mc) = self.0.attached_to.get() {
                    mc.invalidate_cached_bitmap();
                    *context.needs_render = true;
                }
            }
            Ok(Presentation::Unchanged | Presentation::Empty) => {}
            Err(e) => tracing::error!("Presenting video frame at {}ms failed: {}", playhead, e),
        }
    }

    /// Process a parsed FLV video tag.
    ///
    /// `write` must be an active borrow of the current `NetStream`. `slice`
    /// must reference the underlying backing buffer.
    ///
    /// `dts` is the tag's own timestamp, which is when the frame has to be
    /// *decoded*. When it has to be *shown* can be later than that, and is
    /// carried separately by the bitstream's composition time offset.
    ///
    /// `tag_needs_preloading` indicates that this video tag has not been
    /// encountered before.
    fn flv_video_tag(
        self,
        context: &mut UpdateContext<'gc>,
        slice: &Slice,
        video_data: FlvVideoData<'_>,
        dts: PresentationTime,
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

                // None of these codecs reorder, so a frame is shown at the same
                // time it is decoded.
                self.note_video_timing(dts, 0);
                Self::submit_video_frame(context, video_handle, encoded_frame, dts);
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
                    composition_time_offset,
                    data,
                },
            ) => {
                let encoded_frame = EncodedFrame {
                    codec,
                    data,
                    frame_id,
                };

                // H.264 frames arrive in the order they have to be decoded in,
                // which with bidirectional prediction is not the order they are
                // shown in. The composition time offset is the difference.
                let pts = dts + composition_time_offset as PresentationTime;
                self.note_video_timing(dts, composition_time_offset as i64);
                Self::submit_video_frame(context, video_handle, encoded_frame, pts);
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

    /// Note how far ahead of presentation this stream decodes, so the feed
    /// cursor knows how far ahead of the playhead it has to run.
    fn note_video_timing(self, dts: PresentationTime, composition_offset: i64) {
        let source = self.source();
        let Some(NetStreamType::Flv {
            max_composition_offset,
            video_frame_interval,
            last_video_dts,
            ..
        }) = &mut *source.stream_type.borrow_mut()
        else {
            return;
        };

        *max_composition_offset = (*max_composition_offset).max(composition_offset);

        let delta = dts - *last_video_dts;
        if delta > 0 {
            *video_frame_interval = delta;
        }
        *last_video_dts = dts;
    }

    /// Set up the video stream a script tag's `onMetaData` describes.
    ///
    /// This is the half of script tag processing that belongs to the feed
    /// cursor: the decoder has to exist before any frame can be handed to it,
    /// and that has to happen whether or not the playhead has reached the tag
    /// yet. Only the first `onMetaData` in a stream is acted on.
    fn flv_script_tag_preload(
        self,
        context: &mut UpdateContext<'gc>,
        script_data: FlvScriptData<'_>,
    ) {
        let source = self.source();
        let has_stream_already = match &*source.stream_type.borrow() {
            Some(NetStreamType::Flv { video_stream, .. }) => video_stream.is_some(),
            _ => unreachable!(),
        };

        if has_stream_already {
            return;
        }

        let mut width = None;
        let mut height = None;
        let mut video_codec_id = None;
        let mut frame_rate = None;
        let mut duration = None;

        for var in script_data.0 {
            if var.name != b"onMetaData" {
                continue;
            }

            match var.data {
                FlvValue::Object(subvars) | FlvValue::EcmaArray(subvars) => {
                    for subvar in subvars {
                        match (subvar.name, subvar.data) {
                            (b"width", FlvValue::Number(val)) => width = Some(val),
                            (b"height", FlvValue::Number(val)) => height = Some(val),
                            (b"videocodecid", FlvValue::Number(val)) => video_codec_id = Some(val),
                            (b"framerate", FlvValue::Number(val)) => frame_rate = Some(val),
                            (b"duration", FlvValue::Number(val)) => duration = Some(val),
                            _ => {}
                        }
                    }
                }
                _ => tracing::error!("Invalid FLV metadata tag!"),
            }
        }

        let (Some(width), Some(height), Some(video_codec_id), Some(frame_rate), Some(duration)) =
            (width, height, video_codec_id, frame_rate, duration)
        else {
            return;
        };

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

    /// Hand a script tag's variables to the AVM.
    ///
    /// This is the half of script tag processing that belongs to the
    /// presentation cursor, so that callbacks stay in step with what the viewer
    /// is seeing instead of running as early as the decoders are fed.
    ///
    /// This function attempts to borrow the current `NetStream`, you must drop
    /// any existing borrows and pick them back up when you're done.
    fn flv_script_tag_dispatch(
        self,
        context: &mut UpdateContext<'gc>,
        script_data: FlvScriptData<'_>,
    ) {
        for var in script_data.0 {
            let avm_object = self.0.avm_object.get();
            // This is necessary because the script callback functions can call back into
            // these methods, (e.g. NetStream::play), so we need to avoid holding a borrow
            // while the script data is being handled.
            let _ = self.handle_script_data(avm_object, context, var.name, var.data);
            // Any errors while trying to lookup or call AVM2 properties are silently swallowed.
        }
    }

    /// Process stream data.
    ///
    /// `dt` is the elapsed time since the last tick.
    pub fn tick(self, context: &mut UpdateContext<'gc>, dt: FloatDuration) {
        let source = self.source();
        let seek_offset = source.queued_seek_time.take();
        if let Some(offset) = seek_offset {
            self.execute_seek(context, offset);
        }

        // Paused streams deactivate themselves after seek processing.
        if !self.0.playing.get() {
            StreamManager::deactivate(context, self);
            return;
        }

        // Ensure the container stream type is known before continuing.
        if source.stream_type.borrow().is_none() && !self.sniff_stream_type(context) {
            return;
        }

        self.cleanup_sound_stream(context);

        let playhead = source.stream_time.get() + dt.as_millis();

        // Hand the decoders everything up to the feed horizon first, so that
        // whatever is due at the playhead has had as long as possible to come
        // back out again.
        self.run_feed_cursor(context, playhead + self.feed_lookahead());

        // Then move the playhead itself. Script callbacks run from here rather
        // than from the feed cursor, so the lookahead cannot make them fire
        // early.
        let stop = self.run_presentation_cursor(context, playhead);

        self.present_video_frame(context, playhead);

        source.stream_time.set(playhead);
        if let Err(e) = self.commit_sound_stream(context) {
            //TODO: Fire an error event at AS.
            tracing::error!("Error committing sound stream: {}", e);
        }

        // Running out of tags is not the end of playback on its own: several
        // pictures can still be waiting for their turn on screen, and with
        // bidirectional prediction the last of them is due well after the last
        // tag's own timestamp.
        if stop == CursorStop::OutOfData && self.video_is_drained(context) {
            let is_end_of_video = source.expected_length.get().is_none();

            self.trigger_status_event(
                context,
                [("code", "NetStream.Buffer.Flush"), ("level", "status")],
            );

            if is_end_of_video {
                self.trigger_status_event(
                    context,
                    [("code", "NetStream.Play.Stop"), ("level", "status")],
                );
            }

            // Check if AVM code in the event handler invoked stream.play() and replaced the source.
            if Gc::ptr_eq(source, self.source()) {
                self.trigger_status_event(
                    context,
                    [("code", "NetStream.Buffer.Empty"), ("level", "status")],
                );

                if is_end_of_video {
                    self.pause(context, false);
                }
            }
        }

        if stop == CursorStop::Corrupt {
            //TODO: Fire an error event at AS.
            self.pause(context, false);
        }
    }

    /// Whether the video decoder has no pictures left to show.
    ///
    /// True when there is no video at all, so that an audio-only stream is not
    /// held open waiting for one.
    fn video_is_drained(self, context: &mut UpdateContext<'gc>) -> bool {
        let video_stream = match &*self.source().stream_type.borrow() {
            Some(NetStreamType::Flv { video_stream, .. }) => *video_stream,
            None => None,
        };

        video_stream.is_none_or(|handle| context.video.video_stream_is_drained(handle))
    }

    /// How far past the playhead the feed cursor runs, in milliseconds.
    ///
    /// For video this comes from the stream itself rather than from a guess:
    /// the largest composition time offset it uses is exactly how far ahead of
    /// presentation it expects to be decoded, and one more frame on top covers
    /// a decoder that releases a picture a little later than it has to.
    fn feed_lookahead(self) -> f64 {
        let video = match &*self.source().stream_type.borrow() {
            Some(NetStreamType::Flv {
                max_composition_offset,
                video_frame_interval,
                ..
            }) => (*max_composition_offset + *video_frame_interval) as f64,
            None => 0.0,
        };

        video.max(AUDIO_LOOKAHEAD_MS)
    }

    /// Hand the decoders every tag up to `horizon`.
    ///
    /// This runs ahead of the playhead, so that a decoder which cannot produce
    /// a picture the moment it is given a frame still has it ready by the time
    /// it is due, and so that the audio backend does not run dry between ticks.
    /// Script tags are only looked at here for the stream setup they carry;
    /// their callbacks belong to the presentation cursor.
    fn run_feed_cursor(self, context: &mut UpdateContext<'gc>, horizon: f64) {
        let source = self.source();
        if !matches!(
            &*source.stream_type.borrow(),
            Some(NetStreamType::Flv { .. })
        ) {
            return;
        }

        let slice = source.buffer.borrow().to_full_slice();
        let buffer = slice.data();
        let mut reader = FlvReader::from_parts(&buffer, source.feed_offset.get());

        loop {
            // Out of data, or the stream is corrupt; either way there is
            // nothing more to feed. The presentation cursor is what reports
            // both of those, once the playhead gets there.
            let Ok(tag) = FlvTag::parse(&mut reader) else {
                self.flush_video_at_end_of_stream(context);
                break;
            };

            // FLV timestamps are also ms. Leaving `feed_offset` where it is
            // means this tag gets picked up again on a later tick.
            if tag.timestamp as f64 >= horizon {
                break;
            }

            let tag_needs_preloading = reader.stream_position().expect("valid position") as usize
                >= source.preload_offset.get();

            match tag.data {
                FlvTagData::Audio(audio_data) => {
                    if let Err(e) = self.flv_audio_tag(&slice, audio_data) {
                        //TODO: Fire an error event at AS.
                        tracing::error!("Error queueing sound stream: {}", e);
                    }
                }
                FlvTagData::Video(video_data) => self.flv_video_tag(
                    context,
                    &slice,
                    video_data,
                    tag.timestamp as PresentationTime,
                    tag_needs_preloading,
                ),
                FlvTagData::Script(script_data) => {
                    if tag_needs_preloading {
                        self.flv_script_tag_preload(context, script_data);
                    }
                }
                FlvTagData::Invalid(e) => {
                    tracing::error!("FLV data parsing failed: {}", e)
                }
            }

            let offset = reader
                .stream_position()
                .expect("FLV reader stream position") as usize;
            source.feed_offset.set(offset);
            source
                .preload_offset
                .set(max(offset, source.preload_offset.get()));
        }
    }

    /// Let the decoder know that the last frame has been handed over, once the
    /// feed cursor has run out of tags and there is no more data coming.
    ///
    /// A decoder holding frames back for reordering has no way to know that the
    /// stream has ended, so without this the final few pictures are never
    /// produced at all. Draining a decoder that has nothing buffered is a
    /// no-op, so it does not matter that this happens on every tick from here
    /// on.
    fn flush_video_at_end_of_stream(self, context: &mut UpdateContext<'gc>) {
        let source = self.source();

        // More data is still on its way, so running out of tags only means the
        // download has not caught up yet.
        if source.expected_length.get().is_some() {
            return;
        }

        let Some(NetStreamType::Flv {
            video_stream: Some(video_stream),
            ..
        }) = &*source.stream_type.borrow()
        else {
            return;
        };

        if let Err(e) = context.video.flush_video_stream(*video_stream) {
            tracing::error!("Flushing video stream failed: {}", e);
        }
    }

    /// Walk the tag stream up to `playhead`, running the script callbacks that
    /// have come due.
    ///
    /// Audio and video tags are skipped here: the feed cursor handed them over
    /// already, possibly several ticks ago.
    fn run_presentation_cursor(
        self,
        context: &mut UpdateContext<'gc>,
        playhead: f64,
    ) -> CursorStop {
        let source = self.source();
        if !matches!(
            &*source.stream_type.borrow(),
            Some(NetStreamType::Flv { .. })
        ) {
            return CursorStop::CaughtUp;
        }

        let slice = source.buffer.borrow().to_full_slice();
        let buffer = slice.data();
        let mut reader = FlvReader::from_parts(&buffer, source.offset.get());

        loop {
            let tag = match FlvTag::parse(&mut reader) {
                Ok(tag) => tag,
                Err(FlvError::EndOfData) => return CursorStop::OutOfData,
                Err(e) => {
                    tracing::error!("FLV tag parsing failed: {}", e);
                    return CursorStop::Corrupt;
                }
            };

            if tag.timestamp as f64 >= playhead {
                return CursorStop::CaughtUp;
            }

            if let FlvTagData::Script(script_data) = tag.data {
                self.flv_script_tag_dispatch(context, script_data);

                // A callback may have called `play()`, which swaps in a fresh
                // source; the cursor we are holding no longer describes it.
                if !Gc::ptr_eq(source, self.source()) {
                    return CursorStop::SourceReplaced;
                }
            }

            source.offset.set(
                reader
                    .stream_position()
                    .expect("FLV reader stream position") as usize,
            );
        }
    }

    pub fn last_decoded_bitmap(self) -> Option<BitmapInfo> {
        self.0.last_decoded_bitmap.borrow().clone()
    }

    /// Trigger a status event on the stream.
    pub fn trigger_status_event<'a>(
        self,
        context: &mut UpdateContext<'gc>,
        values: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) {
        let object = self.0.avm_object.get();
        match object {
            Some(NetStreamKind::Avm1(object)) => {
                let root = context.stage.root_clip().expect("root");
                let mut activation = Avm1Activation::from_nothing(
                    context,
                    Avm1ActivationIdentifier::root("[NetStream Status Event]"),
                    root,
                );
                let object_proto = activation.prototypes().object;
                let info_object = Avm1Object::new(&activation.context.strings, Some(object_proto));

                for (key, value) in values {
                    let key = AvmString::new_utf8(activation.gc(), key);
                    let value = AvmString::new_utf8(activation.gc(), value);

                    info_object
                        .set(key, value, &mut activation)
                        .expect("valid set");
                }

                if let Err(e) = object.call_method(
                    istr!("onStatus"),
                    &[info_object.into()],
                    &mut activation,
                    Avm1ExecutionReason::Special,
                ) {
                    tracing::error!(
                        "Got error when dispatching AVM1 onStatus event from NetStream: {}",
                        e
                    );
                }
            }
            Some(NetStreamKind::Avm2(object)) => {
                let domain = context.avm2.stage_domain();
                let mut activation = Avm2Activation::from_domain(context, domain);
                let net_status_event = Avm2EventObject::net_status_event(&mut activation, values);
                Avm2::dispatch_event(activation.context, net_status_event, object.into());
            }
            None => {}
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
