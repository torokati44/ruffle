use super::{Decoder, SeekableDecoder};
use bitstream_io::{BitReader, LittleEndian};
use rustfft::num_complex::Complex32;
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

const TABLE: [usize; 64] = [
    0, 63, 31, 47, 15, 55, 23, 39, 7, 59, 27, 43, 11, 51, 19, 35, 3, 61, 29, 45, 13, 53, 21, 37, 5, 57, 25, 41, 9, 49, 17, 33, 1, 62, 30, 46, 14, 54, 22, 38, 6, 58, 26, 42, 10, 50, 18, 34, 2, 60, 28, 44, 12, 52, 20, 36, 4, 56, 24, 40, 8, 48, 16, 32,
];

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

const NELLY_SIGNAL_TABLE: [f32; 64] = [
    0.1250000000,
    0.1249623969,
    0.1248494014,
    0.1246612966,
    0.1243980974,
    0.1240599006,
    0.1236471012,
    0.1231596991,
    0.1225982010,
    0.1219628006,
    0.1212539002,
    0.1204719990,
    0.1196174994,
    0.1186909974,
    0.1176929995,
    0.1166241020,
    0.1154849008,
    0.1142762005,
    0.1129987016,
    0.1116530001,
    0.1102401987,
    0.1087609008,
    0.1072160974,
    0.1056066975,
    0.1039336994,
    0.1021981016,
    0.1004009023,
    0.0985433012,
    0.0966262966,
    0.0946511030,
    0.0926188976,
    0.0905309021,
    0.0883883014,
    0.0861926004,
    0.0839449018,
    0.0816465989,
    0.0792991966,
    0.0769039020,
    0.0744623989,
    0.0719759986,
    0.0694463030,
    0.0668746978,
    0.0642627999,
    0.0616123006,
    0.0589246005,
    0.0562013984,
    0.0534444004,
    0.0506552011,
    0.0478353985,
    0.0449868999,
    0.0421111993,
    0.0392102003,
    0.0362856016,
    0.0333391018,
    0.0303725004,
    0.0273876991,
    0.0243862998,
    0.0213702004,
    0.0183412991,
    0.0153013002,
    0.0122520998,
    0.0091955997,
    0.0061335000,
    0.0030677000,
];

const NELLY_INIT_TABLE: [u16; 64] = [
    3134, 5342, 6870, 7792, 8569, 9185, 9744, 10191, 10631, 11061, 11434, 11770, 12116, 12513,
    12925, 13300, 13674, 14027, 14352, 14716, 15117, 15477, 15824, 16157, 16513, 16804, 17090,
    17401, 17679, 17948, 18238, 18520, 18764, 19078, 19381, 19640, 19921, 20205, 20500, 20813,
    21162, 21465, 21794, 22137, 22453, 22756, 23067, 23350, 23636, 23926, 24227, 24521, 24819,
    25107, 25414, 25730, 26120, 26497, 26895, 27344, 27877, 28463, 29426, 31355,
];

const NELLY_STATE_TABLE: [f32; 128] = [
    0.0061359000,
    0.0184067003,
    0.0306748003,
    0.0429382995,
    0.0551952012,
    0.0674438998,
    0.0796824023,
    0.0919089988,
    0.1041216031,
    0.1163185984,
    0.1284981072,
    0.1406581998,
    0.1527972072,
    0.1649131030,
    0.1770042032,
    0.1890687048,
    0.2011045963,
    0.2131102979,
    0.2250839025,
    0.2370236069,
    0.2489275932,
    0.2607941031,
    0.2726213932,
    0.2844074965,
    0.2961508930,
    0.3078495860,
    0.3195019960,
    0.3311063051,
    0.3426606953,
    0.3541634977,
    0.3656130135,
    0.3770073950,
    0.3883450031,
    0.3996241987,
    0.4108431935,
    0.4220002890,
    0.4330937862,
    0.4441221058,
    0.4550836086,
    0.4659765065,
    0.4767991900,
    0.4875501990,
    0.4982276857,
    0.5088300705,
    0.5193560123,
    0.5298035741,
    0.5401715040,
    0.5504580140,
    0.5606616139,
    0.5707806945,
    0.5808140039,
    0.5907596946,
    0.6006165147,
    0.6103827953,
    0.6200572252,
    0.6296381950,
    0.6391243935,
    0.6485143900,
    0.6578066945,
    0.6669998765,
    0.6760926843,
    0.6850836873,
    0.6939715147,
    0.7027546763,
    0.7114322186,
    0.7200024724,
    0.7284644246,
    0.7368165851,
    0.7450578213,
    0.7531868219,
    0.7612023950,
    0.7691032887,
    0.7768884897,
    0.7845566273,
    0.7921066284,
    0.7995373011,
    0.8068475723,
    0.8140363097,
    0.8211025000,
    0.8280450106,
    0.8348628879,
    0.8415549994,
    0.8481202722,
    0.8545579910,
    0.8608669043,
    0.8670461774,
    0.8730949759,
    0.8790122271,
    0.8847970963,
    0.8904486895,
    0.8959661722,
    0.9013488293,
    0.9065957069,
    0.9117059708,
    0.9166790843,
    0.9215139747,
    0.9262102246,
    0.9307669997,
    0.9351835251,
    0.9394592047,
    0.9435935020,
    0.9475855827,
    0.9514350295,
    0.9551411867,
    0.9587035179,
    0.9621214271,
    0.9653943777,
    0.9685220718,
    0.9715039134,
    0.9743394256,
    0.9770280719,
    0.9795697927,
    0.9819638729,
    0.9842100739,
    0.9863080978,
    0.9882575870,
    0.9900581837,
    0.9917098284,
    0.9932119250,
    0.9945645928,
    0.9957674146,
    0.9968202710,
    0.9977231026,
    0.9984756112,
    0.9990776777,
    0.9995294213,
    0.9998306036,
    0.9999812245,
];

const NELLY_DELTA_TABLE: [i16; 32] = [
    -11725, -9420, -7910, -6801, -5948, -5233, -4599, -4039, -3507, -3030, -2596, -2170, -1774,
    -1383, -1016, -660, -329, -1, 337, 696, 1085, 1512, 1962, 2433, 2968, 3569, 4314, 5279, 6622,
    8154, 10076, 12975,
];

const NELLY_POS_UNPACK_TABLE: [f32; 64] = [
    0.9999812245,
    0.9995294213,
    0.9984756112,
    0.9968202710,
    0.9945645928,
    0.9917098284,
    0.9882575870,
    0.9842100739,
    0.9795697927,
    0.9743394256,
    0.9685220718,
    0.9621214271,
    0.9551411867,
    0.9475855827,
    0.9394592047,
    0.9307669997,
    0.9215139747,
    0.9117059708,
    0.9013488293,
    0.8904486895,
    0.8790122271,
    0.8670461774,
    0.8545579910,
    0.8415549994,
    0.8280450106,
    0.8140363097,
    0.7995373011,
    0.7845566273,
    0.7691032887,
    0.7531868219,
    0.7368165851,
    0.7200024724,
    0.7027546763,
    0.6850836873,
    0.6669998765,
    0.6485143900,
    0.6296381950,
    0.6103827953,
    0.5907596946,
    0.5707806945,
    0.5504580140,
    0.5298035741,
    0.5088300705,
    0.4875501990,
    0.4659765065,
    0.4441221058,
    0.4220002890,
    0.3996241987,
    0.3770073950,
    0.3541634977,
    0.3311063051,
    0.3078495860,
    0.2844074965,
    0.2607941031,
    0.2370236069,
    0.2131102979,
    0.1890687048,
    0.1649131030,
    0.1406581998,
    0.1163185984,
    0.0919089988,
    0.0674438998,
    0.0429382995,
    0.0184067003,
];

const NELLY_NEG_UNPACK_TABLE: [f32; 64] = [
    -0.0061359000,
    -0.0306748003,
    -0.0551952012,
    -0.0796824023,
    -0.1041216031,
    -0.1284981072,
    -0.1527972072,
    -0.1770042032,
    -0.2011045963,
    -0.2250839025,
    -0.2489275932,
    -0.2726213932,
    -0.2961508930,
    -0.3195019960,
    -0.3426606953,
    -0.3656130135,
    -0.3883450031,
    -0.4108431935,
    -0.4330937862,
    -0.4550836086,
    -0.4767991900,
    -0.4982276857,
    -0.5193560123,
    -0.5401715040,
    -0.5606616139,
    -0.5808140039,
    -0.6006165147,
    -0.6200572252,
    -0.6391243935,
    -0.6578066945,
    -0.6760926843,
    -0.6939715147,
    -0.7114322186,
    -0.7284644246,
    -0.7450578213,
    -0.7612023950,
    -0.7768884897,
    -0.7921066284,
    -0.8068475723,
    -0.8211025000,
    -0.8348628879,
    -0.8481202722,
    -0.8608669043,
    -0.8730949759,
    -0.8847970963,
    -0.8959661722,
    -0.9065957069,
    -0.9166790843,
    -0.9262102246,
    -0.9351835251,
    -0.9435935020,
    -0.9514350295,
    -0.9587035179,
    -0.9653943777,
    -0.9715039134,
    -0.9770280719,
    -0.9819638729,
    -0.9863080978,
    -0.9900581837,
    -0.9932119250,
    -0.9957674146,
    -0.9977231026,
    -0.9990776777,
    -0.9998306036,
];

const NELLY_INV_DFT_TABLE: [f32; 129] = [
    0.0000000000,
    0.0122715384,
    0.0245412290,
    0.0368072242,
    0.0490676723,
    0.0613207370,
    0.0735645667,
    0.0857973099,
    0.0980171412,
    0.1102222130,
    0.1224106774,
    0.1345807165,
    0.1467304677,
    0.1588581353,
    0.1709618866,
    0.1830398887,
    0.1950903237,
    0.2071113735,
    0.2191012353,
    0.2310581058,
    0.2429801822,
    0.2548656464,
    0.2667127550,
    0.2785196900,
    0.2902846932,
    0.3020059466,
    0.3136817515,
    0.3253102899,
    0.3368898630,
    0.3484186828,
    0.3598950505,
    0.3713171780,
    0.3826834261,
    0.3939920366,
    0.4052413106,
    0.4164295495,
    0.4275550842,
    0.4386162460,
    0.4496113360,
    0.4605387151,
    0.4713967443,
    0.4821837842,
    0.4928981960,
    0.5035383701,
    0.5141027570,
    0.5245896578,
    0.5349976420,
    0.5453249812,
    0.5555702448,
    0.5657318234,
    0.5758081675,
    0.5857978463,
    0.5956993103,
    0.6055110693,
    0.6152315736,
    0.6248595119,
    0.6343932748,
    0.6438315511,
    0.6531728506,
    0.6624158025,
    0.6715589762,
    0.6806010008,
    0.6895405650,
    0.6983762383,
    0.7071067691,
    0.7157308459,
    0.7242470980,
    0.7326542735,
    0.7409511209,
    0.7491363883,
    0.7572088242,
    0.7651672959,
    0.7730104327,
    0.7807372212,
    0.7883464098,
    0.7958369255,
    0.8032075167,
    0.8104572296,
    0.8175848126,
    0.8245893121,
    0.8314695954,
    0.8382247090,
    0.8448535800,
    0.8513551950,
    0.8577286005,
    0.8639728427,
    0.8700869679,
    0.8760700822,
    0.8819212317,
    0.8876396418,
    0.8932242990,
    0.8986744881,
    0.9039893150,
    0.9091680050,
    0.9142097831,
    0.9191138744,
    0.9238795042,
    0.9285060763,
    0.9329928160,
    0.9373390079,
    0.9415440559,
    0.9456073046,
    0.9495281577,
    0.9533060193,
    0.9569403529,
    0.9604305029,
    0.9637760520,
    0.9669764638,
    0.9700312614,
    0.9729399681,
    0.9757021070,
    0.9783173800,
    0.9807852507,
    0.9831054807,
    0.9852776527,
    0.9873014092,
    0.9891765118,
    0.9909026623,
    0.9924795032,
    0.9939069748,
    0.9951847196,
    0.9963126183,
    0.9972904325,
    0.9981181026,
    0.9987954497,
    0.9993223548,
    0.9996988177,
    0.9999247193,
    1.0000000000,
];

const NELLY_CENTER_TABLE: [usize; 64] = [
    0, 32, 16, 48, 8, 40, 24, 56, 4, 36, 20, 52, 12, 44, 28, 60, 2, 34, 18, 50, 10, 42, 26, 58, 6,
    38, 22, 54, 14, 46, 30, 62, 1, 33, 17, 49, 9, 41, 25, 57, 5, 37, 21, 53, 13, 45, 29, 61, 3, 35,
    19, 51, 11, 43, 27, 59, 7, 39, 23, 55, 15, 47, 31, 63,
];

pub struct NellymoserDecoder<R: Read> {
    inner: R,
    sample_rate: u16,
    state: [f32; 64],
    samples: [f32; NELLY_SAMPLES],
    cur_sample: usize,
}

impl<R: Read> NellymoserDecoder<R> {
    pub fn new(inner: R, sample_rate: u16) -> Self {
        NellymoserDecoder {
            inner,
            sample_rate,
            state: [0.0; 64],
            samples: [0.0; NELLY_SAMPLES],
            cur_sample: 0,
        }
    }
}

#[inline]
fn signed_shift(i: i32, shift: i32) -> i32 {
    if shift > 0 {
        i << shift
    } else {
        i >> -shift
    }
}

fn sum_bits(buf: [i16; NELLY_BUF_LEN], shift: i16, off: i16) -> i32 {
    buf[0..NELLY_FILL_LEN].iter().fold(0i32, |ret, &i| {
        let b = i as i32 - off as i32;
        let b = ((b >> (shift - 1)) + 1) >> 1;
        ret + if b < 0 {
            0
        } else if b > NELLY_BIT_CAP as i32 {
            NELLY_BIT_CAP as i32
        } else {
            b
        }
    })
}

fn headroom(la: &mut i32) -> i32 {
    if *la == 0 {
        return 31;
    }

    let l = la.abs().leading_zeros() as i32 - 1;
    *la *= 1 << l;
    l
}

fn unpack_coeffs(buf: [f32; NELLY_BUF_LEN], audio: &mut Vec<Complex32>) {
    let end = NELLY_BUF_LEN / 2 - 1;
    let mid_hi = NELLY_BUF_LEN / 2;
    let mid_lo = mid_hi - 1;

    for i in 0..NELLY_BUF_LEN / 4 {
        let b = buf[i * 2];
        let c = buf[i * 2 + 1];
        let d = buf[end * 2 - i * 2 - 1];
        let a = buf[end * 2 - i * 2];
        let e = NELLY_POS_UNPACK_TABLE[i];
        let f = NELLY_NEG_UNPACK_TABLE[i];
        audio[i] = Complex32::new(b * e - a * f, a * e + b * f);

        let a = NELLY_NEG_UNPACK_TABLE[mid_lo - i];
        let b = NELLY_POS_UNPACK_TABLE[mid_lo - i];
        audio[end - i] = Complex32::new(b * d - a * c, b * c + a * d);
    }
}

fn center(audio: &mut Vec<Complex32>) {
    for i in 0..NELLY_BUF_LEN / 2 {
        let j = NELLY_CENTER_TABLE[i];
        if j > i {
            audio.swap(i, j);
        }
    }
}

fn inverse_dft(audio: &mut Vec<Complex32>) {
    let mut offset = 0;
    for _ in 0..NELLY_BUF_LEN / 4 {
        let a = audio[offset + 0];
        let b = audio[offset + 1];

        audio[offset + 0] = a + b;
        audio[offset + 1] = a - b;

        offset += 2;
    }

    offset = 0;
    for _ in 0..NELLY_BUF_LEN / 8 {
        let a = audio[offset + 0];
        let b = audio[offset + 2];

        audio[offset + 0] = a + b;
        audio[offset + 2] = a - b;

        offset += 1;

        let a = audio[offset + 0];
        let b = audio[offset + 2];

        audio[offset + 0] = Complex32::new(a.re + b.im, a.im - b.re);
        audio[offset + 2] = Complex32::new(a.re - b.im, a.im + b.re);

        offset += 3;
    }

    let mut i = 0;
    let mut advance = 4;
    while advance < NELLY_BUF_LEN / 2 {
        offset = 0;

        for _ in 0..NELLY_BUF_LEN / (advance * 4) {
            for _ in 0..advance / 2 {
                let a = NELLY_INV_DFT_TABLE[128 - i];
                let c = NELLY_INV_DFT_TABLE[i];
                let (b, d) = (audio[offset + advance].re, audio[offset + advance].im);
                let (e, f) = (audio[offset].re, audio[offset].im);

                audio[offset] = Complex32::new(e + (a * b + c * d), f - (b * c - a * d));
                audio[offset + advance] = Complex32::new(e - (a * b + c * d), f + (b * c - a * d));

                i += 256 / advance;
                offset += 1;
            }

            for _ in 0..advance / 2 {
                let a = NELLY_INV_DFT_TABLE[128 - i];
                let c = NELLY_INV_DFT_TABLE[i];
                let (b, d) = (audio[offset + advance].re, audio[offset + advance].im);
                let (e, f) = (audio[offset].re, audio[offset].im);

                audio[offset] = Complex32::new(e - (a * b - c * d), f - (a * d + b * c));
                audio[offset + advance] = Complex32::new(e + (a * b - c * d), f + (a * d + b * c));

                i -= 256 / advance;
                offset += 1;
            }

            offset += advance;
        }

        advance *= 2;
    }
}

fn complex_to_signal(audio: &Vec<Complex32>, output: &mut [f32]) {
    let end = NELLY_BUF_LEN - 1;
    let end2 = NELLY_BUF_LEN / 2 - 1;
    let mid_hi = NELLY_BUF_LEN / 2;
    let mid_lo = mid_hi - 1;

    let (b, a) = (audio[end2].re, audio[end2].im);
    let (e, c) = (audio[0].re, audio[0].im);
    let d = NELLY_SIGNAL_TABLE[0];
    let g = NELLY_SIGNAL_TABLE[1];
    let f = NELLY_SIGNAL_TABLE[mid_lo];

    output[0] = d * e;
    output[1] = b * g - a * f;
    output[end - 1] = a * g + b * f;
    output[end] = c * -d;

    let mut offset = end2 - 1;
    let mut sig = mid_hi - 1;
    for i in 1..NELLY_BUF_LEN / 4 {
        let (a, b) = (audio[i].re, audio[i].im);
        let c = NELLY_SIGNAL_TABLE[i];
        let d = NELLY_SIGNAL_TABLE[sig];
        let (e, f) = (audio[offset].re, audio[offset].im);
        let a2 = NELLY_SIGNAL_TABLE[i + 1];
        let b2 = NELLY_SIGNAL_TABLE[sig - 1];

        output[i * 2 + 0] = a * c + b * d;
        output[i * 2 + 1] = a2 * e - b2 * f;
        output[offset * 2 + 0] = b2 * e + a2 * f;
        output[offset * 2 + 1] = a * d - b * c;

        sig -= 1;
        offset -= 1;
    }
}

fn apply_state(state: &mut [f32; 64], audio: &mut [f32]) {
    for i in 0..NELLY_BUF_LEN / 4 {
        let top = NELLY_BUF_LEN - i - 1;
        let mid_up = NELLY_BUF_LEN / 2 + i;
        let mid_down = NELLY_BUF_LEN / 2 - i - 1;
        let s_bot = audio[i];
        let s_top = audio[top];

        audio[i] = audio[mid_up] * NELLY_STATE_TABLE[i] + state[i] * NELLY_STATE_TABLE[top];
        audio[top] = state[i] * NELLY_STATE_TABLE[i] - audio[mid_up] * NELLY_STATE_TABLE[top];
        state[i] = -audio[mid_down];

        audio[mid_down] =
            s_top * NELLY_STATE_TABLE[mid_down] + state[mid_down] * NELLY_STATE_TABLE[mid_up];
        audio[mid_up] =
            state[mid_down] * NELLY_STATE_TABLE[mid_down] - s_top * NELLY_STATE_TABLE[mid_up];
        state[mid_down] = -s_bot;
    }
}

fn decode_block(
    state: &mut [f32; 64],
    block: &[u8; NELLY_BLOCK_LEN],
    samples: &mut [f32; NELLY_SAMPLES],
) {
    let mut buf = [0f32; NELLY_BUF_LEN];
    let mut pows = [0f32; NELLY_BUF_LEN];
    {
        let mut reader = BitReader::endian(Cursor::new(&block), LittleEndian);
        let mut val = NELLY_INIT_TABLE[reader.read::<u8>(6).unwrap() as usize] as f32;
        let mut ptr: usize = 0;
        for i in 0..NELLY_BANDS {
            if i > 0 {
                val += NELLY_DELTA_TABLE[reader.read::<u8>(5).unwrap() as usize] as f32;
            }

            let pval = 2f32.powf(val / 2048.0);
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

        let mut sbuf = [0i16; NELLY_BUF_LEN];
        for i in 0..NELLY_FILL_LEN {
            sbuf[i] = signed_shift(buf[i] as i32, shift as i32) as i16;
            sbuf[i] = ((3 * sbuf[i] as i32) >> 2) as i16;
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

            if (big_bitsum - NELLY_DETAIL_BITS).abs() >= (small_bitsum - NELLY_DETAIL_BITS).abs() {
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

    for i in 0..2 {
        let mut reader = BitReader::endian(Cursor::new(&block), LittleEndian);
        reader
            .skip(NELLY_HEADER_BITS + i * NELLY_DETAIL_BITS as u32)
            .unwrap();

        let mut input = [0f32; NELLY_BUF_LEN];
        for j in 0..NELLY_FILL_LEN {
            input[j] = if bits[j] <= 0 {
                std::f32::consts::FRAC_1_SQRT_2
            } else {
                let v = reader.read::<u8>(bits[j] as u32).unwrap();
                NELLY_DEQUANTIZATION_TABLE[((1 << bits[j]) - 1 + v) as usize]
            } * pows[j];
        }

        use rustfft::num_traits::Zero;
        use rustfft::FFTplanner;

        let mut input_complex: Vec<Complex32> = vec![Zero::zero(); NELLY_BUF_LEN / 2];

        unpack_coeffs(input, &mut input_complex);
        center(&mut input_complex);

        let mut new_input: Vec<Complex32> = Vec::new();
        for i in 0..64 {
            new_input.push(input_complex[TABLE[i]]);
        }
        let mut output_complex: Vec<Complex32> = vec![Zero::zero(); NELLY_BUF_LEN / 2];
        let mut planner = FFTplanner::new(true);
        let fft = planner.plan_fft(NELLY_BUF_LEN / 2);
        fft.process(&mut new_input, &mut output_complex);
        // inverse_dft(&mut new_input);
        let slice = &mut samples[i as usize * NELLY_BUF_LEN..(i as usize + 1) * NELLY_BUF_LEN];
        complex_to_signal(&output_complex, slice);
        apply_state(state, slice);
    }
}

impl<R: Read> Iterator for NellymoserDecoder<R> {
    type Item = [i16; 2];

    fn next(&mut self) -> Option<Self::Item> {
        if self.cur_sample >= NELLY_SAMPLES {
            let mut block = [0u8; NELLY_BLOCK_LEN];
            self.inner.read_exact(&mut block).ok()?;
            decode_block(&mut self.state, &block, &mut self.samples);
            self.cur_sample = 0;
        }

        let sample = self.samples[self.cur_sample] as i16;
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
