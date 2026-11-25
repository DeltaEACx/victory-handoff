//! Exact one-console-wave compute control flow on two host waves.
//!
//! Native scalar control flow is one program counter for the entire console
//! wave, not one program counter for each enabled lane. Keeping that property
//! also keeps scalar PHIs scalar: a disabled lane must see a scalar update
//! made while a different lane has EXEC set.
//!
//! First predicate proven empty-EXEC skips (with IR SSA reconstruction). Keep
//! skips with observable scalar changes, including SCC and loop counters.
//! Then close *all remaining control dependencies*, not only dependencies of
//! an LDS instruction reported by a failed lowering. Widening one branch can
//! remove path knowledge that previously made a nested branch seem uniform;
//! reconstruct and inspect the structure until no such branch remains.
//!
//! The final graph has only scalar-uniform decisions or exact native mask
//! decisions reduced across both host halves. Thus every thread follows the
//! same block/loop/exit trace. EXEC remains an instruction guard, not a reason
//! for a host half to miss a rendezvous. Existing lowering consequently places
//! its LDS phase barriers and mask exchanges outside instruction guards, and
//! native scalar PHIs select the same incoming edge for all 64 threads.
//!
//! This is not the global WAVE_WIDE switch: no other stage, group size, host
//! width, or approximate per-half operation is admitted by this form.

use std::collections::HashSet;

use crate::ShaderError;
use crate::ir::Program;
use crate::native::{HwStage, NativeProgram, StageIo};
use crate::structure::{self, Atom, LaneExit, Structured};

/// A deliberately bounded form: one full console wave, split into exactly
/// two host waves. No multi-console-wave rendezvous is admitted here.
pub(super) fn one_wave(
    native: &NativeProgram,
    p: &Program,
) -> Result<(Program, Structured), ShaderError> {
    let StageIo::Cs(cs) = native.io else {
        return Err(not_one_wave());
    };
    if native.stage != HwStage::Cs
        || !native.wave64
        || !p.wave64
        || cs.threads.iter().product::<u32>() != 64
    {
        return Err(not_one_wave());
    }
    // If EXEC is empty the native skip has no vector effects. Remove only
    // skips whose scalar results cannot escape the region. This existing IR
    // transformation recomputes predecessors and renames scalar/vector SSA,
    // rather than splicing edges while keeping obsolete PHI arguments.
    let p = crate::ir::without_skips(p, true).unwrap_or_else(|| p.clone());
    let mut wide = HashSet::new();
    loop {
        let st = structure::structure_wide(&p, &wide)?;
        let before = wide.len();
        for (b, exit) in st.exits.iter().enumerate() {
            let LaneExit::Branch { when, .. } = *exit else {
                continue;
            };
            if st
                .bools
                .support(when)
                .iter()
                .any(|a| !matches!(a, Atom::Uniform(_)))
            {
                if st.wave_any[b].is_none() {
                    return Err(ShaderError::Unsupported {
                        offset: p.blocks[b].offset,
                        what: "one-wave convergence cannot prove a native branch",
                    });
                }
                wide.insert(b);
            }
        }
        if wide.len() == before {
            for &b in &st.wide {
                if st.wave_any[b].is_none() {
                    return Err(ShaderError::Unsupported {
                        offset: p.blocks[b].offset,
                        what: "one-wave convergence lost a native mask predicate",
                    });
                }
            }
            if st.exits.iter().any(|e| matches!(e, LaneExit::Indirect)) {
                return Err(ShaderError::Unsupported {
                    offset: 0,
                    what: "one-wave convergence cannot follow an indirect exit",
                });
            }
            return Ok((p, st));
        }
    }
}

fn not_one_wave() -> ShaderError {
    ShaderError::Unsupported {
        offset: 0,
        what: "one-wave convergence requires exactly 64 compute threads",
    }
}

#[cfg(test)]
#[path = "converge_tests.rs"]
mod tests;
