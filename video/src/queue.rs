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

    /// Whether every picture that has come out of the decoder has now been
    /// shown or dropped.
    ///
    /// Frames the decoder still owes are deliberately not counted: once it has
    /// been flushed, anything that has not appeared is not going to, and
    /// waiting on it would leave the stream unable to ever end.
    pub fn is_drained(&self) -> bool {
        self.ready.is_empty()
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

#[cfg(test)]
mod tests {
    use super::*;
    use ruffle_render::backend::ViewportDimensions;
    use ruffle_render::backend::null::NullRenderer;
    use ruffle_render::bitmap::BitmapFormat;

    fn renderer() -> NullRenderer {
        NullRenderer::new(ViewportDimensions {
            width: 1,
            height: 1,
            scale_factor: 1.0,
        })
    }

    /// A one-pixel-tall picture whose width identifies it, so that assertions
    /// can tell which one ended up on screen.
    fn picture(id: u32) -> DecodedFrame {
        DecodedFrame::new(id, 1, BitmapFormat::Rgba, vec![0; id as usize * 4])
    }

    fn submit(queue: &mut PresentationQueue, frame_id: u32, pts: PresentationTime) {
        queue.submitted(frame_id, pts);
    }

    fn deliver(queue: &mut PresentationQueue, frame_id: u32) {
        queue.absorb(&mut vec![DecodedFrameOut {
            frame_id,
            frame: picture(frame_id),
        }]);
    }

    fn shown(queue: &mut PresentationQueue, up_to: PresentationTime) -> Option<u32> {
        match queue
            .present(up_to, &mut renderer())
            .expect("null renderer never fails")
        {
            Presentation::Changed(info) => Some(info.width),
            Presentation::Unchanged | Presentation::Empty => None,
        }
    }

    #[test]
    fn presents_in_presentation_order_not_delivery_order() {
        let mut queue = PresentationQueue::new();

        // Decode order 1, 2, 3 against presentation order 1, 3, 2 - the shape
        // an H.264 stream with a B-frame has.
        submit(&mut queue, 1, 1000);
        submit(&mut queue, 2, 3000);
        submit(&mut queue, 3, 2000);
        deliver(&mut queue, 1);
        deliver(&mut queue, 3);
        deliver(&mut queue, 2);

        assert_eq!(shown(&mut queue, 1000), Some(1));
        assert_eq!(shown(&mut queue, 2000), Some(3));
        assert_eq!(shown(&mut queue, 3000), Some(2));
    }

    #[test]
    fn holds_frames_back_until_they_are_due() {
        let mut queue = PresentationQueue::new();
        submit(&mut queue, 1, 1000);
        deliver(&mut queue, 1);

        assert_eq!(shown(&mut queue, 999), None);
        assert_eq!(shown(&mut queue, 1000), Some(1));
    }

    #[test]
    fn nothing_decoded_yet_reads_as_empty_not_unchanged() {
        let mut queue = PresentationQueue::new();
        assert!(matches!(
            queue.present(1000, &mut renderer()),
            Ok(Presentation::Empty)
        ));

        submit(&mut queue, 1, 1000);
        deliver(&mut queue, 1);
        let _ = shown(&mut queue, 1000);

        assert!(matches!(
            queue.present(1500, &mut renderer()),
            Ok(Presentation::Unchanged)
        ));
    }

    #[test]
    fn skips_over_frames_that_are_already_late() {
        let mut queue = PresentationQueue::new();
        for id in 1..=4 {
            submit(&mut queue, id, id as PresentationTime * 1000);
            deliver(&mut queue, id);
        }

        // Jumping the clock past three of them shows the newest that is due,
        // and the ones passed over are gone rather than shown afterwards.
        assert_eq!(shown(&mut queue, 3000), Some(3));
        assert_eq!(shown(&mut queue, 3999), None);
        assert_eq!(shown(&mut queue, 4000), Some(4));
    }

    #[test]
    fn never_goes_backwards() {
        let mut queue = PresentationQueue::new();
        submit(&mut queue, 1, 1000);
        submit(&mut queue, 2, 2000);
        deliver(&mut queue, 2);
        assert_eq!(shown(&mut queue, 2000), Some(2));

        // An earlier picture turning up after a later one has been shown has
        // missed its turn; it must not replace what is on screen.
        deliver(&mut queue, 1);
        assert_eq!(shown(&mut queue, 2000), None);
    }

    #[test]
    fn reset_drops_pending_frames_but_keeps_the_picture_on_screen() {
        let mut queue = PresentationQueue::new();
        submit(&mut queue, 1, 1000);
        submit(&mut queue, 2, 2000);
        deliver(&mut queue, 1);
        deliver(&mut queue, 2);
        assert_eq!(shown(&mut queue, 1000), Some(1));

        queue.reset();
        assert!(queue.is_drained());
        assert!(queue.current().is_some());
        assert_eq!(shown(&mut queue, 2000), None);
    }

    #[test]
    fn drained_only_once_everything_delivered_has_been_shown() {
        let mut queue = PresentationQueue::new();
        assert!(queue.is_drained());

        submit(&mut queue, 1, 1000);
        // Still drained: the decoder owes us a picture, but there is nothing
        // waiting to go on screen, and waiting on a frame that may never
        // arrive would leave a stream unable to end.
        assert!(queue.is_drained());

        deliver(&mut queue, 1);
        assert!(!queue.is_drained());

        assert_eq!(shown(&mut queue, 1000), Some(1));
        assert!(queue.is_drained());
    }

    #[test]
    fn a_picture_that_was_never_submitted_is_ignored() {
        let mut queue = PresentationQueue::new();
        deliver(&mut queue, 7);
        assert!(queue.is_drained());
        assert_eq!(shown(&mut queue, i64::MAX), None);
    }
}
