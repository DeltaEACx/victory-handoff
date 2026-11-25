# RTX 4050 handoff — November 25, 2026

**Not fixed in the title.** The opening still flashes, the cinematic can
render very dark, and the warmed run was below one frame per second.
Audio lag and opening gameplay have not been resolved.

## What was verified

- The isolated first-boot profile reached the actual setup screen and wrote
  settings without resetting the normal profile. The account picker is not
  the setup screen.
- The wave32 admission and cache-key corrections prevented unsupported
  wave64 DXIL from being treated as a valid RTX 4050 translation. Three
  original compute shaders and two original pixel shaders created offline
  PSOs on the RTX 4050. The synthetic Rust-emitted paired-wave fixture
  matched 2,048 lane records, both with and without optimization.
- An eligible RGBA8 texture viewed as sRGB was sampled incorrectly when the
  host dropped sRGB decoding. The opt-in typeless storage/typed-view path
  passed offline descriptor and hardware readback tests. There is no
  same-shot proof it repairs this title's dark scene.
- Live D3D12 message 676 rejected the windshield-mask mesh PSO because
  blending was enabled on an `R8_UINT` target. The opt-in replacement
  workaround created that PSO offline and in two live runs. **Those runs
  did not fix the flash, darkness, or FPS.** Replacement is not MAX for
  overlapping fragments. An offline synthetic numeric MAX using UNORM
  aliases matched every ordered byte pair and UINT readback, but title
  export semantics and runtime bindings are not established.
- The warmed live run recorded 1,786–2,236 ms/frame with no PSOs still
  compiling. The aggregate heap pass took about 768–820 ms/frame, and one
  window used 5,635 MB of a 5,119 MB local budget. Batched texture mapping
  returned the same results as single-block queries in a sparse Windows
  fixture (about 5 ms versus 12 ms over 32 scans). No live gain was measured.
- Targeted capture, queue, tag correlation, and title-memory tests passed
  in the latest isolated build. These were not a full-suite pass.

## Where to continue

1. Use `-PresentTokenTrace` and join the observations by frame. The black
   interruption and repeated 4×4 samples are in the title's present source,
   before DXGI. Reused source handles are compatible with double buffering.
   A prior ExecuteX token match identifies a call, not the surface's last
   writer or GPU completion. Do not suppress black frames on this evidence.
2. Use `-PackedLightAfter '2,6,12'` to compare the `R32_UINT` writer and
   `R9G9B9E5` sampled alias. Check copy and writer order before attributing
   different end-of-frame hashes to the copy. No retained pair is not
   evidence of zero light. Keep this separate from frame-time runs.
3. Lower the cascade-shadow shader (`f133e35efed2a48a`) with exact
   per-logical-wave control, EXEC, scalar PHIs, and memory effects. Preserve
   the current wave32 rejection until the unresolved alias/range contract
   is proved. DefragHeap (`02384a60fda6f2d0`) also still requires exact
   multi-wave LDS scheduling. Do not enable approximate half-wave voting.
4. Leave the ray-tracing builder off: partial translation previously
   corrupted title memory. Missing indirect light and discarded sRGB
   decoding are distinct issues. Use the heap-pass phase timings to isolate
   costs and compare relay modes on the same warmed profile.
