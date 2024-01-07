//! NetStream implementation

mod flv;

use crate::avm1::{
    Activation as Avm1Activation, ActivationIdentifier as Avm1ActivationIdentifier,
    ExecutionReason as Avm1ExecutionReason, FlvValueAvm1Ext, Object as Avm1Object,
    Value as Avm1Value,
};
use crate::avm2::object::NetStreamObject;
use crate::avm2::{
    Activation as Avm2Activation, Avm2, Error as Avm2Error, EventObject as Avm2EventObject,
    FlvValueAvm2Ext, FunctionArgs, Object as Avm2Object, Value as Avm2Value,
};
use crate::backend::audio::{DecodeError, SoundInstanceHandle, SoundStreamInfo};
use crate::backend::navigator::Request;
use crate::context::UpdateContext;
use crate::display_object::MovieClip;
use crate::loader::Error;
use crate::string::AvmString;
use flv_rs::{Error as FlvError, FlvReader, Header as FlvHeader, Value as FlvValue};
use gc_arena::barrier::unlock;
use gc_arena::{Collect, DynamicRoot, Gc, Lock, Mutation, Rootable};
use ruffle_common::buffer::{Buffer, Substream, SubstreamError};
use ruffle_common::duration::FloatDuration;
use ruffle_macros::istr;
use ruffle_render::bitmap::BitmapInfo;
use ruffle_video::VideoStreamHandle;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use thiserror::Error;
use url::Url;

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
    },
}

#[derive(Clone, Debug, Collect)]
#[collect(no_drop)]
pub struct NetStreamSource {
    /// All data currently loaded in the stream.
    buffer: RefCell<Buffer>,

    /// The buffer position that we are currently seeking to.
    offset: Cell<usize>,

    /// The expected length of the buffer once downloading is complete.
    ///
    /// `None` indicates that downloading is already complete and that the
    /// length of the associated `Buffer` is the final length.
    expected_length: Cell<Option<usize>>,

    /// The buffer position for processing incoming data.
    ///
    /// This points to the first byte that the stream has *never* processed
    /// before in the buffer. It should always be greater than or equal to the
    /// offset position.
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

impl Default for NetStreamSource {
    fn default() -> Self {
        Self {
            buffer: RefCell::new(Buffer::new()),
            offset: Cell::new(0),
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
            self.flv_seek(offset);
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

        match buffer.get(0..8) {
            // Only version 1 is valid.
            Some([b'F', b'L', b'V', 1, _, _, _, _]) => {
                let mut reader = FlvReader::from_parts(&buffer, source.offset.get());
                match FlvHeader::parse(&mut reader) {
                    Ok(header) => {
                        source.offset.set(reader.into_parts().1);
                        source.preload_offset.set(source.offset.get());
                        source.stream_type.replace(Some(NetStreamType::Flv {
                            header,
                            video_stream: None,
                            frame_id: 0,
                        }));
                        true
                    }
                    Err(FlvError::EndOfData) => false,
                    Err(e) => {
                        //TODO: Fire an error event to AS & stop playing too
                        tracing::error!("FLV header parsing failed: {}", e);
                        source.preload_offset.set(8); // ???
                        false
                    }
                }
            }
            Some(magic) => {
                //Unrecognized signature
                //TODO: Fire an error event to AS & stop playing too
                tracing::error!("Unrecognized file signature: {:?}", magic);
                source.preload_offset.set(8); // ???
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

        let max_time = source.stream_time.get() + dt.as_millis();
        let mut buffer_underrun = false;
        let mut error = false;

        // At this point we should know our stream type.
        if matches!(
            &*source.stream_type.borrow(),
            Some(NetStreamType::Flv { .. })
        ) {
            let (flv_underrun, flv_error) = self.flv_tick(context, max_time);
            buffer_underrun |= flv_underrun;
            error |= flv_error;
        }

        source.stream_time.set(max_time);
        if let Err(e) = self.commit_sound_stream(context) {
            //TODO: Fire an error event at AS.
            tracing::error!("Error committing sound stream: {}", e);
        }

        if buffer_underrun {
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

        if error {
            //TODO: Fire an error event at AS.
            self.pause(context, false);
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
                        .set(key, Avm1Value::String(value), &mut activation)
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
