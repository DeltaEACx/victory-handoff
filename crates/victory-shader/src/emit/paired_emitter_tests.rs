//! Emit only the dual-bank wave64 prologue and primitives, not native ISA lowering.

use super::{PAIRED_MASK_CASES, rtx4050};
use crate::emit::{
    HostShader, UserData,
    bitcode::{Attr, Bin, Rmw, Term, V, pred},
    host::{Dispatch, Root, View},
    shader::{Shader, flags},
    validator::Validator,
};
use crate::native::{HwStage, RootType};

const WAVE: u32 = 32;
const VIRTUAL_THREADS: u32 = 128;
const GROUPS: [u32; 3] = [2, 2, 1];
const RECORD_WORDS: u32 = 8;
const NO_LANE: u32 = u32::MAX;

fn emit_fixture() -> HostShader {
    let mut s = Shader::new(false);
    s.threads = Some([64, 1, 1]);
    s.wave_size = Some(WAVE);
    s.lds_dwords = 1;
    s.user_data = 8;
    s.flags |= flags::WAVE_OPS;
    let (i1, i32, void) = (s.t.i1, s.t.i32, s.t.void);
    let [zero, one, width] = [0, 1, WAVE].map(|n| s.m.i32(n));

    // A single group-wide rendezvous before the native waves can diverge.
    let (entry, init, sync, allocate, body) = (
        s.entry, s.m.new_block(), s.m.new_block(), s.m.new_block(), s.m.new_block(),
    );
    let lds = s.lds();
    let next_wave = s.m.element(entry, lds, zero);
    let flat = s.op(entry, "flattenedThreadIdInGroup.i32", Attr::ReadNone, i32, 96, &[]);
    let initialize = s.m.cmp(entry, pred::EQ, flat, zero);
    s.m.terminate(entry, Term::CondBr(initialize, init, sync));
    s.m.store(init, next_wave, zero);
    s.m.terminate(init, Term::Br(sync));
    let mode = s.m.i32(9); // SyncThreadGroup | TGSMFence
    s.op(sync, "barrier", Attr::NoDuplicate, void, 80, &[mode]);
    let leader = s.op(sync, "waveIsFirstLane", Attr::ReadOnly, i1, 110, &[]);
    s.m.terminate(sync, Term::CondBr(leader, allocate, body));
    let id = s.m.atomic(allocate, Rmw::Add, next_wave, one);
    s.m.terminate(allocate, Term::Br(body));
    let id_phi = s.m.phi(body, i32);
    s.m.set_phi(id_phi, vec![(zero, sync), (id, allocate)]);
    let wave = s.op(body, "waveReadLaneFirst.i32", Attr::NoUnwind, i32, 118, &[id_phi]);

    // All physical lanes participate; EXEC only controls each bank's vote.
    let lane = s.op(body, "waveGetLaneIndex", Attr::ReadOnly, i32, 111, &[]);
    let own_bit = s.m.bin(body, Bin::Shl, one, lane);
    let cb = s.user_data_handle();
    let masks = s.op(body, "cbufferLoadLegacy.i32", Attr::ReadOnly, s.t.cbuf_ret, 59, &[cb, wave]);
    let four = s.m.named_struct("dx.types.fouri32", vec![i32; 4]);
    let ballot = [0, 1].map(|bank| {
        let exec = s.m.extract(body, masks, bank);
        let predicate = s.m.extract(body, masks, bank + 2);
        let enabled = s.m.bin(body, Bin::And, exec, predicate);
        let own = s.m.bin(body, Bin::And, enabled, own_bit);
        let vote = s.m.cmp(body, pred::NE, own, zero);
        let voted = s.op(body, "waveActiveBallot", Attr::NoUnwind, four, 116, &[vote]);
        s.m.extract(body, voted, 0)
    });
    let popcount =
        |s: &mut Shader, bits: V| s.op(body, "unaryBits.i32", Attr::ReadNone, i32, 31, &[bits]);
    let lo_count = popcount(&mut s, ballot[0]);
    let hi_count = popcount(&mut s, ballot[1]);
    let count = s.m.bin(body, Bin::Add, lo_count, hi_count);
    let below = s.m.bin(body, Bin::Sub, own_bit, one);
    let [lo_prefix, hi_prefix] = ballot.map(|bits| {
        let below = s.m.bin(body, Bin::And, bits, below);
        popcount(&mut s, below)
    });
    let hi_prefix = s.m.bin(body, Bin::Add, lo_count, hi_prefix);

    let group_x = s.op(body, "groupId.i32", Attr::ReadNone, i32, 94, &[zero]);
    let group_y = s.op(body, "groupId.i32", Attr::ReadNone, i32, 94, &[one]);
    let groups_x = s.m.i32(GROUPS[0]);
    let row = s.m.bin(body, Bin::Mul, group_y, groups_x);
    let group = s.m.bin(body, Bin::Add, group_x, row);
    let group_stride = s.m.i32(VIRTUAL_THREADS);
    let group_base = s.m.bin(body, Bin::Mul, group, group_stride);
    let native_width = s.m.i32(2 * WAVE);
    let wave_base = s.m.bin(body, Bin::Mul, wave, native_width);
    let lo = s.m.bin(body, Bin::Add, wave_base, lane);
    let hi = s.m.bin(body, Bin::Add, lo, width);
    let indices = [lo, hi].map(|local| s.m.bin(body, Bin::Add, group_base, local));

    // Prefer the low bank. The empty case shuffles from a valid lane,
    // then discards the result.
    let any = s.m.cmp(body, pred::NE, count, zero);
    let has_lo = s.m.cmp(body, pred::NE, ballot[0], zero);
    let bits = s.m.select(body, has_lo, ballot[0], ballot[1]);
    let nonempty_bits = s.m.select(body, any, bits, one);
    let first = s.op(body, "unaryBits.i32", Attr::ReadNone, i32, 32, &[nonempty_bits]);
    let [first_lo, first_hi] = indices.map(|index| {
        let value = s.m.bin(body, Bin::Add, index, one);
        s.op(body, "waveReadLaneAt.i32", Attr::NoUnwind, i32, 117, &[value, first])
    });
    let first_value = s.m.select(body, has_lo, first_lo, first_hi);
    let no_lane = s.m.i32(NO_LANE);
    let first_value = s.m.select(body, any, first_value, no_lane);
    let wave_size = s.op(body, "waveGetLaneCount", Attr::ReadOnly, i32, 112, &[]);

    let out = s.heap_handle(body, zero, false, false, (11 | 0x1000, 0));
    let stride = s.m.i32(RECORD_WORDS * 4);
    let half_record = s.m.i32(16);
    let (undef, write4, align) = (s.m.undef(i32), s.m.i8(15), s.m.i32(4));
    for (index, local, prefix) in [(indices[0], lo, lo_prefix), (indices[1], hi, hi_prefix)] {
        let at = s.m.bin(body, Bin::Mul, index, stride);
        let next = s.m.bin(body, Bin::Add, at, half_record);
        for (addr, [a, b, c, d]) in [
            (at, [local, flat, ballot[0], ballot[1]]),
            (next, [count, prefix, first_value, wave_size]),
        ] {
            s.op(body, "rawBufferStore.i32", Attr::NoUnwind, void, 140,
                &[out, addr, undef, a, b, c, d, write4, align]);
        }
    }
    s.m.terminate(body, Term::Ret);

    HostShader {
        stage: HwStage::Cs,
        dxil: s.finish(),
        user_data: (0..8)
            .map(|index| UserData { index, slot: 0, root: RootType::Constants, dword: index })
            .collect(),
        samplers: vec![],
        static_samplers: vec![],
        root_cbvs: vec![],
        root_srvs: vec![],
        root_uavs: vec![],
        root_indexed_views: vec![],
    }
}

#[test]
#[ignore = "explicit RTX4050 dispatch; requires Windows SDK dxil.dll"]
fn emitted_paired_ballot_prefix_and_first_lane_on_rtx4050() {
    let mut shader = emit_fixture();
    let dll = Validator::default_path().expect("Windows SDK dxil.dll");
    Validator::load(Some(&dll))
        .unwrap()
        .validate(&mut shader.dxil)
        .expect("validate emitter-generated DXIL");
    eprintln!("validated emitter-generated DXIL with {}", dll.display());
    if let Some(path) = std::env::var_os("VICTORY_PAIRED_EMITTER_DXIL") {
        std::fs::write(path, &shader.dxil).unwrap();
    }
    let host = rtx4050();
    let (root, pso) = host.compute_pipeline(&shader).expect("create hardware compute PSO");
    let groups: u32 = GROUPS.into_iter().product();
    for masks in PAIRED_MASK_CASES {
        let result = host.dispatch(
            &root, &pso,
            &Dispatch {
                roots: vec![(0, Root::Constants(masks.into_iter().flatten().collect()))],
                views: &[View::RawUav(groups * VIRTUAL_THREADS * RECORD_WORDS)],
                samplers: &[],
                groups: GROUPS,
            },
        ).unwrap();
        for group in 0..groups {
            let mut seen_flat = 0u64;
            for (wave, [exec_lo, exec_hi, pred_lo, pred_hi]) in masks.into_iter().enumerate() {
                let (lo, hi) = (exec_lo & pred_lo, exec_hi & pred_hi);
                let mask = u64::from(lo) | (u64::from(hi) << 32);
                let base = (wave as u32) * 2 * WAVE;
                let group_base = group * VIRTUAL_THREADS;
                let first = if mask == 0 { NO_LANE } else { group_base + base + mask.trailing_zeros() + 1 };
                let record = |lane| {
                    let at = ((group_base + base + lane) * RECORD_WORDS) as usize;
                    &result[0][at..at + RECORD_WORDS as usize]
                };
                for lane in 0..2 * WAVE {
                    let flat = record(lane % WAVE)[1];
                    let prefix = (mask & ((1u64 << lane) - 1)).count_ones();
                    assert_eq!(
                        record(lane),
                        &[base + lane, flat, lo, hi, mask.count_ones(), prefix, first, WAVE],
                        "masks={masks:?}, group {group}, logical wave {wave}, lane {lane}",
                    );
                    if lane < WAVE {
                        assert!(flat < 64, "group {group}: invalid physical index {flat}");
                        seen_flat |= 1u64 << flat;
                    }
                }
            }
            assert_eq!(seen_flat, u64::MAX, "group {group}: aliased physical lanes");
        }
        eprintln!("{} emitted virtual lanes exact for {masks:?}", groups * VIRTUAL_THREADS);
    }
}
