use ruffle_video::error::Error;
use ruffle_video::frame::{DecodedFrame, DecodedFrameOut, EncodedFrame, FrameDependency};

#[cfg(feature = "h263")]
pub mod h263;

#[cfg(feature = "vp6")]
pub mod vp6;

#[cfg(feature = "screenvideo")]
pub mod screen;

/// Trait for video decoders.
/// This should be implemented for each video codec.
///
/// Handing a frame to the decoder and getting a picture back are separate
/// steps, because for codecs with bidirectional prediction they genuinely are:
/// such a decoder has to hold a picture back until it has seen enough of the
/// frames that follow it in the bitstream to know that nothing earlier is still
/// to come.
///
/// Codecs without that property - which is all of them except H.264 - should
/// implement [`LowDelayDecoder`] and be wrapped in [`LowDelay`] rather than
/// implementing this trait by hand.
pub trait VideoDecoder {
    /// Configure the decoder.
    fn configure_decoder(&mut self, _configuration_data: &[u8]) -> Result<(), Error> {
        Ok(())
    }

    /// Preload a frame.
    ///
    /// No decoding is intended to happen at this point in time. Instead, the
    /// video data should be inspected to determine inter-frame dependencies
    /// between this and any previous frames in the stream.
    ///
    /// Frames should be preloaded in the order that they are received.
    ///
    /// Any dependencies listed here are inherent to the video bitstream. The
    /// containing video stream is also permitted to introduce additional
    /// interframe dependencies.
    fn preload_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<FrameDependency, Error>;

    /// Hand a frame of a given video stream to the decoder.
    ///
    /// Frames must be submitted in decode order, and must not violate the frame
    /// dependencies declared by the output of `preload_frame`.
    ///
    /// Producing no picture here is normal rather than an error; pictures are
    /// collected separately, with `poll_frames`.
    fn submit_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<(), Error>;

    /// Move every picture the decoder has finished into `out`.
    ///
    /// Each one is tagged with the `frame_id` of the frame it was decoded from,
    /// because the order pictures come out in need not be the order the frames
    /// went in.
    fn poll_frames(&mut self, out: &mut Vec<DecodedFrameOut>) -> Result<(), Error>;

    /// Release everything the decoder is still holding back for reordering,
    /// because no more frames are coming.
    ///
    /// The pictures still arrive through `poll_frames`; this only stops the
    /// decoder waiting for frames that will never be submitted.
    fn flush(&mut self) -> Result<(), Error> {
        Ok(())
    }

    /// Throw away all decoder state: reference pictures, and anything that was
    /// submitted but has not come back out yet.
    ///
    /// Used when the stream jumps elsewhere, so that nothing from before the
    /// jump can surface after it.
    fn reset(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

/// A decoder that returns exactly one picture per submitted frame, straight
/// away.
///
/// Wrap one in [`LowDelay`] to get a [`VideoDecoder`].
pub trait LowDelayDecoder {
    /// Configure the decoder.
    fn configure_decoder(&mut self, _configuration_data: &[u8]) -> Result<(), Error> {
        Ok(())
    }

    /// Preload a frame. See [`VideoDecoder::preload_frame`].
    fn preload_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<FrameDependency, Error>;

    /// Decode a frame, returning the picture it codes for.
    fn decode_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<DecodedFrame, Error>;
}

/// Adapts a [`LowDelayDecoder`] to the submit/poll interface of
/// [`VideoDecoder`], by holding each picture until it is polled for.
pub struct LowDelay<D> {
    inner: D,
    ready: Vec<DecodedFrameOut>,
}

impl<D> LowDelay<D> {
    pub fn new(inner: D) -> Self {
        Self {
            inner,
            ready: Vec::new(),
        }
    }
}

impl<D: LowDelayDecoder> VideoDecoder for LowDelay<D> {
    fn configure_decoder(&mut self, configuration_data: &[u8]) -> Result<(), Error> {
        self.inner.configure_decoder(configuration_data)
    }

    fn preload_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<FrameDependency, Error> {
        self.inner.preload_frame(encoded_frame)
    }

    fn submit_frame(&mut self, encoded_frame: EncodedFrame<'_>) -> Result<(), Error> {
        let frame_id = encoded_frame.frame_id;
        let frame = self.inner.decode_frame(encoded_frame)?;
        self.ready.push(DecodedFrameOut { frame_id, frame });
        Ok(())
    }

    fn poll_frames(&mut self, out: &mut Vec<DecodedFrameOut>) -> Result<(), Error> {
        out.append(&mut self.ready);
        Ok(())
    }

    fn reset(&mut self) -> Result<(), Error> {
        self.ready.clear();
        Ok(())
    }
}
