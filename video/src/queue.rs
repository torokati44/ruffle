use crate::error::Error;
use crate::frame::{DecodedFrame, DecodedFrameOut, PresentationTime};
use ruffle_render::backend::RenderBackend;
use ruffle_render::bitmap::{BitmapHandle, BitmapInfo, PixelRegion};
use std::collections::BTreeMap;

/// A backstop on how many decoded pictures a stream may hold at once.
///
/// This is not what normally bounds the queue - how far ahead of the playhead
/// the caller feeds the decoder is - so it is set well above any depth real
/// playback reaches, and exists only so that a caller which submits without
/// ever presenting cannot grow it without limit.
const MAX_DEPTH: usize = 32;

/// The outcome of moving a stream's presentation clock forward.
#[derive(Clone, Debug)]
pub enum Presentation {
    /// A new picture was uploaded, and is now the current frame.
    Changed(BitmapInfo),

    /// Nothing newer was due, so whatever was already on screen stays there.
    Unchanged,

    /// This stream has never presented anything.
    Empty,
}

/// Decoded pictures waiting for their turn on screen.
///
/// This is a presentation gate first and a reorder buffer only in principle:
/// both OpenH264 and WebCodecs already emit in presentation order, so in
/// practice `ready` fills in ascending key order and behaves as a plain FIFO.
/// It is keyed by presentation time anyway so that a decoder which emits in
/// decode order instead - or one that skips a picture after an error, which
/// would permanently desynchronise any positional pairing - still comes out
/// right here rather than needing a second mechanism.
///
/// Nothing reaches the renderer until it is actually due, so a stream that has
/// fallen behind drops its late frames without ever paying for an upload.
pub struct PresentationQueue {
    /// Decoded, not yet shown, keyed by presentation time.
    ready: BTreeMap<PresentationTime, DecodedFrame>,

    /// The presentation time of every frame that has been submitted to the
    /// decoder but has not come back out of it yet.
    in_flight: BTreeMap<u32, PresentationTime>,

    /// The picture that is on screen right now, and when it was due.
    current: Option<(PresentationTime, BitmapInfo)>,

    /// The texture the current picture lives in, reused across frames.
    bitmap: Option<BitmapHandle>,
}

impl Default for PresentationQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl PresentationQueue {
    pub fn new() -> Self {
        Self {
            ready: BTreeMap::new(),
            in_flight: BTreeMap::new(),
            current: None,
            bitmap: None,
        }
    }

    /// Note that `frame_id` has been handed to the decoder, and is meant to be
    /// shown at `pts`.
    pub fn submitted(&mut self, frame_id: u32, pts: PresentationTime) {
        self.in_flight.insert(frame_id, pts);
    }

    /// Take everything a decoder has just produced and file it by the
    /// presentation time recorded for it in `submitted`.
    pub fn absorb(&mut self, polled: &mut Vec<DecodedFrameOut>) {
        for out in polled.drain(..) {
            let Some(pts) = self.in_flight.remove(&out.frame_id) else {
                tracing::warn!(
                    "Decoder produced frame {} which was never submitted",
                    out.frame_id
                );
                continue;
            };

            self.ready.insert(pts, out.frame);
        }

        // Going over depth means frames are arriving faster than they are being
        // shown, which is routine rather than exceptional - a seek that sweeps
        // from a keyframe does it on purpose. The oldest frame is the one that
        // was going to be skipped anyway, so drop that.
        while self.ready.len() > MAX_DEPTH {
            self.ready.pop_first();
        }
    }

    /// Move the presentation clock to `up_to`, putting the newest picture that
    /// is due on screen.
    pub fn present(
        &mut self,
        up_to: PresentationTime,
        renderer: &mut dyn RenderBackend,
    ) -> Result<Presentation, Error> {
        let Some(due) = self.ready.range(..=up_to).next_back().map(|(pts, _)| *pts) else {
            return Ok(self.hold());
        };

        if self
            .current
            .as_ref()
            .is_some_and(|(shown, _)| *shown >= due)
        {
            return Ok(Presentation::Unchanged);
        }

        // Anything older than `due` has missed its turn and will never be
        // shown, so drop it without ever handing it to the renderer.
        while self
            .ready
            .first_key_value()
            .is_some_and(|(pts, _)| *pts < due)
        {
            self.ready.pop_first();
        }

        let frame = self.ready.remove(&due).expect("located just above");
        let info = self.upload(frame, renderer)?;
        self.current = Some((due, info.clone()));

        Ok(Presentation::Changed(info))
    }

    /// The picture currently on screen, if any.
    pub fn current(&self) -> Option<BitmapInfo> {
        self.current.as_ref().map(|(_, info)| info.clone())
    }

    /// Whether everything submitted has now been shown or dropped.
    pub fn is_drained(&self) -> bool {
        self.ready.is_empty() && self.in_flight.is_empty()
    }

    /// Forget everything that has not been shown yet, because the stream has
    /// jumped somewhere else.
    ///
    /// The picture on screen is kept: Flash Player leaves the last frame up
    /// across a seek rather than blanking the video.
    pub fn reset(&mut self) {
        self.ready.clear();
        self.in_flight.clear();
    }

    fn hold(&self) -> Presentation {
        match self.current {
            Some(_) => Presentation::Unchanged,
            None => Presentation::Empty,
        }
    }

    fn upload(
        &mut self,
        frame: DecodedFrame,
        renderer: &mut dyn RenderBackend,
    ) -> Result<BitmapInfo, Error> {
        let width = frame.width();
        let height = frame.height();

        let handle = if let Some(bitmap) = self.bitmap.clone() {
            renderer.update_texture(&bitmap, frame, PixelRegion::for_whole_size(width, height))?;
            bitmap
        } else {
            renderer.register_bitmap(frame)?
        };
        self.bitmap = Some(handle.clone());

        Ok(BitmapInfo {
            handle,
            width,
            height,
        })
    }
}
