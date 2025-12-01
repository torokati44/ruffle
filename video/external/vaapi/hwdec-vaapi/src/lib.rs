use std::collections::VecDeque;
use std::convert::TryInto;
use std::rc::Rc;

use nihav_core::codecs::*;
use nihav_core::io::byteio::*;
use nihav_core::io::bitreader::*;
use nihav_core::io::intcode::*;

use libva::*;

#[cfg(debug_assertions)]
macro_rules! validate {
    ($a:expr) => { if !$a { println!("check failed at {}:{}", file!(), line!()); return Err(DecoderError::InvalidData); } };
}
#[cfg(not(debug_assertions))]
macro_rules! validate {
    ($a:expr) => { if !$a { return Err(DecoderError::InvalidData); } };
}

mod pic_ref;
pub use pic_ref::*;
#[allow(clippy::manual_range_contains)]
#[allow(clippy::needless_range_loop)]
mod sets;
use sets::*;
#[allow(clippy::manual_range_contains)]
mod slice;
use slice::*;

trait ReadUE {
    fn read_ue(&mut self) -> DecoderResult<u32>;
    fn read_ue_lim(&mut self, max_val: u32) -> DecoderResult<u32> {
        let val = self.read_ue()?;
        validate!(val <= max_val);
        Ok(val)
    }
    fn read_se(&mut self) -> DecoderResult<i32> {
        let val = self.read_ue()?;
        if (val & 1) != 0 {
            Ok (((val >> 1) as i32) + 1)
        } else {
            Ok (-((val >> 1) as i32))
        }
    }
}

impl<'a> ReadUE for BitReader<'a> {
    fn read_ue(&mut self) -> DecoderResult<u32> {
        Ok(self.read_code(UintCodeType::GammaP)? - 1)
    }
}

fn get_long_term_id(is_idr: bool, slice_hdr: &SliceHeader) -> Option<usize> {
    if is_idr && !slice_hdr.long_term_reference {
        None
    } else {
        let marking = &slice_hdr.adaptive_ref_pic_marking;
        for (&op, &arg) in marking.memory_management_control_op.iter().zip(marking.operation_arg.iter()).take(marking.num_ops) {
            if op == 6 {
                return Some(arg as usize);
            }
        }
        None
    }
}

fn unescape_nal(src: &[u8], dst: &mut Vec<u8>) -> usize {
    let mut off = 0;
    let mut zrun = 0;
    dst.clear();
    dst.reserve(src.len());
    while off < src.len() {
        dst.push(src[off]);
        if src[off] != 0 {
            zrun = 0;
        } else {
            zrun += 1;
            if zrun == 2 && off + 1 < src.len() && src[off + 1] == 0x03 {
                zrun = 0;
                off += 1;
            }
            if zrun >= 3 && off + 1 < src.len() && src[off + 1] == 0x01 {
                off -= 3;
                dst.truncate(off);
                break;
            }
        }
        off += 1;
    }
    off
}

fn make_dummy_h264_pic() -> PictureH264 {
    PictureH264::new(VA_INVALID_ID, 0, H264PictureFlag::Invalid.into(), 0, 0)
}

trait MakePicH264 {
    fn make_pic(&self) -> PictureH264;
}

impl MakePicH264 for PictureInfo {
    fn make_pic(&self) -> PictureH264 {
        let mut flags = H264PictureFlags::default();
        let frame_idx = if let Some(id) = self.long_term {
                flags |= H264PictureFlag::LongTermReference;
                id as u32
            } else {
                if self.is_ref {
                    flags |= H264PictureFlag::ShortTermReference;
                }
                u32::from(self.id)
            };
        PictureH264::new(self.surface_id, frame_idx, flags, self.top_id as i32, self.bot_id as i32)
    }
}

fn map_ref_list(refs: &[Option<PictureInfo>]) -> [PictureH264; 32] {
    let mut ref_list = Vec::with_capacity(32);

    for rpic in refs.iter() {
        ref_list.push(rpic.as_ref().map_or_else(make_dummy_h264_pic, |pic| pic.make_pic()));
    }

    while ref_list.len() < 32 {
        ref_list.push(make_dummy_h264_pic());
    }
    if let Ok(ret) = ref_list.try_into() {
        ret
    } else {
        panic!("can't convert");
    }
}

fn profile_name(profile: VAProfile::Type) -> &'static str {
    match profile {
        VAProfile::VAProfileMPEG2Simple => "MPEG2 Simple",
        VAProfile::VAProfileMPEG2Main => "MPEG2 Main",
        VAProfile::VAProfileMPEG4Simple => "MPEG4 Simple",
        VAProfile::VAProfileMPEG4AdvancedSimple => "MPEG4 Advanced Simple",
        VAProfile::VAProfileMPEG4Main => "MPEG4 Main",
        VAProfile::VAProfileH264Baseline => "H264 Baseline",
        VAProfile::VAProfileH264Main => "H264 Main",
        VAProfile::VAProfileH264High => "H264 High",
        VAProfile::VAProfileVC1Simple => "VC1 Simple",
        VAProfile::VAProfileVC1Main => "VC1 Main",
        VAProfile::VAProfileVC1Advanced => "VC1 Advanced",
        VAProfile::VAProfileH263Baseline => "H263 Baseline",
        VAProfile::VAProfileJPEGBaseline => "JPEG Baseline",
        VAProfile::VAProfileH264ConstrainedBaseline => "H264 Constrained Baseline",
        VAProfile::VAProfileVP8Version0_3 => "VP8",
        VAProfile::VAProfileH264MultiviewHigh => "H.264 Multiview High",
        VAProfile::VAProfileH264StereoHigh => "H264 Stereo High",
        VAProfile::VAProfileHEVCMain => "H.EVC Main",
        VAProfile::VAProfileHEVCMain10 => "H.EVC Main10",
        VAProfile::VAProfileVP9Profile0 => "VP9 Profile 0",
        VAProfile::VAProfileVP9Profile1 => "VP9 Profile 1",
        VAProfile::VAProfileVP9Profile2 => "VP9 Profile 2",
        VAProfile::VAProfileVP9Profile3 => "VP9 Profile 3",
        VAProfile::VAProfileHEVCMain12 => "HEVC Main12",
        VAProfile::VAProfileHEVCMain422_10 => "HEVC Main10 4:2:2",
        VAProfile::VAProfileHEVCMain422_12 => "HEVC Main12 4:2:2",
        VAProfile::VAProfileHEVCMain444 => "HEVC Main 4:4:4",
        VAProfile::VAProfileHEVCMain444_10 => "HEVC Main10 4:4:4",
        VAProfile::VAProfileHEVCMain444_12 => "HEVC Main12 4:4:4",
        VAProfile::VAProfileHEVCSccMain => "HEVC SCC Main",
        VAProfile::VAProfileHEVCSccMain10 => "HEVC SCC Main10",
        VAProfile::VAProfileHEVCSccMain444 => "HEVC SCC Main 4:4:4",
        VAProfile::VAProfileAV1Profile0 => "AV1 Profile 0",
        VAProfile::VAProfileAV1Profile1 => "AV1 Profile 1",
        VAProfile::VAProfileHEVCSccMain444_10 => "HEVC SCC Main10 4:4:4",
        _ => "unknown",
    }
}

const NUM_REF_PICS: usize = 16;

struct WaitingFrame {
    ts:     u64,
    pic:    Picture<PictureEnd>,
    is_idr: bool,
    is_ref: bool,
    ftype:  FrameType,
}

struct Reorderer {
    last_ref_dts:   Option<u64>,
    ready_idx:      usize,
    frames:         VecDeque<WaitingFrame>,
}

impl Default for Reorderer {
    fn default() -> Self {
        Self {
            last_ref_dts:   None,
            ready_idx:      0,
            frames:         VecDeque::with_capacity(16),
        }
    }
}

impl Reorderer {
    fn add_frame(&mut self, new_frame: WaitingFrame) {
        if !new_frame.is_ref {
            if self.frames.is_empty() {
                self.frames.push_back(new_frame);
            } else {
                let new_dts = new_frame.ts;
                let mut idx = 0;
                for (i, frm) in self.frames.iter().enumerate() {
                    idx = i;
                    if frm.ts > new_dts {
                        break;
                    }
                }
                self.frames.insert(idx, new_frame);
            }
        } else {
            for (i, frm) in self.frames.iter().enumerate() {
                if Some(frm.ts) == self.last_ref_dts {
                    self.ready_idx = i + 1;
                }
            }
            self.last_ref_dts = Some(new_frame.ts);
            self.frames.push_back(new_frame);
        }
    }
    fn get_frame(&mut self) -> Option<WaitingFrame> {
        if self.ready_idx > 0 {
            match self.frames[0].pic.query_status() {
                _ if self.ready_idx > 16 => {},
                Ok(VASurfaceStatus::Ready) => {},
                Ok(VASurfaceStatus::Rendering) => return None,
                _ => {
                    unimplemented!();
                },
            };
            self.ready_idx -= 1;
            self.frames.pop_front()
        } else {
            None
        }
    }
    fn flush(&mut self) {
        self.last_ref_dts = None;
        self.ready_idx = 0;
    }
}

#[allow(dead_code)]
struct VaapiInternals {
    display:        Rc<Display>,
    context:        Rc<Context>,
    ref_pics:       Vec<(Picture<PictureSync>, VASurfaceID)>,
    surfaces:       Vec<Surface>,
    ifmt:           VAImageFormat,
}

pub struct VaapiH264Decoder {
    info:           NACodecInfoRef,
    vaapi:          Option<VaapiInternals>,
    needs_derive:   bool,
    spses:          Vec<SeqParameterSet>,
    ppses:          Vec<PicParameterSet>,
    frame_refs:     FrameRefs,
    nal_len:        u8,
    out_frm:        NABufferType,
    reorderer:      Reorderer,
    tb_num:         u32,
    tb_den:         u32,
}

fn copy_luma_default(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    for (dline, sline) in dst.chunks_mut(dstride)
            .zip(src.chunks(sstride))
            .take(h) {
        dline[..w].copy_from_slice(&sline[..w]);
    }
}
#[cfg(not(target_arch="x86_64"))]
fn copy_luma(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    copy_luma_default(dst, dstride, src, sstride, w, h);
}
#[cfg(not(target_arch="x86_64"))]
fn deint_chroma(frm: NASimpleVideoFrame<u8>, src: &[u8], sstride: usize) {
    let mut uoff = frm.offset[1];
    let mut voff = frm.offset[2];
    for cline in src.chunks(sstride).take(frm.height[1]) {
        for (x, pair) in cline.chunks_exact(2).take(frm.width[1]).enumerate() {
            frm.data[uoff + x] = pair[0];
            frm.data[voff + x] = pair[1];
        }
        uoff += frm.stride[1];
        voff += frm.stride[2];
    }
}

#[cfg(target_arch="x86_64")]
use std::arch::asm;
#[cfg(target_arch="x86_64")]
fn copy_luma(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    if !is_x86_feature_detected!("avx") {
        copy_luma_default(dst, dstride, src, sstride, w, h);
        return;
    }
    if dst.as_ptr().align_offset(32) == 0 && src.as_ptr().align_offset(32) == 0 &&
            (w % 64) == 0 && ((dstride | sstride) % 32) == 0 {
        unsafe {
            asm!(
                "2:",
                "  mov {x}, {w}",
                "  3:",
                "    vmovdqa ymm0, [{src}]",
                "    vmovdqa ymm1, [{src}+32]",
                "    vmovdqa [{dst}], ymm0",
                "    vmovdqa [{dst}+32], ymm1",
                "    add {src}, 64",
                "    add {dst}, 64",
                "    sub {x},   64",
                "    jnz 3b",
                "  add {src}, {sstep}",
                "  add {dst}, {dstep}",
                "  dec {h}",
                "  jnz 2b",
                dst = inout(reg) dst.as_mut_ptr() => _,
                src = inout(reg) src.as_ptr() => _,
                sstep = in(reg) sstride - w,
                dstep = in(reg) dstride - w,
                w = in(reg) w,
                h = inout(reg) h => _,
                x = out(reg) _,
                out("ymm0") _,
                out("ymm1") _,
            );
        }
    } else if dst.as_ptr().align_offset(16) == 0 && src.as_ptr().align_offset(16) == 0 &&
            (w % 64) == 0 && ((dstride | sstride) % 16) == 0 {
        unsafe {
            asm!(
                "2:",
                "  mov {x}, {w}",
                "  3:",
                "    movdqa xmm0, [{src}]",
                "    movdqa xmm1, [{src}+16]",
                "    movdqa xmm2, [{src}+32]",
                "    movdqa xmm3, [{src}+48]",
                "    movdqa [{dst}], xmm0",
                "    movdqa [{dst}+16], xmm1",
                "    movdqa [{dst}+32], xmm2",
                "    movdqa [{dst}+48], xmm3",
                "    add {src}, 64",
                "    add {dst}, 64",
                "    sub {x},   64",
                "    jnz 3b",
                "  add {src}, {sstep}",
                "  add {dst}, {dstep}",
                "  dec {h}",
                "  jnz 2b",
                dst = inout(reg) dst.as_mut_ptr() => _,
                src = inout(reg) src.as_ptr() => _,
                sstep = in(reg) sstride - w,
                dstep = in(reg) dstride - w,
                w = in(reg) w,
                h = inout(reg) h => _,
                x = out(reg) _,
                out("xmm0") _,
                out("xmm1") _,
                out("xmm2") _,
                out("xmm3") _,
            );
        }
    } else {
        let copy_len = dstride.min(w);
        for (dline, sline) in dst.chunks_mut(dstride)
                .zip(src.chunks(sstride))
                .take(h) {
            dline[..copy_len].copy_from_slice(&sline[..copy_len]);
        }
    }
}
#[cfg(target_arch="x86_64")]
fn deint_chroma(frm: NASimpleVideoFrame<u8>, src: &[u8], sstride: usize) {
    unsafe {
        let width = (frm.width[1] + 7) & !7;
        let height = (frm.height[1] + 7) & !7;
        let dst = frm.data.as_mut_ptr();
        let udst = dst.add(frm.offset[1]);
        let vdst = dst.add(frm.offset[2]);
        let dstep = frm.stride[1] - width;
        let sstep = sstride - width * 2;
        asm!(
            "2:",
            "  mov {tmp}, {width}",
            "  test {width}, 8",
            "  jz 3f",
            "  movaps   xmm0,     [{src}]",
            "  movaps   xmm1,     xmm0",
            "  psllw    xmm0,     8",
            "  psrlw    xmm1,     8",
            "  psrlw    xmm0,     8",
            "  packuswb xmm1,     xmm1",
            "  packuswb xmm0,     xmm0",
            "  movq     [{vdst}], xmm1",
            "  movq     [{udst}], xmm0",
            "  add      {src},    16",
            "  add      {vdst},   8",
            "  add      {udst},   8",
            "  sub      {tmp},    8",
            "  3:",
            "    movaps   xmm0,     [{src}]",
            "    movaps   xmm1,     [{src} + 16]",
            "    movaps   xmm2,     xmm0",
            "    movaps   xmm3,     xmm1",
            "    psllw    xmm0,     8",
            "    psllw    xmm1,     8",
            "    psrlw    xmm2,     8",
            "    psrlw    xmm3,     8",
            "    psrlw    xmm0,     8",
            "    psrlw    xmm1,     8",
            "    packuswb xmm2,     xmm3",
            "    packuswb xmm0,     xmm1",
            "    movups   [{vdst}], xmm2",
            "    movups   [{udst}], xmm0",
            "    add      {src},    32",
            "    add      {vdst},   16",
            "    add      {udst},   16",
            "    sub      {tmp},    16",
            "    jnz      3b",
            "  add {udst}, {dstep}",
            "  add {vdst}, {dstep}",
            "  add {src},  {sstep}",
            "  dec {height}",
            "  jnz 2b",
            src = inout(reg) src.as_ptr() => _,
            udst = inout(reg) udst => _,
            vdst = inout(reg) vdst => _,
            width = in(reg) width,
            height = inout(reg) height => _,
            dstep = in(reg) dstep,
            sstep = in(reg) sstep,
            tmp = out(reg) _,
            out("xmm0") _,
            out("xmm1") _,
            out("xmm2") _,
            out("xmm3") _,
        );
    }
}

fn fill_frame(ifmt: VAImageFormat, pic: &Picture<PictureSync>, frm: &mut NABufferType, needs_derive: bool) -> DecoderResult<()> {
    let mut vbuf = frm.get_vbuf().unwrap();
    let (w, h) = pic.surface_size();
    //let cur_ts = pic.timestamp();

    let img = Image::new(pic, ifmt, w, h, !needs_derive).expect("get image");

    let iimg = img.image();
    let imgdata: &[u8] = img.as_ref();

    match iimg.format.fourcc().map_err(|_| DecoderError::InvalidData)? {
        VAFourcc::NV12 => {
            let frm = NASimpleVideoFrame::from_video_buf(&mut vbuf).unwrap();
            validate!(iimg.width  == (((frm.width[0]  + 15) & !15) as u16));
            validate!(iimg.height == (((frm.height[0] + 15) & !15) as u16));

            copy_luma(&mut frm.data[frm.offset[0]..], frm.stride[0], &imgdata[iimg.offsets[0] as usize..], iimg.pitches[0] as usize, (frm.width[0] + 15) & !15, frm.height[0]);

            deint_chroma(frm, &imgdata[iimg.offsets[1] as usize..], iimg.pitches[1] as usize);
        },
        VAFourcc::YV12 => {
            let frm = NASimpleVideoFrame::from_video_buf(&mut vbuf).unwrap();
            validate!(iimg.width  == (((frm.width[0]  + 15) & !15) as u16));
            validate!(iimg.height == (((frm.height[0] + 15) & !15) as u16));

            copy_luma(&mut frm.data[frm.offset[0]..], frm.stride[0], &imgdata[iimg.offsets[0] as usize..], iimg.pitches[0] as usize, (frm.width[0] + 15) & !15, frm.height[0]);
            copy_luma(&mut frm.data[frm.offset[2]..], frm.stride[2], &imgdata[iimg.offsets[1] as usize..], iimg.pitches[1] as usize, (frm.width[1] + 15) & !15, frm.height[1]);
            copy_luma(&mut frm.data[frm.offset[1]..], frm.stride[1], &imgdata[iimg.offsets[2] as usize..], iimg.pitches[2] as usize, (frm.width[2] + 15) & !15, frm.height[2]);
        },
        _ => unimplemented!(),
    };
    Ok(())
}

impl Default for VaapiH264Decoder {
    fn default() -> Self {
        Self {
            info:           NACodecInfoRef::default(),
            vaapi:          None,
            needs_derive:   false,
            spses:          Vec::with_capacity(1),
            ppses:          Vec::with_capacity(4),
            frame_refs:     FrameRefs::new(),
            nal_len:        0,
            out_frm:        NABufferType::None,
            reorderer:      Reorderer::default(),
            tb_num:         0,
            tb_den:         0,
        }
    }
}

impl VaapiH264Decoder {
    pub fn new() -> Self { Self::default() }
    pub fn init(&mut self, info: NACodecInfoRef) -> DecoderResult<()> {
        if let NACodecTypeInfo::Video(vinfo) = info.get_properties() {
            let edata = info.get_extradata().unwrap();
//print!("edata:"); for &el in edata.iter() { print!(" {:02X}", el); } println!();
            let profile;
            let mut nal_buf = Vec::with_capacity(1024);
            if edata.len() > 11 && &edata[0..4] == b"avcC" {
                let mut br = MemoryReader::new_read(edata.as_slice());

                                          br.read_skip(4)?;
                let version             = br.read_byte()?;
                validate!(version == 1);
                profile                 = br.read_byte()?;
                let _compatibility      = br.read_byte()?;
                let _level              = br.read_byte()?;
                let b                   = br.read_byte()?;
                //validate!((b & 0xFC) == 0xFC);
                self.nal_len            = (b & 3) + 1;
                let b                   = br.read_byte()?;
                //validate!((b & 0xE0) == 0xE0);
                let num_sps = (b & 0x1F) as usize;
                for _ in 0..num_sps {
                    let len             = br.read_u16be()? as usize;
                    let offset = br.tell() as usize;
                    validate!((br.peek_byte()? & 0x1F) == 7);
                    let _size = unescape_nal(&edata[offset..][..len], &mut nal_buf);
                                          br.read_skip(len)?;
                    let sps = parse_sps(&nal_buf[1..])?;
                    self.spses.push(sps);
                }
                let num_pps             = br.read_byte()? as usize;
                for _ in 0..num_pps {
                    let len             = br.read_u16be()? as usize;
                    let offset = br.tell() as usize;
                    validate!((br.peek_byte()? & 0x1F) == 8);
                    let _size = unescape_nal(&edata[offset..][..len], &mut nal_buf);
                                          br.read_skip(len)?;
                    let src = &nal_buf;

                    let mut full_size = src.len() * 8;
                    for &byte in src.iter().rev() {
                        if byte == 0 {
                            full_size -= 8;
                        } else {
                            full_size -= (byte.trailing_zeros() + 1) as usize;
                            break;
                        }
                    }
                    validate!(full_size > 0);

                    let pps = parse_pps(&src[1..], &self.spses, full_size - 8)?;
                    let mut found = false;
                    for stored_pps in self.ppses.iter_mut() {
                        if stored_pps.pic_parameter_set_id == pps.pic_parameter_set_id {
                            *stored_pps = pps.clone();
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        self.ppses.push(pps);
                    }
                }
                if br.left() > 0 {
                    match profile {
                        100 | 110 | 122 | 144 => {
                            let b       = br.read_byte()?;
                            // some encoders put something different here
                            if (b & 0xFC) != 0xFC {
                                return Ok(());
                            }
                            // b & 3 -> chroma format
                            let b       = br.read_byte()?;
                            validate!((b & 0xF8) == 0xF8);
                            // b & 7 -> luma depth minus 8
                            let b       = br.read_byte()?;
                            validate!((b & 0xF8) == 0xF8);
                            // b & 7 -> chroma depth minus 8
                            let num_spsext  = br.read_byte()? as usize;
                            for _ in 0..num_spsext {
                                let len = br.read_u16be()? as usize;
                                // parse spsext
                                          br.read_skip(len)?;
                            }
                        },
                        _ => {},
                    };
                }
            } else {
                return Err(DecoderError::NotImplemented);
            }

            validate!(profile > 0);
            let width  = (vinfo.get_width()  + 15) & !15;
            let height = (vinfo.get_height() + 15) & !15;

            let display = Display::open_silently().ok_or_else(||{ println!(" cannot open display"); DecoderError::InvalidData})?;

            let num_surfaces = self.spses[0].num_ref_frames + 4 + 64;

            let va_profile = match profile {
                    66 => VAProfile::VAProfileH264ConstrainedBaseline,
                    77 => VAProfile::VAProfileH264Main,
                    88 | 100 | 110 | 122 => VAProfile::VAProfileH264High,
                    _ => return Err(DecoderError::NotImplemented),
                };
            if let Ok(profiles) = display.query_config_profiles() {
                if !profiles.contains(&va_profile) {
println!("Profile {} ({}) not supported", profile, profile_name(va_profile));
                    return Err(DecoderError::NotImplemented);
                }
            } else {
                return Err(DecoderError::Bug);
            }
            if let Ok(points) = display.query_config_entrypoints(va_profile) {
                if !points.contains(&VAEntrypoint::VAEntrypointVLD) {
println!("no decoding support for this profile");
                    return Err(DecoderError::NotImplemented);
                }
            } else {
                return Err(DecoderError::Bug);
            }

            let needs_derive= if let Ok(vendor) = display.query_vendor_string() {
                    vendor.contains("Kaby Lake")
                } else { false };

            let config = display.create_config(vec![
                    VAConfigAttrib { type_: VAConfigAttribType::VAConfigAttribRTFormat, value: RTFormat::YUV420.into() },
                ], va_profile, VAEntrypoint::VAEntrypointVLD).map_err(|_| {
println!("config creation failed!");
                    DecoderError::Bug
                })?;
            let surfaces = display.create_surfaces(RTFormat::YUV420, None, width as u32, height as u32, Some(UsageHint::Decoder.into()), num_surfaces as u32).map_err(|_| DecoderError::AllocError)?;
            let context = display.create_context(&config, width as i32, height as i32, Some(&surfaces), true).map_err(|_| DecoderError::Bug)?;

            let ref_pics = Vec::new();

            let image_formats = display.query_image_formats().map_err(|_| DecoderError::Bug)?;
            validate!(!image_formats.is_empty());
            let mut ifmt = image_formats[0];
            for fmt in image_formats.iter() {
                if fmt.bits_per_pixel == 12 {
                    ifmt = *fmt;
                    break;
                }
            }

            self.vaapi = Some(VaapiInternals { display, context, ref_pics, surfaces, ifmt });
            self.needs_derive = needs_derive;

            let vinfo = NAVideoInfo::new(vinfo.get_width(), vinfo.get_height(), false, YUV420_FORMAT);
            self.info = NACodecInfo::new_ref(info.get_name(), NACodecTypeInfo::Video(vinfo), info.get_extradata()).into_ref();
            self.out_frm = alloc_video_buffer(vinfo, 4)?;

            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, pkt: &NAPacket) -> DecoderResult<()> {
        let src = pkt.get_buffer();
        let vactx = if let Some(ref mut ctx) = self.vaapi { ctx } else { return Err(DecoderError::Bug) };

        let timestamp = pkt.get_dts().unwrap_or_else(|| pkt.get_pts().unwrap_or(0));

        if vactx.surfaces.is_empty() {
panic!("ran out of free surfaces");
//            return Err(DecoderError::AllocError);
        }
        let surface = vactx.surfaces.pop().unwrap();
        let surface_id = surface.id();
        let mut pic = Picture::new(timestamp, vactx.context.clone(), surface);
        let mut is_ref = false;
        let mut is_keyframe = false;

        self.tb_num = pkt.ts.tb_num;
        self.tb_den = pkt.ts.tb_den;

        let mut br = MemoryReader::new_read(&src);
        let mut frame_type = FrameType::I;
        let mut nal_buf = Vec::with_capacity(1024);
        while br.left() > 0 {
            let size = match self.nal_len {
                    1 => br.read_byte()? as usize,
                    2 => br.read_u16be()? as usize,
                    3 => br.read_u24be()? as usize,
                    4 => br.read_u32be()? as usize,
                    _ => unreachable!(),
                };
            validate!(br.left() >= (size as i64));
            let offset = br.tell() as usize;
            let raw_nal = &src[offset..][..size];
            let _size = unescape_nal(raw_nal, &mut nal_buf);

            let src = &nal_buf;
            validate!((src[0] & 0x80) == 0);
            let nal_ref_idc   = src[0] >> 5;
            let nal_unit_type = src[0] & 0x1F;

            let mut full_size = src.len() * 8;
            for &byte in src.iter().rev() {
                if byte == 0 {
                    full_size -= 8;
                } else {
                    full_size -= (byte.trailing_zeros() + 1) as usize;
                    break;
                }
            }
            validate!(full_size > 0);

            match nal_unit_type {
                 1 | 5 => {
                    let is_idr = nal_unit_type == 5;
                    is_ref |= nal_ref_idc != 0;
                    is_keyframe |= is_idr;
                    let mut br = BitReader::new(&src[..(full_size + 7)/8], BitReaderMode::BE);
                                                        br.skip(8)?;

                    let slice_hdr = parse_slice_header(&mut br, &self.spses, &self.ppses, is_idr, nal_ref_idc)?;
                    match slice_hdr.slice_type {
                        SliceType::P if frame_type != FrameType::B => frame_type = FrameType::P,
                        SliceType::SP if frame_type != FrameType::B => frame_type = FrameType::P,
                        SliceType::B => frame_type = FrameType::B,
                        _ => {},
                    };
                    let mut cur_sps = 0;
                    let mut cur_pps = 0;
                    let mut pps_found = false;
                    for (i, pps) in self.ppses.iter().enumerate() {
                        if pps.pic_parameter_set_id == slice_hdr.pic_parameter_set_id {
                            cur_pps = i;
                            pps_found = true;
                            break;
                        }
                    }
                    validate!(pps_found);
                    let mut sps_found = false;
                    for (i, sps) in self.spses.iter().enumerate() {
                        if sps.seq_parameter_set_id == self.ppses[cur_pps].seq_parameter_set_id {
                            cur_sps = i;
                            sps_found = true;
                            break;
                        }
                    }
                    validate!(sps_found);
                    let sps = &self.spses[cur_sps];
                    let pps = &self.ppses[cur_pps];

                    if slice_hdr.first_mb_in_slice == 0 {
                        let (top_id, bot_id) = self.frame_refs.calc_picture_num(&slice_hdr, is_idr, nal_ref_idc, sps);
                        if is_idr {
                            self.frame_refs.clear_refs();
                            for (pic, _) in vactx.ref_pics.drain(..) {
                                if let Ok(surf) = pic.take_surface() {
                                    vactx.surfaces.push(surf);
                                } else {
                                    panic!("can't take surface");
                                }
                            }
                        }
                        self.frame_refs.select_refs(sps, &slice_hdr, top_id);
                        let mut pic_refs = Vec::with_capacity(NUM_REF_PICS);
                        for pic in self.frame_refs.ref_pics.iter().rev().take(NUM_REF_PICS) {
                            pic_refs.push(pic.make_pic());
                        }
                        if slice_hdr.adaptive_ref_pic_marking_mode {
                            self.frame_refs.apply_adaptive_marking(&slice_hdr.adaptive_ref_pic_marking, slice_hdr.frame_num, 1 << sps.log2_max_frame_num)?;
                        }

                        while pic_refs.len() < NUM_REF_PICS {
                            pic_refs.push(make_dummy_h264_pic());
                        }

                        let mut flags = H264PictureFlags::default();
                        let frame_idx = if let Some(id) = get_long_term_id(is_idr, &slice_hdr) {
                                flags |= H264PictureFlag::LongTermReference;
                                id as u32
                            } else {
                                if nal_ref_idc != 0 {
                                    flags |= H264PictureFlag::ShortTermReference;
                                }
                                u32::from(slice_hdr.frame_num)
                            };
                        let pic_refs: [PictureH264; NUM_REF_PICS] = pic_refs.try_into().unwrap_or_else(|_| panic!("can't convert"));

                        let h264pic = PictureH264::new(surface_id, frame_idx, flags, top_id as i32, bot_id as i32);

                        let seq_fields = H264SeqFields::new(
                                u32::from(sps.chroma_format_idc),
                                u32::from(sps.separate_colour_plane),
                                u32::from(sps.gaps_in_frame_num_value_allowed),
                                u32::from(sps.frame_mbs_only),
                                u32::from(sps.mb_adaptive_frame_field),
                                u32::from(sps.direct_8x8_inference),
                                u32::from(sps.level_idc >= 31),
                                u32::from(sps.log2_max_frame_num) - 4,
                                u32::from(sps.pic_order_cnt_type),
                                u32::from(sps.log2_max_pic_order_cnt_lsb).wrapping_sub(4),
                                u32::from(sps.delta_pic_order_always_zero)
                            );
                        let pic_fields = H264PicFields::new(
                                u32::from(pps.entropy_coding_mode),
                                u32::from(pps.weighted_pred),
                                u32::from(pps.weighted_bipred_idc),
                                u32::from(pps.transform_8x8_mode),
                                u32::from(slice_hdr.field_pic),
                                u32::from(pps.constrained_intra_pred),
                                u32::from(pps.pic_order_present),
                                u32::from(pps.deblocking_filter_control_present),
                                u32::from(pps.redundant_pic_cnt_present),
                                u32::from(nal_ref_idc != 0)
                            );
                        let ppd = PictureParameterBufferH264::new(
                                h264pic,
                                pic_refs,
                                sps.pic_width_in_mbs as u16 - 1,
                                sps.pic_height_in_mbs as u16 - 1,
                                sps.bit_depth_luma - 8,
                                sps.bit_depth_chroma - 8,
                                sps.num_ref_frames as u8,
                                &seq_fields,
                                pps.num_slice_groups as u8 - 1, // should be 0
                                pps.slice_group_map_type, // should be 0
                                0, //pps.slice_group_change_rate as u16 - 1,
                                pps.pic_init_qp as i8 - 26,
                                pps.pic_init_qs as i8 - 26,
                                pps.chroma_qp_index_offset,
                                pps.second_chroma_qp_index_offset,
                                &pic_fields,
                                slice_hdr.frame_num
                            );
                        let pic_param = BufferType::PictureParameter(PictureParameter::H264(ppd));
                        let buf = vactx.context.create_buffer(pic_param).map_err(|_| DecoderError::Bug)?;
                        pic.add_buffer(buf);

                        let mut scaling_list_8x8 = [[0; 64]; 2];
                        scaling_list_8x8[0].copy_from_slice(&pps.scaling_list_8x8[0]);
                        scaling_list_8x8[1].copy_from_slice(&pps.scaling_list_8x8[3]);
                        let iqmatrix = BufferType::IQMatrix(IQMatrix::H264(IQMatrixBufferH264::new(pps.scaling_list_4x4, scaling_list_8x8)));
                        let buf = vactx.context.create_buffer(iqmatrix).map_err(|_| DecoderError::Bug)?;
                        pic.add_buffer(buf);

                        let cpic = PictureInfo {
                                id: slice_hdr.frame_num,
                                full_id: top_id,
                                surface_id,
                                top_id, bot_id,
                                //pic_type: slice_hdr.slice_type.to_frame_type(),
                                is_ref,
                                is_idr,
                                long_term: get_long_term_id(is_idr, &slice_hdr),
                            };
                        if cpic.is_ref {
                            self.frame_refs.add_short_term(cpic.clone(), sps.num_ref_frames);
                        }
                        if let Some(lt_idx) = cpic.long_term {
                            self.frame_refs.add_long_term(lt_idx, cpic);
                        }
                    }

                    let mut luma_weight_l0 = [0i16; 32];
                    let mut luma_offset_l0 = [0i16; 32];
                    let mut chroma_weight_l0 = [[0i16; 2]; 32];
                    let mut chroma_offset_l0 = [[0i16; 2]; 32];
                    let mut luma_weight_l1 = [0i16; 32];
                    let mut luma_offset_l1 = [0i16; 32];
                    let mut chroma_weight_l1 = [[0i16; 2]; 32];
                    let mut chroma_offset_l1 = [[0i16; 2]; 32];
                    let mut luma_weighted_l0 = false;
                    let mut chroma_weighted_l0 = false;
                    let mut luma_weighted_l1 = false;
                    let mut chroma_weighted_l1 = false;
                    let mut luma_log2_weight_denom = slice_hdr.luma_log2_weight_denom;
                    let mut chroma_log2_weight_denom = slice_hdr.chroma_log2_weight_denom;

                    if (pps.weighted_pred && matches!(slice_hdr.slice_type, SliceType::P | SliceType::B)) || (pps.weighted_bipred_idc == 1 && slice_hdr.slice_type == SliceType::B) {
                        luma_weighted_l0 = true;
                        chroma_weighted_l0 = false;
                        for (i, winfo) in slice_hdr.weights_l0.iter().enumerate().take(slice_hdr.num_ref_idx_l0_active) {
                            if winfo.luma_weighted {
                                luma_weight_l0[i] = winfo.luma_weight.into();
                                luma_offset_l0[i] = winfo.luma_offset.into();
                            } else {
                                luma_weight_l0[i] = 1 << slice_hdr.luma_log2_weight_denom;
                            }
                            if winfo.chroma_weighted {
                                chroma_weight_l0[i][0] = winfo.chroma_weight[0].into();
                                chroma_weight_l0[i][1] = winfo.chroma_weight[1].into();
                                chroma_offset_l0[i][0] = winfo.chroma_offset[0].into();
                                chroma_offset_l0[i][1] = winfo.chroma_offset[1].into();
                            } else {
                                chroma_weight_l0[i][0] = 1 << slice_hdr.chroma_log2_weight_denom;
                                chroma_weight_l0[i][1] = 1 << slice_hdr.chroma_log2_weight_denom;
                                chroma_offset_l0[i][0] = 0;
                                chroma_offset_l0[i][1] = 0;
                            }
                            chroma_weighted_l0 |= winfo.chroma_weighted;
                        }
                    }
                    if pps.weighted_bipred_idc == 1 && slice_hdr.slice_type == SliceType::B {
                        luma_weighted_l1 = true;
                        chroma_weighted_l1 = sps.chroma_format_idc != 0;
                        for (i, winfo) in slice_hdr.weights_l1.iter().enumerate().take(slice_hdr.num_ref_idx_l1_active) {
                            if winfo.luma_weighted {
                                luma_weight_l1[i] = winfo.luma_weight.into();
                                luma_offset_l1[i] = winfo.luma_offset.into();
                            } else {
                                luma_weight_l1[i] = 1 << slice_hdr.luma_log2_weight_denom;
                            }
                            if chroma_weighted_l1 && winfo.chroma_weighted {
                                chroma_weight_l1[i][0] = winfo.chroma_weight[0].into();
                                chroma_weight_l1[i][1] = winfo.chroma_weight[1].into();
                                chroma_offset_l1[i][0] = winfo.chroma_offset[0].into();
                                chroma_offset_l1[i][1] = winfo.chroma_offset[1].into();
                            } else {
                                chroma_weight_l1[i][0] = 1 << slice_hdr.chroma_log2_weight_denom;
                                chroma_weight_l1[i][1] = 1 << slice_hdr.chroma_log2_weight_denom;
                                chroma_offset_l1[i][0] = 0;
                                chroma_offset_l1[i][1] = 0;
                            }
                        }
                    }
                    if pps.weighted_bipred_idc == 2 && slice_hdr.slice_type == SliceType::B {
                        let num_l0 = slice_hdr.num_ref_idx_l0_active;
                        let num_l1 = slice_hdr.num_ref_idx_l1_active;
                        if num_l0 != 1 || num_l1 != 1 { //xxx: also exclude symmetric case
                            luma_weighted_l0 = false;
                            luma_weighted_l1 = false;
                            chroma_weighted_l0 = false;
                            chroma_weighted_l1 = false;
                            luma_log2_weight_denom = 5;
                            chroma_log2_weight_denom = 5;

                            for w in luma_weight_l0.iter_mut() {
                                *w = 32;
                            }
                            for w in luma_weight_l1.iter_mut() {
                                *w = 32;
                            }
                            for w in chroma_weight_l0.iter_mut() {
                                *w = [32; 2];
                            }
                            for w in chroma_weight_l1.iter_mut() {
                                *w = [32; 2];
                            }
                        }
                    }

                    let ref_pic_list_0 = map_ref_list(&self.frame_refs.cur_refs.ref_list0);
                    let ref_pic_list_1 = map_ref_list(&self.frame_refs.cur_refs.ref_list1);

                    let slice_param = SliceParameterBufferH264::new(
                            raw_nal.len() as u32,
                            0, // no offset
                            VASliceDataFlag::All,
                            br.tell() as u16,
                            slice_hdr.first_mb_in_slice as u16,
                            match slice_hdr.slice_type {
                                SliceType::I => 2,
                                SliceType::P => 0,
                                SliceType::B => 1,
                                SliceType::SI => 4,
                                SliceType::SP => 3,
                            },
                            slice_hdr.direct_spatial_mv_pred as u8,
                            (slice_hdr.num_ref_idx_l0_active as u8).saturating_sub(1),
                            (slice_hdr.num_ref_idx_l1_active as u8).saturating_sub(1),
                            slice_hdr.cabac_init_idc,
                            slice_hdr.slice_qp_delta as i8,
                            slice_hdr.disable_deblocking_filter_idc,
                            slice_hdr.slice_alpha_c0_offset / 2,
                            slice_hdr.slice_beta_offset / 2,
                            ref_pic_list_0,
                            ref_pic_list_1,
                            luma_log2_weight_denom,
                            chroma_log2_weight_denom,
                            luma_weighted_l0 as u8, luma_weight_l0, luma_offset_l0,
                            chroma_weighted_l0 as u8, chroma_weight_l0, chroma_offset_l0,
                            luma_weighted_l1 as u8, luma_weight_l1, luma_offset_l1,
                            chroma_weighted_l1 as u8, chroma_weight_l1, chroma_offset_l1,
                        );
                    let slc_param = BufferType::SliceParameter(SliceParameter::H264(slice_param));
                    let buf = vactx.context.create_buffer(slc_param).map_err(|_| DecoderError::Bug)?;
                    pic.add_buffer(buf);

                    let slc_data = BufferType::SliceData(raw_nal.to_vec());
                    let buf = vactx.context.create_buffer(slc_data).map_err(|_| DecoderError::Bug)?;
                    pic.add_buffer(buf);
                },
                 2 => { // slice data partition A
                    //slice header
                    //slice id = read_ue()
                    //cat 2 slice data (all but MB layer residual)
                    return Err(DecoderError::NotImplemented);
                },
                 3 => { // slice data partition B
                    //slice id = read_ue()
                    //if pps.redundant_pic_cnt_present { redundant_pic_cnt = read_ue() }
                    //cat 3 slice data (MB layer residual)
                    return Err(DecoderError::NotImplemented);
                },
                 4 => { // slice data partition C
                    //slice id = read_ue()
                    //if pps.redundant_pic_cnt_present { redundant_pic_cnt = read_ue() }
                    //cat 4 slice data (MB layer residual)
                    return Err(DecoderError::NotImplemented);
                },
                 6 => {}, //SEI
                 7 => {
                    let sps = parse_sps(&src[1..])?;
                    self.spses.push(sps);
                },
                 8 => {
                    validate!(full_size >= 8 + 16);
                    let pps = parse_pps(&src[1..], &self.spses, full_size - 8)?;
                    let mut found = false;
                    for stored_pps in self.ppses.iter_mut() {
                        if stored_pps.pic_parameter_set_id == pps.pic_parameter_set_id {
                            *stored_pps = pps.clone();
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        self.ppses.push(pps);
                    }
                },
                 9 => { // access unit delimiter
                },
                10 => {}, //end of sequence
                11 => {}, //end of stream
                12 => {}, //filler
                _  => {},
            };

            br.read_skip(size)?;
        }

        let bpic = pic.begin().expect("begin");
        let rpic = bpic.render().expect("render");
        let epic = rpic.end().expect("end");

        self.reorderer.add_frame(WaitingFrame {
                pic:    epic,
                is_idr: is_keyframe,
                is_ref,
                ftype:  frame_type,
                ts:     timestamp,
            });

        let mut idx = 0;
        while idx < vactx.ref_pics.len() {
            let cur_surf_id = vactx.ref_pics[idx].1;
            if self.frame_refs.ref_pics.iter().any(|fref| fref.surface_id == cur_surf_id) {
                idx += 1;
            } else {
                let (pic, _) = vactx.ref_pics.remove(idx);
                if let Ok(surf) = pic.take_surface() {
                    vactx.surfaces.push(surf);
                } else {
                    panic!("can't take surface");
                }
            }
        }

        Ok(())
    }
    fn get_frame(&mut self) -> Option<NAFrameRef> {
        if let Some(ref mut vactx) = self.vaapi {
            if let Some(frm) = self.reorderer.get_frame() {
                let ts = frm.ts;
                let is_idr = frm.is_idr;
                let is_ref = frm.is_ref;
                let ftype = frm.ftype;
                if let Ok(pic) = frm.pic.sync() {
                    let _ = fill_frame(vactx.ifmt, &pic, &mut self.out_frm, self.needs_derive);

                    if !is_ref {
                        if let Ok(surf) = pic.take_surface() {
                            vactx.surfaces.push(surf);
                        } else {
                            panic!("can't take surface");
                        }
                    } else {
                        let id = pic.surface_id();
                        vactx.ref_pics.push((pic, id));
                    }

                    let ts = NATimeInfo::new(None, Some(ts), None, self.tb_num, self.tb_den);
                    Some(NAFrame::new(ts, ftype, is_idr, self.info.clone(), self.out_frm.clone()).into_ref())
                } else {
                    panic!("can't sync");
                }
            } else {
                None
            }
        } else {
            None
        }
    }
    fn get_last_frames(&mut self) -> Option<NAFrameRef> {
        if let Some(ref mut vactx) = self.vaapi {
            if let Some(frm) = self.reorderer.frames.pop_front() {
                let ts = frm.ts;
                let is_idr = frm.is_idr;
                let is_ref = frm.is_ref;
                let ftype = frm.ftype;
                if let Ok(pic) = frm.pic.sync() {
                    let _ = fill_frame(vactx.ifmt, &pic, &mut self.out_frm, self.needs_derive);

                    if !is_ref {
                        if let Ok(surf) = pic.take_surface() {
                            vactx.surfaces.push(surf);
                        } else {
                            panic!("can't take surface");
                        }
                    } else {
                        let id = pic.surface_id();
                        vactx.ref_pics.push((pic, id));
                    }

                    let ts = NATimeInfo::new(None, Some(ts), None, self.tb_num, self.tb_den);
                    Some(NAFrame::new(ts, ftype, is_idr, self.info.clone(), self.out_frm.clone()).into_ref())
                } else {
                    panic!("can't sync");
                }
            } else {
                None
            }
        } else {
            None
        }
    }
    fn flush(&mut self) {
        self.frame_refs.clear_refs();
        if let Some(ref mut vactx) = self.vaapi {
            for frm in self.reorderer.frames.drain(..) {
                if let Ok(pic) = frm.pic.sync() {
                    if let Ok(surf) = pic.take_surface() {
                        vactx.surfaces.push(surf);
                    } else {
                        panic!("can't take surface");
                    }
                } else {
                    panic!("can't sync");
                }
            }
            self.reorderer.flush();
            for (pic, _) in vactx.ref_pics.drain(..) {
                if let Ok(surf) = pic.take_surface() {
                    vactx.surfaces.push(surf);
                } else {
                    panic!("can't take surface");
                }
            }
        }
    }
}

use std::thread::*;
use std::sync::mpsc::*;

enum DecMessage {
    Init(NACodecInfoRef),
    Decode(NAPacket),
    Flush,
    GetFrame,
    GetLastFrames,
    End
}

enum DecResponse {
    Ok,
    Nothing,
    Err(DecoderError),
    Frame(NAFrameRef),
}

pub trait HWDecoder {
    fn init(&mut self, info: NACodecInfoRef) -> DecoderResult<()>;
    fn queue_pkt(&mut self, pkt: &NAPacket) -> DecoderResult<()>;
    fn get_frame(&mut self) -> Option<NAFrameRef>;
    fn get_last_frames(&mut self) -> Option<NAFrameRef>;
    fn flush(&mut self);
}

pub struct HWWrapper {
    handle:     Option<JoinHandle<DecoderResult<()>>>,
    send:       SyncSender<DecMessage>,
    recv:       Receiver<DecResponse>,
}

#[allow(clippy::new_without_default)]
impl HWWrapper {
    pub fn new() -> Self {
        let (in_send, in_recv) = sync_channel(1);
        let (out_send, out_recv) = sync_channel(1);
        let handle = std::thread::spawn(move || {
                let receiver = in_recv;
                let sender = out_send;
                let mut dec = VaapiH264Decoder::new();
                while let Ok(msg) = receiver.recv() {
                    match msg {
                        DecMessage::Init(info) => {
                            let msg = if let Err(err) = dec.init(info) {
                                    DecResponse::Err(err)
                                } else {
                                    DecResponse::Ok
                                };
                            sender.send(msg).map_err(|_| DecoderError::Bug)?;
                        },
                        DecMessage::Decode(pkt) => {
                            let msg = match dec.decode(&pkt) {
                                    Ok(()) => DecResponse::Ok,
                                    Err(err) => DecResponse::Err(err),
                                };
                            sender.send(msg).map_err(|_| DecoderError::Bug)?;
                        },
                        DecMessage::GetFrame => {
                            let msg = match dec.get_frame() {
                                    Some(frm) => DecResponse::Frame(frm),
                                    None => DecResponse::Nothing,
                                };
                            sender.send(msg).map_err(|_| DecoderError::Bug)?;
                        },
                        DecMessage::GetLastFrames => {
                            let msg = match dec.get_last_frames() {
                                    Some(frm) => DecResponse::Frame(frm),
                                    None => DecResponse::Nothing,
                                };
                            sender.send(msg).map_err(|_| DecoderError::Bug)?;
                        },
                        DecMessage::Flush => dec.flush(),
                        DecMessage::End => return Ok(()),
                    };
                }
                Err(DecoderError::Bug)
            });

        Self {
            handle:     Some(handle),
            send:       in_send,
            recv:       out_recv,
        }
    }
}

impl HWDecoder for HWWrapper {
    fn init(&mut self, info: NACodecInfoRef) -> DecoderResult<()> {
        if self.send.send(DecMessage::Init(info)).is_ok() {
            match self.recv.recv() {
                Ok(DecResponse::Ok) => Ok(()),
                Ok(DecResponse::Err(err)) => Err(err),
                Err(_) => Err(DecoderError::Bug),
                _ => unreachable!(),
            }
        } else {
            Err(DecoderError::Bug)
        }
    }
    fn queue_pkt(&mut self, pkt: &NAPacket) -> DecoderResult<()> {
        let pkt2 = NAPacket::new_from_refbuf(pkt.get_stream(), pkt.ts, pkt.keyframe, pkt.get_buffer());
        if self.send.send(DecMessage::Decode(pkt2)).is_ok() {
            match self.recv.recv() {
                Ok(DecResponse::Ok) => Ok(()),
                Ok(DecResponse::Err(err)) => Err(err),
                Err(_) => Err(DecoderError::Bug),
                _ => unreachable!(),
            }
        } else {
            Err(DecoderError::Bug)
        }
    }
    fn get_frame(&mut self) -> Option<NAFrameRef> {
        if self.send.send(DecMessage::GetFrame).is_ok() {
            match self.recv.recv() {
                Ok(DecResponse::Frame(frm)) => Some(frm),
                Ok(DecResponse::Nothing) => None,
                Err(_) => None,
                _ => unreachable!(),
            }
        } else {
            None
        }
    }
    fn get_last_frames(&mut self) -> Option<NAFrameRef> {
        if self.send.send(DecMessage::GetLastFrames).is_ok() {
            match self.recv.recv() {
                Ok(DecResponse::Frame(frm)) => Some(frm),
                Ok(DecResponse::Nothing) => None,
                Err(_) => None,
                _ => unreachable!(),
            }
        } else {
            None
        }
    }
    fn flush(&mut self) {
        let _ = self.send.send(DecMessage::Flush);
    }
}

impl Drop for HWWrapper {
    fn drop(&mut self) {
        if self.send.send(DecMessage::End).is_ok() {
            let mut handle = None;
            std::mem::swap(&mut handle, &mut self.handle);
            if let Some(hdl) = handle {
                let _ = hdl.join();
            }
        }
    }
}


pub fn new_h264_hwdec() -> Box<dyn HWDecoder + Send> {
    Box::new(HWWrapper::new())
}
