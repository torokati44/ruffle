//! Picture-layer decoder

use crate::decoder::reader::H263Reader;
use crate::error::{Error, Result};
use crate::types::{
    CustomPictureClock, CustomPictureFormat, Picture, PictureOption, PictureTypeCode,
    PixelAspectRatio, SourceFormat,
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
        const MayHaveReferenceLayerNumber = 0b10000;
        const HasReferencePictureSelection = 0b100000;
        const MayHaveTemporalReference = 0b1000000;
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

/// Attempts to read a `PLUSPTYPE` record from the bitstream.
fn decode_plusptype<R>(reader: &mut H263Reader<R>) -> Result<PlusPType>
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
            }

            if opptype & 0x00080 != 0 {
                options |= PictureOption::ReferencePictureSelection;
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

        let picture_width_indication = ((cpfmt & 0x07FC00) >> 10) as u8;
        let picture_height_indication = (cpfmt & 0x0000FF) as u8;

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

/// Attempts to read a picture record from an H.263 bitstream.
///
/// If no valid picture record could be found at the current position in the
/// reader's bitstream, this function returns `None` and leaves the reader at
/// the same position.
fn decode_picture<R>(reader: &mut H263Reader<R>) -> Result<Option<Picture>>
where
    R: Read,
{
    reader.with_transaction_option(|reader| {
        reader.skip_to_alignment()?;

        let psc: u32 = reader.read_bits(22)?;
        if psc != 0x000020 {
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
                let (extra_options, maybe_format, picture_type, followers) =
                    decode_plusptype(reader)?;

                options |= extra_options;

                multiplex_bitstream = Some(decode_cpm_and_psbi(reader)?);

                (maybe_format, picture_type, followers)
            }
        };

        //TODO: H.263 5.1.4.4-6 indicate a number of semantic restrictions on
        //picture options, modes, and followers. We should be inspecting our
        //set of options and raising an error if they're incorrect at this
        //time.

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

        //TODO: Implement all of the other follower records implied by the
        //options or followers returned from parsing `PlusPType`.
        //Start from H.263 5.1.9

        Ok(None)
    })
}
