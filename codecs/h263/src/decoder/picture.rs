//! Picture-layer decoder

use crate::decoder::reader::H263Reader;
use crate::decoder::types::DecoderOptions;
use crate::error::{Error, Result};
use crate::types::{
    BPictureQuantizer, BackchannelMessage, CustomPictureClock, CustomPictureFormat,
    MotionVectorRange, Picture, PictureOption, PictureTypeCode, PixelAspectRatio,
    ReferencePictureResampling, ReferencePictureSelectionMode, ScalabilityLayer, SliceSubmode,
    SourceFormat,
};
use std::io::Read;

/// The information imparted by a `PTYPE` record.
///
/// If the optional portion of this type is `None`, that signals that a
/// `PLUSPTYPE` immediately follows the `PTYPE` record.
pub type PType = (PictureOption, Option<(SourceFormat, PictureTypeCode)>);

/// Decodes the first 8 bits of `PTYPE`.
fn decode_ptype<R>(reader: &mut H263Reader<R>) -> Result<PType>
where
    R: Read,
{
    reader.with_transaction(|reader| {
        let mut options = PictureOption::empty();

        let high_ptype_bits = reader.read_u8()?;
        if high_ptype_bits & 0xC0 != 0x80 {
            return Err(Error::InvalidBitstream);
        }

        if high_ptype_bits & 0x20 != 0 {
            options |= PictureOption::UseSplitScreen;
        }

        if high_ptype_bits & 0x10 != 0 {
            options |= PictureOption::UseDocumentCamera;
        }

        if high_ptype_bits & 0x08 != 0 {
            options |= PictureOption::ReleaseFullPictureFreeze;
        }

        let source_format = match high_ptype_bits & 0x07 {
            0 => return Err(Error::InvalidBitstream),
            1 => SourceFormat::SubQCIF,
            2 => SourceFormat::QuarterCIF,
            3 => SourceFormat::FullCIF,
            4 => SourceFormat::FourCIF,
            5 => SourceFormat::SixteenCIF,
            6 => SourceFormat::Reserved,
            _ => return Ok((options, None)),
        };

        let low_ptype_bits: u8 = reader.read_bits(5)?;
        let mut r#type = if low_ptype_bits & 0x10 != 0 {
            PictureTypeCode::IFrame
        } else {
            PictureTypeCode::PFrame
        };

        if low_ptype_bits & 0x08 != 0 {
            options |= PictureOption::UnrestrictedMotionVectors;
        }

        if low_ptype_bits & 0x04 != 0 {
            options |= PictureOption::SyntaxBasedArithmeticCoding;
        }

        if low_ptype_bits & 0x02 != 0 {
            options |= PictureOption::AdvancedPrediction;
        }

        if low_ptype_bits & 0x01 != 0 {
            r#type = PictureTypeCode::PBFrame;
        }

        Ok((options, Some((source_format, r#type))))
    })
}

bitflags! {
    /// Indicates which fields follow `PLUSPTYPE`.
    ///
    /// A field is only listed in here if the H.263 spec mentions the
    /// requirement that `UFEP` equal 001. Otherwise, the existence of a
    /// follower can be determined by the set of `PictureOption`s returned in
    /// the `PlusPType`.
    pub struct PlusPTypeFollower: u8 {
        const HasCustomFormat = 0b1;
        const HasCustomClock = 0b10;
        const HasMotionVectorRange = 0b100;
        const HasSliceStructuredSubmode = 0b1000;
        const HasReferenceLayerNumber = 0b10000;
        const HasReferencePictureSelectionMode = 0b100000;
    }
}

/// The information imparted by a `PLUSPTYPE` record.
///
/// `SourceFormat` is optional and will be `None` either if the record did not
/// specify a `SourceFormat` or if it specified a custom one. To determine if
/// one needs to be parsed, read the `PlusPTypeFollower`s, which indicate
/// additional records which follow this one in the bitstream.
pub type PlusPType = (
    PictureOption,
    Option<SourceFormat>,
    PictureTypeCode,
    PlusPTypeFollower,
);

/// The set of picture options defined by an `OPPTYPE` record.
///
/// If a picture does not contain an `OPPTYPE`, then all of these options will
/// be carried forward from the previous picture's options.
lazy_static! {
    static ref OPPTYPE_OPTIONS: PictureOption = PictureOption::UnrestrictedMotionVectors
        | PictureOption::SyntaxBasedArithmeticCoding
        | PictureOption::AdvancedPrediction
        | PictureOption::AdvancedIntraCoding
        | PictureOption::DeblockingFilter
        | PictureOption::SliceStructured
        | PictureOption::ReferencePictureSelection
        | PictureOption::IndependentSegmentDecoding
        | PictureOption::AlternativeInterVLC
        | PictureOption::ModifiedQuantization;
}

/// Attempts to read a `PLUSPTYPE` record from the bitstream.
///
/// The set of previous picture options are used to carry forward previously-
/// enabled options in the case where the `PLUSPTYPE` does not change them.
fn decode_plusptype<R>(
    reader: &mut H263Reader<R>,
    decoder_options: DecoderOptions,
    previous_picture_options: PictureOption,
) -> Result<PlusPType>
where
    R: Read,
{
    reader.with_transaction(|reader| {
        let ufep: u8 = reader.read_bits(3)?;
        let has_opptype = match ufep {
            0 => false,
            1 => true,
            _ => return Err(Error::InvalidBitstream),
        };

        let mut options = PictureOption::empty();
        let mut followers = PlusPTypeFollower::empty();
        let mut source_format = None;

        if has_opptype {
            let opptype: u32 = reader.read_bits(18)?;

            // OPPTYPE should end in bits 1000 as per H.263 5.1.4.2
            if (opptype & 0xF) != 0x8 {
                return Err(Error::InvalidBitstream);
            }

            source_format = match (opptype & 0x38000) >> 15 {
                0 => Some(SourceFormat::Reserved),
                1 => Some(SourceFormat::SubQCIF),
                2 => Some(SourceFormat::QuarterCIF),
                3 => Some(SourceFormat::FullCIF),
                4 => Some(SourceFormat::FourCIF),
                5 => Some(SourceFormat::SixteenCIF),
                6 => {
                    followers |= PlusPTypeFollower::HasCustomFormat;

                    None
                }
                _ => Some(SourceFormat::Reserved),
            };

            if opptype & 0x04000 != 0 {
                followers |= PlusPTypeFollower::HasCustomClock;
            }

            if opptype & 0x02000 != 0 {
                options |= PictureOption::UnrestrictedMotionVectors;
                followers |= PlusPTypeFollower::HasMotionVectorRange;
            }

            if opptype & 0x01000 != 0 {
                options |= PictureOption::SyntaxBasedArithmeticCoding;
            }

            if opptype & 0x00800 != 0 {
                options |= PictureOption::AdvancedPrediction;
            }

            if opptype & 0x00400 != 0 {
                options |= PictureOption::AdvancedIntraCoding;
            }

            if opptype & 0x00200 != 0 {
                options |= PictureOption::DeblockingFilter;
            }

            if opptype & 0x00100 != 0 {
                options |= PictureOption::SliceStructured;
                followers |= PlusPTypeFollower::HasSliceStructuredSubmode;
            }

            if opptype & 0x00080 != 0 {
                options |= PictureOption::ReferencePictureSelection;
                followers |= PlusPTypeFollower::HasReferencePictureSelectionMode;
            }

            if opptype & 0x00040 != 0 {
                options |= PictureOption::IndependentSegmentDecoding;
            }

            if opptype & 0x00020 != 0 {
                options |= PictureOption::AlternativeInterVLC;
            }

            if opptype & 0x00010 != 0 {
                options |= PictureOption::ModifiedQuantization;
            }

            if decoder_options.contains(DecoderOptions::UseScalabilityMode) {
                followers |= PlusPTypeFollower::HasReferenceLayerNumber;
            }
        } else {
            options |= previous_picture_options & *OPPTYPE_OPTIONS;
        }

        let mpptype: u16 = reader.read_bits(9)?;

        // MPPTYPE should end in bits 001 as per H.263 5.1.4.3
        if mpptype & 0x007 != 0x1 {
            return Err(Error::InvalidBitstream);
        }

        let picture_type = match (mpptype & 0x1C0) >> 6 {
            0 => PictureTypeCode::IFrame,
            1 => PictureTypeCode::PFrame,
            2 => PictureTypeCode::ImprovedPBFrame,
            3 => PictureTypeCode::BFrame,
            4 => PictureTypeCode::EIFrame,
            5 => PictureTypeCode::EPFrame,
            r => PictureTypeCode::Reserved(r as u8),
        };

        if mpptype & 0x020 != 0 {
            options |= PictureOption::ReferencePictureResampling;
        }

        if mpptype & 0x010 != 0 {
            options |= PictureOption::ReducedResolutionUpdate;
        }

        if mpptype & 0x008 != 0 {
            options |= PictureOption::RoundingTypeOne;
        }

        Ok((options, source_format, picture_type, followers))
    })
}

type SorensonPType = (SourceFormat, PictureTypeCode, PictureOption);

/// Attempts to read a Sorenson-equivalent PTYPE from the bitstream.
fn decode_sorenson_ptype<R>(reader: &mut H263Reader<R>) -> Result<SorensonPType>
where
    R: Read,
{
    reader.with_transaction(|reader| {
        let (mut source_format, bit_count) = match reader.read_bits(3)? {
            0 => (None, 8),
            1 => (None, 16),
            2 => (Some(SourceFormat::FullCIF), 0),
            3 => (Some(SourceFormat::QuarterCIF), 0),
            4 => (Some(SourceFormat::SubQCIF), 0),
            5 => (
                Some(SourceFormat::Extended(CustomPictureFormat {
                    pixel_aspect_ratio: PixelAspectRatio::Square,
                    picture_width_indication: 320,
                    picture_height_indication: 240,
                })),
                0,
            ),
            6 => (
                Some(SourceFormat::Extended(CustomPictureFormat {
                    pixel_aspect_ratio: PixelAspectRatio::Square,
                    picture_width_indication: 160,
                    picture_height_indication: 120,
                })),
                0,
            ),
            _ => (Some(SourceFormat::Reserved), 0),
        };

        if source_format.is_none() {
            let custom_width = reader.read_bits(bit_count)?;
            let custom_height = reader.read_bits(bit_count)?;

            source_format = Some(SourceFormat::Extended(CustomPictureFormat {
                pixel_aspect_ratio: PixelAspectRatio::Square,
                picture_width_indication: custom_width,
                picture_height_indication: custom_height,
            }));
        }

        let picture_type = match reader.read_bits(2)? {
            0 => PictureTypeCode::IFrame,
            1 => PictureTypeCode::PFrame,
            2 => PictureTypeCode::DisposablePFrame,
            r => PictureTypeCode::Reserved(r),
        };

        let mut options = PictureOption::empty();

        options |= PictureOption::UseDeblocker;

        Ok((source_format.unwrap(), picture_type, options))
    })
}

/// Attempts to read `CPM` and `PSBI` records from the bitstream.
///
/// The placement of this record changes based on whether or not a `PLUSPTYPE`
/// is present in the bitstream. If it is present, then this function should
/// be called immediately after parsing it. Otherwise, this function should be
/// called after parsing `PQUANT`.
fn decode_cpm_and_psbi<R>(reader: &mut H263Reader<R>) -> Result<Option<u8>>
where
    R: Read,
{
    reader.with_transaction(|reader| {
        if reader.read_bits::<u8>(1)? != 0 {
            Ok(Some(reader.read_bits::<u8>(2)?))
        } else {
            Ok(None)
        }
    })
}

/// Attempts to read `CPFMT` from the bitstream.
fn decode_cpfmt<R>(reader: &mut H263Reader<R>) -> Result<CustomPictureFormat>
where
    R: Read,
{
    reader.with_transaction(|reader| {
        let cpfmt: u32 = reader.read_bits(23)?;

        if cpfmt & 0x000200 == 0 {
            return Err(Error::InvalidBitstream);
        }

        let pixel_aspect_ratio = match (cpfmt & 0x780000) >> 19 {
            0 => return Err(Error::InvalidBitstream),
            1 => PixelAspectRatio::Square,
            2 => PixelAspectRatio::Par12_11,
            3 => PixelAspectRatio::Par10_11,
            4 => PixelAspectRatio::Par16_11,
            5 => PixelAspectRatio::Par40_33,
            15 => {
                let par_width = reader.read_u8()?;
                let par_height = reader.read_u8()?;

                if par_width == 0 || par_height == 0 {
                    return Err(Error::InvalidBitstream);
                }

                PixelAspectRatio::Extended {
                    par_width,
                    par_height,
                }
            }
            r => PixelAspectRatio::Reserved(r as u8),
        };

        let picture_width_indication = (((cpfmt & 0x07FC00) >> 10) as u16 + 1) * 4;
        let picture_height_indication = ((cpfmt & 0x0000FF) as u16) * 4;

        Ok(CustomPictureFormat {
            pixel_aspect_ratio,
            picture_width_indication,
            picture_height_indication,
        })
    })
}

/// Attempts to read `CPCFC` from the bitstream.
fn decode_cpcfc<R>(reader: &mut H263Reader<R>) -> Result<CustomPictureClock>
where
    R: Read,
{
    reader.with_transaction(|reader| {
        let cpcfc = reader.read_u8()?;

        Ok(CustomPictureClock {
            times_1001: cpcfc & 0x80 != 0,
            divisor: cpcfc & 0x7F,
        })
    })
}

/// Attempts to read `UUI` from the bitstream.
fn decode_uui<R>(reader: &mut H263Reader<R>) -> Result<MotionVectorRange>
where
    R: Read,
{
    reader.with_transaction(|reader| {
        let is_limited: u8 = reader.read_bits(1)?;
        if is_limited == 1 {
            return Ok(MotionVectorRange::Standard);
        }

        let is_unlimited: u8 = reader.read_bits(1)?;
        if is_unlimited == 1 {
            return Ok(MotionVectorRange::Unlimited);
        }

        Err(Error::InvalidBitstream)
    })
}

/// Attempts to read `SSS` from the bitstream.
fn decode_sss<R>(reader: &mut H263Reader<R>) -> Result<SliceSubmode>
where
    R: Read,
{
    reader.with_transaction(|reader| {
        let mut sss = SliceSubmode::empty();
        let sss_bits: u8 = reader.read_bits(2)?;

        if sss_bits & 0x01 != 0 {
            sss |= SliceSubmode::RectangularSlices;
        }

        if sss_bits & 0x02 != 0 {
            sss |= SliceSubmode::ArbitraryOrder;
        }

        Ok(sss)
    })
}

/// Attempts to read `ELNUM` and `RLNUM` from the bitstream.
fn decode_elnum_rlnum<R>(
    reader: &mut H263Reader<R>,
    followers: PlusPTypeFollower,
) -> Result<ScalabilityLayer>
where
    R: Read,
{
    reader.with_transaction(|reader| {
        let enhancement = reader.read_bits(4)?;
        let reference = if followers.contains(PlusPTypeFollower::HasReferenceLayerNumber) {
            Some(reader.read_bits(4)?)
        } else {
            None
        };

        Ok(ScalabilityLayer {
            enhancement,
            reference,
        })
    })
}

/// Attempts to read `RPSMF` from the bitstream.
fn decode_rpsmf<R>(reader: &mut H263Reader<R>) -> Result<ReferencePictureSelectionMode>
where
    R: Read,
{
    reader.with_transaction(|reader| {
        let mut rpsmf = ReferencePictureSelectionMode::empty();
        let rpsmf_bits: u8 = reader.read_bits(3)?;

        if rpsmf_bits & 0x4 == 0 {
            rpsmf |= ReferencePictureSelectionMode::Reserved;
        }

        if rpsmf_bits & 0x2 != 0 {
            rpsmf |= ReferencePictureSelectionMode::RequestNegativeAcknowledgement;
        }

        if rpsmf_bits & 0x1 != 0 {
            rpsmf |= ReferencePictureSelectionMode::RequestAcknowledgement;
        }

        Ok(rpsmf)
    })
}

/// Attempts to read `TRPI` and `TRP` from the bitstream.
fn decode_trpi<R>(reader: &mut H263Reader<R>) -> Result<Option<u16>>
where
    R: Read,
{
    reader.with_transaction(|reader| {
        let trpi: u8 = reader.read_bits(1)?;

        if trpi == 1 {
            let trp: u16 = reader.read_bits(10)?;

            Ok(Some(trp))
        } else {
            Ok(None)
        }
    })
}

/// Attempts to read `BCI` and `BCM` from the bitstream.
fn decode_bcm<R>(reader: &mut H263Reader<R>) -> Result<Option<BackchannelMessage>>
where
    R: Read,
{
    reader.with_transaction(|reader| {
        let bci: u8 = reader.read_bits(1)?;

        if bci == 1 {
            Err(Error::UnimplementedDecoding)
        } else {
            let not_bci: u8 = reader.read_bits(1)?;

            if not_bci == 1 {
                Ok(None)
            } else {
                // BCI must be `1` or `01`
                Err(Error::InvalidBitstream)
            }
        }
    })
}

/// Attempts to read `RPRP` from the bitstream.
fn decode_rprp<R>(reader: &mut H263Reader<R>) -> Result<Option<ReferencePictureResampling>>
where
    R: Read,
{
    reader.with_transaction(|reader| Err(Error::UnimplementedDecoding))
}

/// Attempts to read `TRB` from the bitstream.
fn decode_trb<R>(reader: &mut H263Reader<R>, has_custom_pclk: bool) -> Result<u8>
where
    R: Read,
{
    reader.with_transaction(|reader| {
        if has_custom_pclk {
            reader.read_bits::<u8>(5)
        } else {
            reader.read_bits::<u8>(3)
        }
    })
}

/// Attempts to read `DBQUANT` from the bitstream.
fn decode_dbquant<R>(reader: &mut H263Reader<R>) -> Result<BPictureQuantizer>
where
    R: Read,
{
    reader.with_transaction(|reader| match reader.read_bits::<u8>(2)? {
        0 => Ok(BPictureQuantizer::FiveFourths),
        1 => Ok(BPictureQuantizer::SixFourths),
        2 => Ok(BPictureQuantizer::SevenFourths),
        3 => Ok(BPictureQuantizer::EightFourths),
        _ => Err(Error::InternalDecoderError),
    })
}

/// Attempts to read the `PSUPP` block from the bitstream as another embedded
/// bitstream.
fn decode_pei<R>(reader: &mut H263Reader<R>) -> Result<Vec<u8>>
where
    R: Read,
{
    reader.with_transaction(|reader| {
        let mut data = Vec::new();

        loop {
            let has_pei: u8 = reader.read_bits(1)?;
            if has_pei == 1 {
                data.push(reader.read_u8()?);
            } else {
                break;
            }
        }

        Ok(data)
    })
}

/// Attempts to read a picture record from an H.263 bitstream.
///
/// If no valid picture record could be found at the current position in the
/// reader's bitstream, this function returns `None` and leaves the reader at
/// the same position. Otherwise, it returns the picture layer data, with the
/// reader at the start of the GOB/slice layer data.
///
/// The set of `DecoderOptions` allows configuring certain information about
/// the decoding process that cannot be determined by decoding the bitstream
/// itself.
///
/// `previous_picture_options` is the set of options that were enabled by the
/// last decoded picture. If this is the first decoded picture in the
/// bitstream, then this should be an empty set.
fn decode_picture<R>(
    reader: &mut H263Reader<R>,
    decoder_options: DecoderOptions,
    previous_picture: Option<&Picture>,
) -> Result<Option<Picture>>
where
    R: Read,
{
    reader.with_transaction_union(|reader| {
        // Sorenson Spark pictures abuse the final bits of the start code as a
        // version field.
        if decoder_options.contains(DecoderOptions::SorensonSparkBitstream) {
            if !reader.recognize_start_code(0x00001, 17)? {
                return Ok(None);
            } else {
                let version = Some(reader.read_bits(5)?);
                let temporal_reference = reader.read_u8()? as u16;
                let (source_format, picture_type, options) = decode_sorenson_ptype(reader)?;
                let quantizer: u8 = reader.read_bits(5)?;
                let extra = decode_pei(reader)?;

                return Ok(Some(Picture {
                    version,
                    temporal_reference,
                    format: Some(source_format),
                    options,
                    picture_type,
                    quantizer,
                    extra,

                    //Sorenson is always unlimited
                    motion_vector_range: Some(MotionVectorRange::Unlimited),

                    //Here's a bunch more modes Sorenson doesn't use.
                    slice_submode: None,
                    scalability_layer: None,
                    reference_picture_selection_mode: None,
                    prediction_reference: None,
                    backchannel_message: None,
                    reference_picture_resampling: None,
                    multiplex_bitstream: None,
                    pb_reference: None,
                    pb_quantizer: None,
                }));
            }
        } else if !reader.recognize_start_code(0x000020, 22)? {
            return Ok(None);
        }

        let low_tr = reader.read_u8()?;
        let (mut options, maybe_format_and_type) = decode_ptype(reader)?;
        let mut multiplex_bitstream = None;
        let (mut format, picture_type, followers) = match maybe_format_and_type {
            Some((format, picture_type)) => {
                (Some(format), picture_type, PlusPTypeFollower::empty())
            }
            None => {
                let (extra_options, maybe_format, picture_type, followers) = decode_plusptype(
                    reader,
                    decoder_options,
                    previous_picture
                        .map(|p| p.options)
                        .unwrap_or_else(PictureOption::empty),
                )?;

                options |= extra_options;

                multiplex_bitstream = Some(decode_cpm_and_psbi(reader)?);

                (maybe_format, picture_type, followers)
            }
        };

        //TODO: H.263 5.1.4.4-6 indicate a number of semantic restrictions on
        //picture options, modes, and followers. We should be inspecting our
        //set of options and raising an error if they're incorrect at this
        //time.

        //TODO: Some pictures don't restate their previous format, but the
        //contents of the picture rely on if the format has changed. We need
        //`decode_picture` to be able to look up previous picture headers
        //somehow.

        if followers.contains(PlusPTypeFollower::HasCustomFormat) {
            format = Some(SourceFormat::Extended(decode_cpfmt(reader)?));
        }

        let picture_clock = if followers.contains(PlusPTypeFollower::HasCustomClock) {
            Some(decode_cpcfc(reader)?)
        } else {
            None
        };

        let temporal_reference = if picture_clock.is_some() {
            let high_tr = reader.read_bits::<u16>(2)? << 8;

            high_tr | low_tr as u16
        } else {
            low_tr as u16
        };

        let motion_vector_range = if followers.contains(PlusPTypeFollower::HasMotionVectorRange) {
            Some(decode_uui(reader)?)
        } else {
            None
        };

        let slice_submode = if followers.contains(PlusPTypeFollower::HasSliceStructuredSubmode) {
            Some(decode_sss(reader)?)
        } else {
            None
        };

        let scalability_layer = if decoder_options.contains(DecoderOptions::UseScalabilityMode) {
            Some(decode_elnum_rlnum(reader, followers)?)
        } else {
            None
        };

        let reference_picture_selection_mode =
            if followers.contains(PlusPTypeFollower::HasReferencePictureSelectionMode) {
                Some(decode_rpsmf(reader)?)
            } else {
                None
            };

        let prediction_reference = if options.contains(PictureOption::ReferencePictureSelection) {
            decode_trpi(reader)?
        } else {
            None
        };

        let backchannel_message = if options.contains(PictureOption::ReferencePictureSelection) {
            decode_bcm(reader)?
        } else {
            None
        };

        //TODO: this should be checking against the reference picture to see if we need RPRP
        let reference_picture_resampling = if options
            .contains(PictureOption::ReferencePictureResampling)
            || previous_picture
                .map(|p| p.format != format)
                .unwrap_or(false)
        {
            decode_rprp(reader)?
        } else {
            None
        };

        let quantizer: u8 = reader.read_bits(5)?;

        if multiplex_bitstream.is_none() {
            multiplex_bitstream = Some(decode_cpm_and_psbi(reader)?);
        }
        let multiplex_bitstream = multiplex_bitstream.unwrap();

        //TODO: This needs to know the picture clock, which has the usual
        //reference picture thing I mentioned before in the last TODO
        let (pb_reference, pb_quantizer) = if matches!(
            picture_type,
            PictureTypeCode::PBFrame | PictureTypeCode::ImprovedPBFrame
        ) {
            (
                Some(decode_trb(reader, picture_clock.is_some())?),
                Some(decode_dbquant(reader)?),
            )
        } else {
            (None, None)
        };

        let extra = decode_pei(reader)?;

        Ok(Some(Picture {
            version: None,
            temporal_reference,
            format,
            options,
            picture_type,
            motion_vector_range,
            slice_submode,
            scalability_layer,
            reference_picture_selection_mode,
            prediction_reference,
            backchannel_message,
            reference_picture_resampling,
            quantizer,
            multiplex_bitstream,
            pb_reference,
            pb_quantizer,
            extra,
        }))
    })
}
