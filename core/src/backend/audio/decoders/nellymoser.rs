use super::{Decoder, SeekableDecoder};
use bitstream_io::{BitReader, LittleEndian};
use std::io::{Cursor, Read};

const NELLY_BANDS: usize = 23;
const NELLY_BLOCK_LEN: usize = 64;
const NELLY_HEADER_BITS: u32 = 116;
const NELLY_DETAIL_BITS: i32 = 198;
const NELLY_BUF_LEN: usize = 128;
const NELLY_FILL_LEN: usize = 124;
const NELLY_BIT_CAP: i16 = 6;
const NELLY_BASE_OFF: i32 = 4228;
const NELLY_BASE_SHIFT: i16 = 19;
const NELLY_SAMPLES: usize = NELLY_BUF_LEN * 2;

const NELLY_DEQUANTIZATION_TABLE: [f32; 127] = [
    0.0000000000,
    -0.8472560048,
    0.7224709988,
    -1.5247479677,
    -0.4531480074,
    0.3753609955,
    1.4717899561,
    -1.9822579622,
    -1.1929379702,
    -0.5829370022,
    -0.0693780035,
    0.3909569979,
    0.9069200158,
    1.4862740040,
    2.2215409279,
    -2.3887870312,
    -1.8067539930,
    -1.4105420113,
    -1.0773609877,
    -0.7995010018,
    -0.5558109879,
    -0.3334020078,
    -0.1324490011,
    0.0568020009,
    0.2548770010,
    0.4773550034,
    0.7386850119,
    1.0443060398,
    1.3954459429,
    1.8098750114,
    2.3918759823,
    -2.3893830776,
    -1.9884680510,
    -1.7514040470,
    -1.5643119812,
    -1.3922129869,
    -1.2164649963,
    -1.0469499826,
    -0.8905100226,
    -0.7645580173,
    -0.6454579830,
    -0.5259280205,
    -0.4059549868,
    -0.3029719889,
    -0.2096900046,
    -0.1239869967,
    -0.0479229987,
    0.0257730000,
    0.1001340002,
    0.1737180054,
    0.2585540116,
    0.3522900045,
    0.4569880068,
    0.5767750144,
    0.7003160119,
    0.8425520062,
    1.0093879700,
    1.1821349859,
    1.3534560204,
    1.5320819616,
    1.7332619429,
    1.9722349644,
    2.3978140354,
    -2.5756309032,
    -2.0573320389,
    -1.8984919786,
    -1.7727810144,
    -1.6662600040,
    -1.5742180347,
    -1.4993319511,
    -1.4316639900,
    -1.3652280569,
    -1.3000990152,
    -1.2280930281,
    -1.1588579416,
    -1.0921250582,
    -1.0135740042,
    -0.9202849865,
    -0.8287050128,
    -0.7374889851,
    -0.6447759867,
    -0.5590940118,
    -0.4857139885,
    -0.4110319912,
    -0.3459700048,
    -0.2851159871,
    -0.2341620028,
    -0.1870580018,
    -0.1442500055,
    -0.1107169986,
    -0.0739680007,
    -0.0365610011,
    -0.0073290002,
    0.0203610007,
    0.0479039997,
    0.0751969963,
    0.0980999991,
    0.1220389977,
    0.1458999962,
    0.1694349945,
    0.1970459968,
    0.2252430022,
    0.2556869984,
    0.2870100141,
    0.3197099864,
    0.3525829911,
    0.3889069855,
    0.4334920049,
    0.4769459963,
    0.5204820037,
    0.5644530058,
    0.6122040153,
    0.6685929894,
    0.7341650128,
    0.8032159805,
    0.8784040213,
    0.9566209912,
    1.0397069454,
    1.1293770075,
    1.2211159468,
    1.3080279827,
    1.4024800062,
    1.5056819916,
    1.6227730513,
    1.7724959850,
    1.9430880547,
    2.2903931141,
];

const NELLY_BAND_SIZES_TABLE: [u8; NELLY_BANDS] = [
    2, 2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 4, 4, 5, 6, 6, 7, 8, 9, 10, 12, 14, 15,
];

const NELLY_INIT_TABLE: [u16; 64] = [
    3134, 5342, 6870, 7792, 8569, 9185, 9744, 10191, 10631, 11061, 11434, 11770, 12116, 12513,
    12925, 13300, 13674, 14027, 14352, 14716, 15117, 15477, 15824, 16157, 16513, 16804, 17090,
    17401, 17679, 17948, 18238, 18520, 18764, 19078, 19381, 19640, 19921, 20205, 20500, 20813,
    21162, 21465, 21794, 22137, 22453, 22756, 23067, 23350, 23636, 23926, 24227, 24521, 24819,
    25107, 25414, 25730, 26120, 26497, 26895, 27344, 27877, 28463, 29426, 31355,
];

const NELLY_DELTA_TABLE: [i16; 32] = [
    -11725, -9420, -7910, -6801, -5948, -5233, -4599, -4039, -3507, -3030, -2596, -2170, -1774,
    -1383, -1016, -660, -329, -1, 337, 696, 1085, 1512, 1962, 2433, 2968, 3569, 4314, 5279, 6622,
    8154, 10076, 12975,
];

pub struct NellymoserDecoder<R: Read> {
    inner: R,
    sample_rate: u16,
    samples: [f32; NELLY_SAMPLES],
    cur_sample: usize,
}

impl<R: Read> NellymoserDecoder<R> {
    pub fn new(inner: R, sample_rate: u16) -> Self {
        NellymoserDecoder {
            inner,
            sample_rate,
            samples: [0.0; NELLY_SAMPLES],
            cur_sample: 0,
        }
    }
}

#[inline]
fn signed_shift(i: i32, shift: i32) -> i32 {
    if shift > 0 { i << shift } else { i >> -shift }
}

fn sum_bits(buf: [i16; NELLY_FILL_LEN], shift: i16, off: i16) -> i32 {
    buf.iter().fold(0i32, |ret, &i| {
        let mut b = i - off;
        b = ((b >> (shift - 1)) + 1) >> 1;
        ret + if b < 0 {
            0
        } else if b > NELLY_BIT_CAP {
            NELLY_BIT_CAP
        } else {
            b
        } as i32
    })
}

fn headroom(la: &mut i32) -> i32 {
    if *la == 0 {
        return 31;
    }

    let l = la.abs().leading_zeros() as i32 - 1;
    *la *= 1 << l;
    return l;
}

impl<R: Read> Iterator for NellymoserDecoder<R> {
    type Item = [i16; 2];

    fn next(&mut self) -> Option<Self::Item> {
        if self.cur_sample >= NELLY_SAMPLES {
            let mut block = [0u8; NELLY_BLOCK_LEN];
            self.inner.read_exact(&mut block).ok()?;

            let mut buf = [0f32; NELLY_FILL_LEN];
            let mut pows = [0f32; NELLY_FILL_LEN];
            {
                let mut reader = BitReader::endian(Cursor::new(&block), LittleEndian);
                let mut val = NELLY_INIT_TABLE[reader.read::<u8>(6).unwrap() as usize] as f32;
                let mut ptr: usize = 0;
                for i in 0..NELLY_BANDS {
                    if i > 0 {
                        val += NELLY_DELTA_TABLE[reader.read::<u8>(5).unwrap() as usize] as f32;
                    }

                    let scale_bias: f32 = 1.0 / (32768.0 * 8.0);
                    let pval = -((val / 2048.0).exp2()) * scale_bias;
                    for _ in 0..NELLY_BAND_SIZES_TABLE[i] {
                        buf[ptr] = val;
                        pows[ptr] = pval;
                        ptr += 1;
                    }
                }
            }

            let bits = {
                let mut max = buf.iter().fold(0, |a, &b| a.max(b as i32));
                let mut shift = headroom(&mut max) as i16 - 16;

                let mut sbuf = [0i16; NELLY_FILL_LEN];
                for i in 0..NELLY_FILL_LEN {
                    sbuf[i] = signed_shift(buf[i] as i32, shift as i32) as i16;
                    sbuf[i] = (3 * sbuf[i]) >> 2;
                }
                let mut sum: i32 = sbuf.iter().map(|&s| s as i32).sum();

                shift += 11;
                let shift_saved = shift;
                sum -= NELLY_DETAIL_BITS << shift;
                shift += headroom(&mut sum) as i16;
                let mut small_off = (NELLY_BASE_OFF * (sum >> 16)) >> 15;
                shift = shift_saved - (NELLY_BASE_SHIFT + shift - 31);

                small_off = signed_shift(small_off, shift as i32);

                let mut bitsum = sum_bits(sbuf, shift_saved, small_off as i16);

                if bitsum != NELLY_DETAIL_BITS {
                    let mut off = bitsum - NELLY_DETAIL_BITS;
                    shift = 0;
                    while off.abs() <= 16383 {
                        off *= 2;
                        shift += 1;
                    }

                    off = (off * NELLY_BASE_OFF) >> 15;
                    shift = shift_saved - (NELLY_BASE_SHIFT + shift - 15);

                    off = signed_shift(off, shift as i32);

                    let mut last_off = small_off;
                    let mut last_bitsum = bitsum;
                    let mut last_j = 0;
                    for j in 1..20 {
                        last_off = small_off;
                        small_off += off;
                        last_bitsum = bitsum;
                        last_j = j;

                        bitsum = sum_bits(sbuf, shift_saved, small_off as i16);

                        if (bitsum - NELLY_DETAIL_BITS) * (last_bitsum - NELLY_DETAIL_BITS) <= 0 {
                            break;
                        }
                    }

                    let mut big_off;
                    let mut big_bitsum;
                    let mut small_bitsum;
                    if bitsum > NELLY_DETAIL_BITS {
                        big_off = small_off;
                        small_off = last_off;
                        big_bitsum = bitsum;
                        small_bitsum = last_bitsum;
                    } else {
                        big_off = last_off;
                        big_bitsum = last_bitsum;
                        small_bitsum = bitsum;
                    }

                    while bitsum != NELLY_DETAIL_BITS && last_j <= 19 {
                        off = (big_off + small_off) >> 1;
                        bitsum = sum_bits(sbuf, shift_saved, off as i16);
                        if bitsum > NELLY_DETAIL_BITS {
                            big_off = off;
                            big_bitsum = bitsum;
                        } else {
                            small_off = off;
                            small_bitsum = bitsum;
                        }
                        last_j += 1;
                    }

                    if (big_bitsum - NELLY_DETAIL_BITS).abs()
                        >= (small_bitsum - NELLY_DETAIL_BITS).abs()
                    {
                        bitsum = small_bitsum;
                    } else {
                        small_off = big_off;
                        bitsum = big_bitsum;
                    }
                }

                let mut bits = [0i32; NELLY_BUF_LEN];
                for i in 0..NELLY_FILL_LEN {
                    let mut tmp = sbuf[i] as i32 - small_off;
                    tmp = ((tmp >> (shift_saved - 1)) + 1) >> 1;
                    bits[i] = if tmp < 0 {
                        0
                    } else if tmp > NELLY_BIT_CAP as i32 {
                        NELLY_BIT_CAP as i32
                    } else {
                        tmp
                    };
                }

                if bitsum > NELLY_DETAIL_BITS {
                    let mut i = 0;
                    let mut tmp = 0;
                    while tmp < NELLY_DETAIL_BITS {
                        tmp += bits[i];
                        i += 1;
                    }

                    bits[i - 1] -= tmp - NELLY_DETAIL_BITS;

                    while i < NELLY_FILL_LEN {
                        bits[i] = 0;
                        i += 1;
                    }
                }

                bits
            };

            use rand::{Rng, rngs::SmallRng, SeedableRng};
            let mut rng = SmallRng::from_seed([0u8; 16]);
            for i in 0..2 {
                let mut reader = BitReader::endian(Cursor::new(&block), LittleEndian);
                reader
                    .skip(NELLY_HEADER_BITS + i * NELLY_DETAIL_BITS as u32)
                    .unwrap();

                let mut input = [0f32; NELLY_BUF_LEN];
                for j in 0..NELLY_FILL_LEN {
                    if bits[j] <= 0 {
                        input[j] =
                            std::f32::consts::FRAC_1_SQRT_2
                                * pows[j]
                                * if rng.gen_bool(0.5) { 1.0 } else { -1.0 };
                    } else {
                        let v = reader.read::<u8>(bits[j] as u32).unwrap();
                        input[j] = NELLY_DEQUANTIZATION_TABLE
                            [((1 << bits[j]) - 1 + v) as usize]
                            * pows[j];
                    }
                }

                // TODO
                use num_complex::Complex32;
                use rustfft::FFTplanner;
                let mut planner = FFTplanner::new(true);
                let fft = planner.plan_fft(NELLY_BUF_LEN);
                let mut input_complex: Vec<Complex32> = input.iter().map(|x| Complex32::new(*x, 0.0)).collect();
                let mut output_complex: Vec<Complex32> = input.iter().map(|_| Complex32::new(0.0, 0.0)).collect();
                fft.process(&mut input_complex, &mut output_complex);
                let mut index = i as usize * NELLY_BUF_LEN;
                for x in output_complex.iter() {
                    self.samples[index] = x.re;
                    index += 1;
                }
            }

            self.cur_sample = 0;
        }

        let sample = (self.samples[self.cur_sample] * 32767.0) as i16;
        self.cur_sample += 1;
        Some([sample, sample])
    }
}

impl<R: Read> Decoder for NellymoserDecoder<R> {
    #[inline]
    fn num_channels(&self) -> u8 {
        1
    }

    #[inline]
    fn sample_rate(&self) -> u16 {
        self.sample_rate
    }
}

impl<R: AsRef<[u8]>> SeekableDecoder for NellymoserDecoder<Cursor<R>> {
    #[inline]
    fn reset(&mut self) {
        self.inner.set_position(0);
    }
}
