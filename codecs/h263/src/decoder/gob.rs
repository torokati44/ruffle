//! Group-of-blocks

use crate::decoder::reader::H263Reader;
use crate::decoder::types::DecoderOptions;
use crate::error::{Error, Result};
use std::io::Read;

/// Attempts to read a GOB record from an H.263 bitstream.
///
/// If no valid picture record could be found at the current position in the
/// reader's bitstream, this function returns `None` and leaves the reader at
/// the same position. Otherwise, it returns the GOB record data, up to the
/// start of the first macroblock in the stream.
///
/// The set of `DecoderOptions` allows configuring certain information about
/// the decoding process that cannot be determined by decoding the bitstream
/// itself.
///
/// TODO: GOB decoding is a stub (and likely will always be so)
fn decode_gob<R>(reader: &mut H263Reader<R>, _decoder_options: DecoderOptions) -> Result<Option<()>>
where
    R: Read,
{
    reader.with_transaction_union(|reader| {
        reader.skip_to_alignment()?;

        let psc: u32 = reader.read_bits(17)?;
        if psc != 0x00001 {
            return Ok(None);
        }

        Err(Error::UnimplementedDecoding)
    })
}
