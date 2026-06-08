Here is a review of commit `1116e2a`:

---

## Commit Review: `video: Add support for playing F4V (MP4) files`

**Summary**: Adds `NetStreamType::F4v` backed by the `re_mp4` crate, with detection, seeking, video decoding (H.264), and AAC audio playback.

---

### Positives

- Clean separation of video/audio exhaustion logic with `video_done`/`audio_done` flags before setting `buffer_underrun`.
- Lazy MP4 parsing (retried each tick until the `moov` box is fully buffered) is a reasonable strategy for streaming.
- The fast-forward/keyframe-skip logic (500 ms threshold) is well thought out and correctly avoids decoding non-sync frames mid-skip.
- Improved FLV detection now also validates the version byte (`1`), which is a correctness improvement.

---

### Issues

**1. `// ???` debugging artifacts left in** (streams.rs)
```rust
source.preload_offset.set(8); // ???
```
Appears in two error paths. The uncertainty should be resolved or at minimum the comment replaced with an explanation.

---

**2. `unwrap()` on codec config and track lookups — potential panics**
```rust
let ccfg = trk.raw_codec_config(media_context).unwrap();
// and
let trk = media_context.tracks().get(&vti).unwrap();
let audio_trk = media_context.tracks().get(&ati).unwrap();
```
Track IDs are stored at parse time and then re-looked-up on every tick. If the `re_mp4` API ever returns `None` (e.g. corrupt file), this panics. Should use `if let` or `?`-style error handling with a `tracing::error!` + graceful exit.

---

**3. `audio_buffer` grows unboundedly**
```rust
audio_buffer: Buffer,
```
Audio samples are appended to `audio_buffer` indefinitely (header + every AAC frame for the whole file lifetime). For a long video this is a significant memory leak. The buffer is only reset on seek. Consider freeing already-submitted audio data.

---

**4. `Rc<re_mp4::Mp4>` inside a GC-collected type**
```rust
context: Option<Rc<re_mp4::Mp4>>,
```
`NetStreamType` derives `Collect`. Using `std::rc::Rc` here is unusual in gc-arena contexts — typically `Gc`/`GcCell` or a `#[collect(require_static)]` annotation is needed. If the derive macro isn't told to treat this as opaque, it may not compile or may silently miss GC roots. Worth verifying the attribute on the enum is correct.

---

**5. FLV error-path `preload_offset` silently changed from 3 → 8**
The old code set `preload_offset` to `3` on FLV header parse failure; the new code sets it to `8`. This is a behavior change for existing FLV files in error paths and needs justification (or the `// ???` resolved).

---

**6. Hardcoded `(8, 8)` video dimensions at stream registration**
```rust
context.video.register_video_stream(1, (8, 8), VideoCodec::H264, ...)
```
The actual dimensions aren't passed from the MP4 track info. If the video backend uses the registered size before the first decoded frame, UI layout may be wrong. The F4V track's width/height are available in `re_mp4` — these should be passed here.

---

**7. AAC AudioSpecificConfig `profile` field — possible off-by-one**
```rust
(ds.profile << 3) | (ds.freq_index >> 1),
```
`AudioSpecificConfig.audioObjectType` is 1-based (AAC-LC = 2). If `re_mp4`'s `dec_specific.profile` stores a 0-based index, the constructed header will be wrong. This should be verified against the `re_mp4` docs/source.

---

**8. `num_samples_per_block: 0` in `SoundStreamInfo`**
```rust
num_samples_per_block: 0,
```
It's unclear if `0` is a valid/safe sentinel for the `Unwrapped` wrapping type — worth checking if downstream audio code divides by this value.

---

### Minor

- The `seek_audio_sample` falls back to `0` when no video sync frame is found. This is correct (both tracks reset to start), but a comment explaining this would help.
- `mp4a.samplerate.value()` — if this returns a value > `u16::MAX` it will silently truncate when stored in `SoundFormat::sample_rate: u16`.