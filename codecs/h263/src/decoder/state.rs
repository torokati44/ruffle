//! H.263 decoder core

use crate::decoder::cpu::{gather, idct_block, inverse_rle, mv_decode, predict_candidate, scatter};
use crate::decoder::picture::DecodedPicture;
use crate::decoder::types::DecoderOption;
use crate::error::{Error, Result};
use crate::parser::{decode_block, decode_gob, decode_macroblock, decode_picture, H263Reader};
use crate::types::{
    GroupOfBlocks, Macroblock, MotionVector, PictureOption, PictureTypeCode, MPPTYPE_OPTIONS,
    OPPTYPE_OPTIONS,
};
use std::cmp::{max, min};
use std::collections::HashMap;
use std::io::Read;

/// All state necessary to decode a successive series of H.263 pictures.
pub struct H263State {
    /// External decoder options enabled on this decoder.
    decoder_options: DecoderOption,

    /// The temporal reference of the last decoded picture.
    last_picture: u16,

    /// All currently in-force picture options as of the last decoded frame.
    running_options: PictureOption,

    /// All previously-encoded reference pictures.
    reference_picture: HashMap<u16, DecodedPicture>,
}

impl H263State {
    /// Construct a new `H263State`.
    pub fn new(decoder_options: DecoderOption) -> Self {
        Self {
            decoder_options,
            last_picture: 0xFFFF,
            running_options: PictureOption::empty(),
            reference_picture: HashMap::new(),
        }
    }

    /// Get the last picture decoded in the bitstream.
    ///
    /// If `None`, then no pictures have yet to be decoded.
    pub fn get_last_picture(&self) -> Option<&DecodedPicture> {
        if self.last_picture == 0xFFFF {
            None
        } else {
            self.reference_picture.get(&self.last_picture)
        }
    }

    /// Decode the next picture in the bitstream.
    ///
    /// This does not yield any picture data: it merely advances the state of
    /// the encoder, if possible, such that the next picture in the stream can
    /// be retrieved from it. Bits are retrieved from the `reader`, which must
    /// be pointing to an optionally-aligned picture start code.
    ///
    /// In the event that an error occurs, previously existing decoder state
    /// and underlying reader state will remain. You may inspect the error in
    /// order to determine how to proceed. When doing so, it will be likely
    /// that you will need to replace the reader or it's bitstream. The rules
    /// for how you can change out readers are as follows:
    ///
    /// 1. A reader is semantically related to a previously-used reader if a
    ///    successful read of the previous reader would have yielded the same
    ///    bits as a read on the current one.
    /// 2. A reader remains semantically related to a previously-used reader if
    ///    unsuccessful reads of the previous reader are successful in the new
    ///    reader and the resulting additional bitstream correctly forms a
    ///    syntactically and semantically valid bitstream for this decoder.
    ///
    /// In practice, this means that streaming additional bits into the reader
    /// is OK, but seeking the reader to a new position is not. In order to
    /// seek to a new position, you must discard all existing decoder state,
    /// then seek to the position of a valid I frame and begin decoding anew.
    pub fn decode_next_picture<R>(&mut self, reader: &mut H263Reader<R>) -> Result<()>
    where
        R: Read,
    {
        reader.with_transaction(|reader| {
            let next_picture = decode_picture(
                reader,
                self.decoder_options,
                self.get_last_picture().map(|p| p.as_header()),
            )?
            .ok_or(Error::InvalidBitstream)?;

            let next_running_options = if next_picture.has_plusptype && next_picture.has_opptype {
                next_picture.options
            } else if next_picture.has_plusptype {
                (next_picture.options & !*OPPTYPE_OPTIONS)
                    | (self.running_options & *OPPTYPE_OPTIONS)
            } else {
                (next_picture.options & !*OPPTYPE_OPTIONS & !*MPPTYPE_OPTIONS)
                    | (self.running_options & (*OPPTYPE_OPTIONS | *MPPTYPE_OPTIONS))
            };

            let format = if let Some(format) = next_picture.format {
                format
            } else if matches!(next_picture.picture_type, PictureTypeCode::IFrame) {
                return Err(Error::PictureFormatMissing);
            } else if let Some(ref_format) = self.get_last_picture().map(|rp| rp.format()) {
                ref_format
            } else {
                return Err(Error::PictureFormatMissing);
            };

            //TODO: Exactly what IS the reference picture? Is it just the last
            //one?
            let reference_picture = self.get_last_picture();

            let mut next_decoded_picture =
                DecodedPicture::new(next_picture, format).ok_or(Error::PictureFormatInvalid)?;
            let mut in_force_quantizer = next_decoded_picture.as_header().quantizer;
            let mut predictor_vectors = Vec::new(); // all previously decoded MVDs
            let mut encountered_macroblocks = 0;
            let mb_per_line = next_decoded_picture
                .format()
                .into_width_and_height()
                .unwrap()
                .0 as usize
                / 16;

            loop {
                match decode_macroblock(
                    reader,
                    &next_decoded_picture.as_header(),
                    next_running_options,
                ) {
                    Ok(Macroblock::Stuffing) => continue,
                    Ok(Macroblock::Uncoded) => {
                        if matches!(
                            next_decoded_picture.as_header().picture_type,
                            PictureTypeCode::IFrame
                        ) {
                            return Err(Error::UncodedIFrameBlocks);
                        }

                        //TODO: copy pixel data as if this was an INTER block
                        //with no new IDCT energy

                        predictor_vectors.push([MotionVector::zero(); 4]);
                        encountered_macroblocks += 1;
                    }
                    Ok(Macroblock::Coded {
                        mb_type,
                        coded_block_pattern,
                        coded_block_pattern_b: _coded_block_pattern_b,
                        d_quantizer,
                        motion_vector,
                        addl_motion_vectors,
                        motion_vectors_b: _motion_vectors_b,
                    }) => {
                        let quantizer = min(
                            max(
                                in_force_quantizer as i16 + d_quantizer.unwrap_or(0) as i16,
                                1,
                            ),
                            31,
                        );

                        let mut motion_vectors = [MotionVector::zero(); 4];

                        if mb_type.is_inter() {
                            motion_vectors[0] = mv_decode(
                                &next_decoded_picture,
                                next_running_options,
                                predict_candidate(
                                    &predictor_vectors[..],
                                    &motion_vectors,
                                    mb_per_line,
                                    0,
                                ),
                                motion_vector.unwrap_or_else(MotionVector::zero),
                            );

                            if let Some([mv2, mv3, mv4]) = addl_motion_vectors {
                                motion_vectors[1] = mv_decode(
                                    &next_decoded_picture,
                                    next_running_options,
                                    predict_candidate(
                                        &predictor_vectors[..],
                                        &motion_vectors,
                                        mb_per_line,
                                        1,
                                    ),
                                    mv2,
                                );
                                motion_vectors[2] = mv_decode(
                                    &next_decoded_picture,
                                    next_running_options,
                                    predict_candidate(
                                        &predictor_vectors[..],
                                        &motion_vectors,
                                        mb_per_line,
                                        2,
                                    ),
                                    mv3,
                                );
                                motion_vectors[3] = mv_decode(
                                    &next_decoded_picture,
                                    next_running_options,
                                    predict_candidate(
                                        &predictor_vectors[..],
                                        &motion_vectors,
                                        mb_per_line,
                                        3,
                                    ),
                                    mv4,
                                );
                            } else {
                                motion_vectors[1] = motion_vectors[0];
                                motion_vectors[2] = motion_vectors[0];
                                motion_vectors[3] = motion_vectors[0];
                            }
                        };

                        predictor_vectors.push(motion_vectors);

                        let pos = (
                            encountered_macroblocks % mb_per_line as u16,
                            encountered_macroblocks / mb_per_line as u16,
                        );
                        let mut macroblock =
                            gather(mb_type, reference_picture, pos, motion_vectors)?;
                        let mut levels = [0; 64];

                        let luma0 = decode_block(
                            reader,
                            self.decoder_options,
                            next_decoded_picture.as_header(),
                            next_running_options,
                            mb_type,
                            coded_block_pattern.codes_luma[0],
                        )?;
                        inverse_rle(&luma0, &mut levels, quantizer);
                        idct_block(&levels, macroblock.luma_mut(0));

                        let luma1 = decode_block(
                            reader,
                            self.decoder_options,
                            next_decoded_picture.as_header(),
                            next_running_options,
                            mb_type,
                            coded_block_pattern.codes_luma[1],
                        )?;
                        inverse_rle(&luma1, &mut levels, quantizer);
                        idct_block(&levels, macroblock.luma_mut(1));

                        let luma2 = decode_block(
                            reader,
                            self.decoder_options,
                            next_decoded_picture.as_header(),
                            next_running_options,
                            mb_type,
                            coded_block_pattern.codes_luma[2],
                        )?;
                        inverse_rle(&luma2, &mut levels, quantizer);
                        idct_block(&levels, macroblock.luma_mut(2));

                        let luma3 = decode_block(
                            reader,
                            self.decoder_options,
                            next_decoded_picture.as_header(),
                            next_running_options,
                            mb_type,
                            coded_block_pattern.codes_luma[3],
                        )?;
                        inverse_rle(&luma3, &mut levels, quantizer);
                        idct_block(&levels, macroblock.luma_mut(3));

                        let chroma_b = decode_block(
                            reader,
                            self.decoder_options,
                            next_decoded_picture.as_header(),
                            next_running_options,
                            mb_type,
                            coded_block_pattern.codes_chroma_b,
                        )?;
                        inverse_rle(&chroma_b, &mut levels, quantizer);
                        idct_block(&levels, macroblock.chroma_b_mut());

                        let chroma_r = decode_block(
                            reader,
                            self.decoder_options,
                            next_decoded_picture.as_header(),
                            next_running_options,
                            mb_type,
                            coded_block_pattern.codes_chroma_r,
                        )?;
                        inverse_rle(&chroma_r, &mut levels, quantizer);
                        idct_block(&levels, macroblock.chroma_r_mut());

                        scatter(&mut next_decoded_picture, macroblock, pos);
                    }
                    Err(Error::InvalidBitstream) => {
                        match decode_gob(reader, self.decoder_options)? {
                            None => break, //We're at the end of the picture now
                            Some(GroupOfBlocks {
                                group_number,
                                multiplex_bitstream,
                                frame_id,
                                quantizer,
                            }) => {
                                in_force_quantizer = quantizer;
                                predictor_vectors = Vec::new();
                            }
                        }
                    } //search for next GOB?
                    Err(e) => return Err(e),
                }
            }

            //At this point, all decoding should be complete, and we should
            //have a fresh picture to put into the reference pile. We treat YUV
            //encoded pictures as "decoded" since the referencing scheme used
            //in H.263 demands it. Ask a GPU for help.
            if matches!(
                next_decoded_picture.as_header().picture_type,
                PictureTypeCode::IFrame
            ) {
                //You cannot backwards predict across iframes
                self.reference_picture = HashMap::new();
            }

            self.last_picture = next_decoded_picture.as_header().temporal_reference;
            self.reference_picture
                .insert(self.last_picture, next_decoded_picture);

            Ok(())
        })
    }
}
