# Wave32 admission and LDS follow-up

`emit::emit` checks the wave requirement after all exact lowering retries,
including constant-edge threading. A remaining wave64 requirement must
return `ShaderError::Unsupported` on a known max-32 host. Checking before
the retries would also reject shaders that can be lowered exactly. The
shader cache separates host wave limits and wave-local modes.

The Windows synthetic paired-lane fixture establishes exact per-wave
ballot, count, prefix, and lane-read results for the two-bank model on an
RTX 4050. It does not implement an arbitrary original shader. In particular,
an EXEC region with interleaved dependent LDS loads and stores cannot be
made exact by fences only at its boundaries; a group-wide barrier also
cannot be placed on a branch an independent wave might skip. Preserve
scalar loop/PHI state and guarded memory effects before admitting new
cases. Never enable approximate half-wave votes globally.

## Regression tests

Use the Windows SDK DXIL validator for shader validation. From the
repository root:

```powershell
cargo test --locked -p victory-shader --lib emit::converge::tests
cargo test --locked -p victory-shader --features host --lib emitted_paired_ballot_prefix_and_first_lane_on_rtx4050 -- --ignored --nocapture --test-threads=1
```

Original-container tests are opt-in. Provide your own local fixture
directory outside the checkout via `VICTORY_WAVE32_FIXTURES`, and run
`cargo test --locked -p victory-shader --lib original_wave64_lds_failures -- --ignored`.
Keep the containers, dumps, and disassembly local. The test results and
open title issues are tracked in [the handoff status](../handoff/PROGRESS.md).
