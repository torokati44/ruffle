//! Parsed H.263 bitstream types

/// ITU-T Recommendation H.263 (01/2005) 5.1.2-5.1.4 `TR`, `PTYPE`, `PLUSPTYPE`
/// and 5.1.8 `ETR`.
///
/// The `Picture` configures the current displayed frame's various options,
/// such as it's resolution, the use of any optional H.263 features, and the
/// intra-prediction mode used.
pub struct Picture {
    /// The version code.
    ///
    /// Only Sorenson Spark bitstreams contain a version code; compliant H.263
    /// bitstreams are unversioned.
    pub version: Option<u8>,

    /// The temporal reference index of this picture.
    ///
    /// This value may either be 8 or 10 bits wide. This means that references
    /// will overflow after frame 255 or 1023.
    pub temporal_reference: u16,

    /// The source format of the image. Determines it's resolution and frame
    /// rate.
    ///
    /// If unspecified, then the source format matches the reference picture
    /// for this picture.
    pub format: Option<SourceFormat>,

    /// Options which are enabled (or were implicitly present) on this picture.
    pub options: PictureOption,

    /// The intra-prediction mode in use, if any.
    pub picture_type: PictureTypeCode,

    /// Exactly *how* unlimited our unlimited motion vectors are.
    ///
    /// Must be specified if and only if the `PictureOption` called
    /// `UnlimitedMotionVectors` is also enabled.
    pub motion_vector_range: Option<MotionVectorRange>,

    /// What slice-structured submodes are active.
    ///
    /// Must be specified if and only if the `PictureOption` called
    /// `SliceStructured` is also enabled.
    pub slice_submode: Option<SliceSubmode>,

    /// Which layer this picture is a member of.
    ///
    /// Only present if Temporal, SNR, and Spatial Scalability mode is enabled.
    pub scalability_layer: Option<ScalabilityLayer>,

    /// What backchannel signals is the encoder requesting from it's decoding
    /// partner.
    pub reference_picture_selection_mode: Option<ReferencePictureSelectionMode>,

    /// ITU-T Recommendation H.263 (01/2005) 5.1.14-5.1.15 `TRP`,`TRPI`
    ///
    /// Indicates the temporal reference of the picture to be used to
    /// reconstruct this picture. Must not be specified if this is an `IFrame`
    /// or `EIFrame`. For `BFrame`s, this field indicates the reference number
    /// of the forward-predicted reference frame. If not specified, intra
    /// prediction proceeds as if `ReferencePictureSelection` had not been
    /// enabled.
    pub prediction_reference: Option<u16>,

    /// ITU-T Recommendation H.263 (01/2005) 5.1.16 `BCI`
    ///
    /// This field stores any backchannel message requests sent by the encoder.
    /// This field may only be present if `ReferencePictureSelection` has been
    /// enabled.
    pub backchannel_message: Option<BackchannelMessage>,

    /// ITU-T Recommendation H.263 (01/2005) 5.1.18 `RPRP`
    ///
    /// Carries the parameters of the `ReferencePictureResampling` mode.
    pub reference_picture_resampling: Option<ReferencePictureResampling>,

    /// ITU-T Recommendation H.263 (01/2005) 5.1.19 `PQUANT`
    ///
    /// The quantizer factor to be used for this picture (unless otherwise
    /// overridden in a particular lower layer).
    pub quantizer: u8,

    /// ITU-T Recommendation H.263 (01/2005) 5.1.20-5.1.21 `CPM`, `PSBI`
    ///
    /// A number from 0 to 3 indicating which multipoint sub-bitstream this
    /// picture is a member of. If `None`, then the continuous presence
    /// multipoint feature is not enabled.
    pub multiplex_bitstream: Option<u8>,

    /// ITU-T Recommendation H.263 (01/2005) 5.1.22 `TRb`
    ///
    /// The number of non-transmitted frames to the B half of the current PB
    /// frame. This field should not be present if not using PB frames or their
    /// improved variety.
    pub pb_reference: Option<u8>,

    /// ITU-T Recommendation H.263 (01/2005) 5.1.23 `DBQUANT`
    ///
    /// The quantization factor used for the B block of a PB frame. This field
    /// should not be present if not using PB frames or their improved variety.
    pub pb_quantizer: Option<BPictureQuantizer>,

    /// ITU-T Recommendation H.263 (01/2005) 5.1.24 `PEI`
    ///
    /// Extra information bytes which may have been added to this picture.
    pub extra: Vec<u8>,
}

/// The default resolution options available in H.263.
///
/// The `CIF` refers to "Common Interchange Format", a video teleconferencing
/// resolution and framerate standard intended to be a halfway house between
/// analog PAL and NTSC video formats. It has the line rate of PAL, with the
/// frame rate of NTSC, and always encodes color as 4:2:0 YCbCr. It's digital
/// video resolution is 352x288 @ 30000/1001hz.
///
/// Most other `SourceFormat` variants are multiples of the CIF picture count.
/// Note that the multiples refer to total pixel count; i.e. a `FourCIF` format
/// image is twice the width and height of a `FullCIF` format image.
#[derive(PartialEq)]
pub enum SourceFormat {
    /// 128x96 @ 30000/1001hz
    SubQCIF,

    /// 176x144 @ 30000/1001hz
    QuarterCIF,

    /// 352x288 @ 30000/1001hz
    FullCIF,

    /// 704x576 @ 30000/1001hz
    FourCIF,

    /// 1408x1152 @ 30000/1001hz
    SixteenCIF,

    /// Reserved by H.264 spec. Does not appear to be in use.
    Reserved,

    /// A custom source format.
    Extended(CustomPictureFormat),
}

bitflags! {
    /// All H.263 options configured by `PTYPE` and `OPPTYPE`.
    ///
    /// Many of these options are specified in annexes to H.263 and are not
    /// required to be supported in all decoders. The meaning of each picture
    /// option should be referenced from ITU-T Recommendation H.263 (01/2005).
    ///
    /// Certain combinations of `PictureOption`s are mutually exclusive and
    /// using them together will result in errors in compliant decoders. Some
    /// `PictureTypeCode`s will also prohibit the use of certain
    /// `PictureOption`s.
    pub struct PictureOption : u16 {
        const UseSplitScreen = 0b1;
        const UseDocumentCamera = 0b10;
        const ReleaseFullPictureFreeze = 0b100;
        const UnrestrictedMotionVectors = 0b1000;
        const SyntaxBasedArithmeticCoding = 0b10000;
        const AdvancedPrediction = 0b100000;
        const AdvancedIntraCoding = 0b1000000;
        const DeblockingFilter = 0b10000000;
        const SliceStructured = 0b100000000;
        const ReferencePictureSelection = 0b1000000000;
        const IndependentSegmentDecoding = 0b10000000000;
        const AlternativeInterVLC = 0b100000000000;
        const ModifiedQuantization = 0b1000000000000;
        const ReferencePictureResampling = 0b10000000000000;
        const ReducedResolutionUpdate = 0b100000000000000;
        const RoundingTypeOne = 0b1000000000000000;
    }
}

/// All available picture types in H.263.
///
/// A picture type indicates what reference frames should be used, if any, to
/// decode the image.
///
/// Certain `PictureTypeCode`s will prohibit the use of particular
/// `PictureOption`s.
pub enum PictureTypeCode {
    /// A full picture update that can be independently decoded.
    IFrame,

    /// A partial picture update that references a previously decoded frame.
    PFrame,

    /// PB frames.
    PBFrame,

    /// "Improved" PB frames.
    ImprovedPBFrame,

    /// A partial picture update that references up to two decoded frames, any
    /// of which may be future frames.
    BFrame,

    /// EI frames
    EIFrame,

    /// EP frames
    EPFrame,

    /// A reserved picture type code.
    ///
    /// The provided `u8` is the `MPPTYPE` that was reserved, anchored to the
    /// lowest significant bit of the `u8`.
    Reserved(u8),
}

/// ITU-T Recommendation H.263 (01/2005) 5.1.5-5.1.6 `CPFMT`, `EPAR`
///
/// This defines a "custom" picture format, outside of the standard CIF options.
#[derive(PartialEq)]
pub struct CustomPictureFormat {
    /// The aspect ratio of a single pixel.
    pub pixel_aspect_ratio: PixelAspectRatio,

    /// The number of pixels per line, shifted right by 4.
    pub picture_width_indication: u8,

    /// The number of lines per image, shifted right by 4.
    pub picture_height_indication: u8,
}

/// The aspect ratio of dots on each line.
///
/// Pixel aspect ratio is a hangover from the world of analog video, where the
/// line rate was determined by CRT circuitry but you could divide up that line
/// by any regular clock you wanted. The number of pixels per line determined
/// the aspect ratio of the dots you generated on the fundamentally analog CRT
/// screen.
///
/// The pixel aspect ratio does not determine anything about the structure of
/// the video data. It only determines how it should be stretched to produce
/// the correct aspect ratio.
///
/// Most modern video formats should be `Square`. Legacy analog formats may be
/// stored in one of the `ParNN_NN` formats. A custom PAR may be indicated with
/// the `Extended` option.
#[derive(PartialEq)]
pub enum PixelAspectRatio {
    /// 1:1 pixel aspect ratio. Most common on modern displays.
    Square,

    /// 12:11 pixel aspect ratio. Noted as "CIF for 4:3 Picture" in H.263.
    Par12_11,

    /// 10:11 pixel aspect ratio. Noted as "525-type for 4:3 Picture" in H.263.
    Par10_11,

    /// 16:11 pixel aspect ratio. Noted as "CIF stretched for 16:9 Picture" in
    /// H.263.
    Par16_11,

    /// 40:33 pixel aspect ratio. Noted as "525-type stretched for 16:9
    /// Picture" in H.263.
    Par40_33,

    /// One of the reserved PAR options.
    ///
    /// The provided `u8` is the `PAR` code that was reserved, anchored to the
    /// lowest significant bit of the `u8`.
    Reserved(u8),

    /// An extended/custom pixel aspect ratio.
    ///
    /// It is forbidden to have a zero width or height pixel.
    Extended { par_width: u8, par_height: u8 },
}

/// ITU-T Recommendation H.263 (01/2005) 5.1.7 `CPCFC`
///
/// The conversion between these factors and frame rate is as follows: Take
/// 1,800,000hz, and divide it by the effective divisor to produce a frame
/// rate. The effective divisor is `divisor` times either 1000 or 1001,
/// depending on the `times_1001` flag.
pub struct CustomPictureClock {
    /// Whether or not the divisor is multiplied by 1000 or 1001.
    ///
    /// `true` indicates 1001, whilst `false` indicates 1000.
    pub times_1001: bool,

    /// The divisor, itself stored divided by a constant factor (see
    /// `times_1001`.)
    pub divisor: u8,
}

/// ITU-T Recommendation H.263 (01/2005) 5.1.9 `UUI`
///
/// Indicates the new motion vector range limitations when
/// `UnrestrictedMotionVectors` are enabled.
pub enum MotionVectorRange {
    Standard,
    Unlimited,
}

bitflags! {
    /// ITU-T Recommendation H.263 (01/2005) 5.1.9 `SSS`
    ///
    /// Indicates slice configuration when slice-structured mode is enabled.
    pub struct SliceSubmode : u8 {
        /// Slices must be rectantular rather than free-running.
        const RectangularSlices = 0b1;

        /// Slices may be sent in arbitrary order.
        const ArbitraryOrder = 0b10;
    }
}

/// ITU-T Recommendation H.263 (01/2005) 5.1.11-5.1.12 `ELNUM`, `RLNUM`
///
/// Only present if Temporal, SNR, and Spatial Scalability is enabled.
pub struct ScalabilityLayer {
    /// The 4-bit enhancement layer index.
    pub enhancement: u8,

    /// The 4-bit reference layer index.
    ///
    /// If `None`, then this picture does not specify the reference layer for
    /// this layer. You should refer to previous pictures that do declare a
    /// reference layer in order to obtain that value in this case.
    pub reference: Option<u8>,
}

bitflags! {
    /// ITU-T Recommendation H.263 (01/2005) 5.1.13 `RPSMF`
    ///
    /// Indicates what backchannel messages the encoder would like out of it's
    /// decoding partner.
    pub struct ReferencePictureSelectionMode : u8 {
        const Reserved = 0b1;
        const RequestNegativeAcknowledgement = 0b10;
        const RequestAcknowledgement = 0b100;
    }
}

/// ITU-T Recommendation H.263 (01/2005) N.4.2 `BCM`
///
/// Indicates backchannel information that a decoder of a (presumably live)
/// video stream is sending in response to an opposing video stream. It may be
/// presented to the encoder with a separate logical channel, or it may be
/// muxed into a video stream that the encoder is also expected to decode.
pub struct BackchannelMessage {
    /// What message type is being back-channeled.
    message_type: BackchannelMessageType,

    /// Whether or not the backchanneler has reliable reference numbers to the
    /// opposing video stream. This being set to `Unreliable` indicates that
    /// the references in this message may not be correct.
    reliable: BackchannelReliability,

    /// The temporal reference of the picture being backchanneled.
    temporal_reference: u16,

    /// The enhancement layer being backchanneled, or `None` if no layer was
    /// specified.
    enhancement_layer: Option<u8>,

    /// The sub-bitstream number being backchanneled.
    sub_bitstream: Option<u8>,

    /// The GOB number or macroblock address being backchanneled.
    gob_macroblock_address: Option<u16>,

    /// The temporal reference being requested for retransmission (if NACK).
    requested_temporal_reference: Option<u16>,
}

/// ITU-T Recommendation H.263 (01/2005) N.4.2.1 `BT`
///
/// Indicates the backchanneler's decoding status of the opposing video stream.
pub enum BackchannelMessageType {
    /// Positive acknowledgement of correct decoding of the opposing video
    /// stream.
    Acknowledge,

    /// Negative acknowledgement of erroneous or failed decoding of the
    /// opposing video stream.
    NegativeAcknowledge,

    /// Reserved message type.
    Reserved(u8),
}

/// ITU-T Recommendation H.263 (01/2005) N.4.2.2 `URF`
///
/// Whether or not the backchanneling decoder has reliable values for temporal
/// references, group-of-block numbers, or macroblock addresses.
pub enum BackchannelReliability {
    Reliable,
    Unreliable,
}

/// ITU-T Recommendation H.263 (01/2005) P.2 `RPRP`
///
/// The parameters necessary for reference-picture resampling.
pub struct ReferencePictureResampling {
    accuracy: WarpingDisplacementAccuracy,

    /// The eight warping parameters for reference picture resampling.
    ///
    /// Each parameter is encoded according to table `D.3` in H.263 (01/2005).
    /// This is a variable-length code whose decoded values max out at around
    /// 11 bits.
    warps: Option<[u16; 8]>,
}

/// ITU-T Recommendation H.263 (01/2005) P.2.1 `WDA`
pub enum WarpingDisplacementAccuracy {
    /// Warping parameters are quantized to half-pixel accuracy.
    HalfPixel,

    /// Warping parameters are quantized to sixteenth-pixel accuracy.
    SixteenthPixel,
}

/// ITU-T Recommendation H.263 (01/2005), 5.1.23 `DBQUANT`
pub enum BPictureQuantizer {
    FiveFourths,
    SixFourths,
    SevenFourths,
    EightFourths,
}
