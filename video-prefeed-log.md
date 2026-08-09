# Video decode prefeed — execution log

Working branch: `video-decoding-prefeed` (branched off `master` + the unrelated
README-badges commit that was already there).

Design note: https://claude.ai/code/artifact/ed496464-e028-4577-9207-49303ceac158

Goal: separate the decode timeline from the presentation timeline, so H.264
frames are fed to the decoder ahead of the playhead and shown when they are
actually due. H.263/VP6/Screen must be bit-identical throughout.

This file is scratch and is deliberately **not** committed.

---

## Stage plan

| # | Stage | Commit | Status |
|---|-------|--------|--------|
| 1 | submit/poll `VideoDecoder` API + `LowDelay` adapter | `b754cf06eb` | done |
| 2 | `PresentationQueue` + `PresentationTime` plumbing | — | not started |
| 3 | `NetStream::tick` feed/presentation cursors, CTS → PTS | — | not started |
| 4 | per-decoder lookahead + priming feed | — | not started |
| 5 | OpenH264 → `DecodeFrame2` + flush/reset | — | not started |
| 6 | WebCodecs output queue | — | not started |
| 7 | EOS/seek lifecycle + test re-baseline | — | not started |

## Invariants to hold at every commit

- `cargo check -p ruffle_video_external -p ruffle_core --features ruffle_video_external/openh264` clean.
- `cargo clippy` clean for the touched crates (workspace denies a fair bit).
- Stages 1–2 must not change rendered output at all: `visual/video/vp6_dispsize`,
  `vp6_alphaoffset`, `deblocking`, `colorconversion` are the canaries.
- Never `git add -A` — the working tree is full of the user's untracked scratch
  files, including this log.

---

## Environment notes

- `target/` is warm (~24G), so incremental checks are cheap.
- OpenH264 2.4.1 is already downloaded at `target/debug/deps/libopenh264-2.4.1-linux64.7.so`.
- Video visual tests need `--features imgtests` (pulls in wgpu + both video backends)
  and a working Vulkan device.

---

## Log

### 2026-08-09 — setup

- Read the current pipeline end to end: `video/src/backend.rs`,
  `video/{software,external}/src/backend.rs`, the three software decoders,
  `openh264.rs`, `webcodecs.rs`, `core/src/display_object/video.rs`,
  `core/src/streams.rs`, `core/src/player.rs` tick order.
- Confirmed `SBufferInfo::{uiInBsTimeStamp,uiOutYuvTimeStamp}` and
  `DECODER_OPTION_NUM_OF_FRAMES_REMAINING_IN_BUFFER` exist in the generated
  bindings, so tagging + drain-loop are both possible for stage 5.
- Confirmed `re_mp4::Sample` carries `composition_timestamp` (stage 3 / f4v).
- Baseline compile check kicked off.

### Stage 1 — submit/poll decoder API

Commit `b754cf06eb`.

- `ruffle_video::frame::DecodedFrameOut` = `{ frame_id, frame }`.
- `VideoDecoder` is now `submit_frame` / `poll_frames` / `flush` / `reset`
  (+ the unchanged `configure_decoder` / `preload_frame`).
- New `LowDelayDecoder` trait keeps the old one-in-one-out shape; `LowDelay<D>`
  adapts it. h263, vp6, screen, openh264 and webcodecs all moved to it verbatim
  — only the trait name and the `use` line changed in those files.
- Both backends gained a private `VideoStream::decode_frame` shim that submits
  and immediately polls, so `VideoBackend` is untouched and behaviour is
  identical.

Verified: native + `wasm32-unknown-unknown` (webcodecs) check clean, clippy
clean, rustfmt applied, and `cargo test -p tests --features imgtests -- video`
is 12/12 including `visual/video/h264`.

Left alone deliberately: the `VideoDecoder` trait still lives in
`ruffle_video_software` and is re-exported by `ruffle_video_external`, which is
backwards — the interface belongs in `ruffle_video`. Not worth widening this
series for; worth a follow-up.

### Stage 3 — the timeline (merged with what was planned as stage 5)

**The plan was wrong about ordering, and the h264 test caught it.**

Splitting `NetStream` onto a real presentation timeline first, and fixing the
OpenH264 decoder afterwards, cannot work. `LowDelay` tags each picture with the
`frame_id` of the frame that was *submitted*, but OpenH264 hands pictures back
in display order, so those tags are lies. With `pts = frame_id` that was
invisible; the moment the pts became a real timestamp, every picture landed at
the wrong time and the screen went blank. Tried the other order too (decoder
first, container second) on paper: also broken, because presenting "as of the
tag just fed" thrashes against display-order output. The two are one change.

So this commit is: OpenH264 submit/poll + `NetStream` feed/presentation cursors
+ CTS → PTS + the end-of-stream handling, together.

**Three OpenH264 findings, none of them in the design note:**

1. `DecodeFrame2` propagates `uiInBsTimeStamp` → `uiOutYuvTimeStamp` reliably,
   and emits in display order. The spike I flagged as gating stage 5 passed.
2. `FlushFrame` does nothing until `DECODER_OPTION_END_OF_STREAM` is set first.
   Without it, `DECODER_OPTION_NUM_OF_FRAMES_REMAINING_IN_BUFFER` keeps
   reporting 2 and the drain hands back nothing, forever.
3. Even then the *last* picture stays stuck, because the Annex B parser holds an
   access unit until it sees the start of the next one. A `DecodeFrame2` with a
   null buffer pops it. Draining must therefore loop until nothing comes out
   rather than until the remaining-frames count hits zero — that count only
   covers the reorder buffer, not the parser.

**The lookahead is not a fixed 3-4 frames.** The design note guessed a per-codec
frame count. On the `hsv.flv` test stream (2 fps) the reorder depth is 2500 ms;
a 3-frame guess would have been fine, but a 100 ms time-based horizon was not.
It is now taken from the stream itself: `max(composition_time_offset)` seen so
far, plus one observed frame interval as margin. Zero for every codec without
B-frames, so H.263/VP6 feed exactly as far ahead as the audio wants and no
further.

**`AUDIO_LOOKAHEAD_MS = 100` replaces `max_lookahead_audio_tags = 5`.** The tag
count could not bound the feed cursor at all on a video-only stream — with one
cursor that was harmless because lookahead tags were re-read every tick, but a
cursor that *feeds* what it reads would have submitted the whole file on tick 1.

**End of stream had to move.** The feed cursor reaches EOF a lookahead before
the playhead does, and on this stream the last picture is due 1000 ms after the
final tag's timestamp. `Play.Stop` is now gated on `video_stream_is_drained`.
Before this, the last two frames of the test video were decoded and then thrown
away unseen — the old expected images had frames 9/10/11 all stuck on
presentation frame 6.

**Test re-baseline, verified rather than accepted.** Decoded `hsv.flv` with
ffmpeg to get the nine pictures in presentation order; their mean brightness is
monotonic in 6.3 steps, so each rendered frame can be identified unambiguously.
Checked every trigger against what the presentation timeline says should be on
screen at that millisecond before touching any expected image:

| trigger | t (ms) | old expected | correct | now |
|---------|--------|--------------|---------|-----|
| 5       | 250    | (disabled)   | nothing | nothing |
| 15      | 750    | (disabled)   | nothing | nothing |
| 25–85   | 1250–4250 | pres 0–6  | pres 0–6 | unchanged |
| 95      | 4750   | pres 6       | pres 7  | pres 7 |
| 105     | 5250   | pres 6       | pres 8  | pres 8 |
| 115     | 5750   | pres 6       | pres 8  | pres 8 |

frame0/frame1 are enabled for the first time; both are correctly blank, which is
what Flash Player shows for the first second of this stream.

### Stage 4 — WebCodecs (was stage 6)

Commit `1ebe045705`. Straightforward once the shape was settled by stage 3:
`Rc<RefCell<Option<DecodedFrame>>>` → `Rc<RefCell<Vec<DecodedFrameOut>>>`, and
the chunk's `frame_id` rides on `EncodedVideoChunkInit.timestamp` (an `i32` in
this web-sys version, so it is round-tripped `f64 as i64 as u32` on the way
back). `flush` ignores the promise; `reset` re-configures, because resetting a
WebCodecs decoder leaves it unconfigured.

Dropped `set_optimize_for_latency(true)`: it was propping up the assumption that
output had to be available immediately, which is exactly the assumption that
went away.

Cannot be exercised locally — no wasm test path for video. Verified only that
both feature sets compile clean (native openh264, wasm32 webcodecs, and
`ruffle_web` itself) with clippy silent.

### Queue unit tests

Commit `60739bd696`. Eight tests over `PresentationQueue` through a
`NullRenderer`, covering the cases no visual test reaches: out-of-order
delivery, holding back frames that are not due, skipping late ones, never going
backwards, reset keeping the on-screen picture, drain semantics, and pictures
delivered for frames that were never submitted.

---

## Final state

Five commits on `video-decoding-prefeed`:

| commit | what |
|--------|------|
| `b754cf06eb` | submit/poll `VideoDecoder` + `LowDelay` adapter |
| `47865f2534` | `PresentationQueue`, `submit`/`present` backend API |
| `6e30e7cc39` | feed/presentation cursors, CTS → PTS, OpenH264 rework, EOS/seek |
| `1ebe045705` | WebCodecs output queue |
| `60739bd696` | queue unit tests |

Verification: `cargo test -p tests --features imgtests` is 4476 passed / 8
failed, and all 8 fail identically on the unmodified tree (missing local fonts,
GPU-specific rendering — edittext, pixelbender, stage3d, acid-mask). Video and
netstream are 12/12 and 9/9. Native, `wasm32-unknown-unknown` + webcodecs, and
`ruffle_web` all build clean with clippy and rustfmt silent.

## Not done

- **No H.264 seek test.** Both `netstream_seek_flv` tests use Sorenson H.263, so
  `reset_video_stream` is only covered for a codec that has nothing to reset.
  Adding one needs a new test SWF, which the repo keeps pre-built from `.fla`
  files rather than compiling.
- **No audio work.** The AAC priming trim and folding `latency_seek` in for AAC
  are untouched; only the audio *lookahead* changed, from five tags to 100 ms.
- **f4v branch not rebased.** It should now use `sample.composition_timestamp`
  for presentation and keep `decode_timestamp` for the feed cursor, and its
  "may be startup delay" warning arm can go.
- **OpenH264 still pinned to 2.4.1.** Nothing depends on `DecodeFrameNoDelay`
  any more, so the version and its per-platform hashes can be bumped, but that
  is its own change and wants testing against a newer binary.
- **`VideoDecoder` still lives in `ruffle_video_software`** and is re-exported by
  `ruffle_video_external`. The interface belongs in `ruffle_video`.
