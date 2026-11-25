use super::*;
use crate::ir::{Exit, Reg};
use crate::native::{ComputeIo, ResourceType, RootParam, RootType, UsedResource};
use crate::{
    dxbc::{Container, FourCC},
    io, resource,
};

#[cfg(windows)]
#[path = "paired_emitter_tests.rs"]
mod paired_emitter;

fn synthetic(body: &[u32], threads: u32) -> NativeProgram {
    let mut words = vec![
        0xBE85_0380, // s_mov_b32 s5, 0
        0xF408_0202,
        0xFA00_0000, // s_load_dwordx4 s[8:11], s[4:5], 0
    ];
    words.extend_from_slice(body);
    words.push(0xBF81_0000);
    NativeProgram {
        stage: HwStage::Cs,
        wave64: true,
        vgprs: 16,
        layout: vec![
            RootParam {
                root: RootType::Constants,
                table_offset_dwords: 0,
                sgprs: 4,
                base_register: 0,
                user_data: 0,
                slot: 0,
            },
            RootParam {
                root: RootType::Table,
                table_offset_dwords: 0,
                sgprs: 1,
                base_register: 0,
                user_data: 4,
                slot: 1,
            },
        ],
        used: vec![UsedResource {
            kind: ResourceType::Uav,
            root: RootType::Table,
            table_offset_dwords: 0,
            lower_bound: 0,
            stride: 0,
            dwords: 4,
            user_data: 4,
            slot: 1,
        }],
        io: StageIo::Cs(ComputeIo {
            rsrc2: 5 << 1 | 1 << 15,
            threads: [threads, 1, 1],
        }),
        code: words.iter().flat_map(|w| w.to_le_bytes()).collect(),
    }
}

/// A finite native scalar loop, containing an EXEC skip with a LIVE scalar
/// write. Each enabled lane adds the scalar to the same LDS counter.
fn scalar_loop() -> NativeProgram {
    synthetic(
        &[
            0xBE86_0391, // s_mov_b32 s6, 17
            0xBE87_0380, // s_mov_b32 s7, 0
            0x3402_0082, // v_lshlrev_b32 v1, 2, v0
            0x7E04_0280, // v_mov_b32 v2, 0
            0xBE8C_047E, // s_mov_b64 s[12:13], exec
            0x7DA4_0080, // v_cmpx_eq_u32 exec, 0, v0
            0xD834_0000,
            0x0000_0202, // ds_write_b32 v2, v2
            0xBEFE_040C, // s_mov_b64 exec, s[12:13]
            // loop:
            0xBE8C_047E, // s_mov_b64 s[12:13], exec
            0x7DA8_0000, // v_cmpx_gt_u32 exec, s0, v0
            0xBF88_0004, // s_cbranch_execz join
            0x8106_8106, // s_add_i32 s6, s6, 1
            0x7E08_0206, // v_mov_b32 v4, s6
            0xD880_0000,
            0x0500_0402, // ds_add_rtn_u32 v5, v2, v4
            // join:
            0xBEFE_040C, // s_mov_b64 exec, s[12:13]
            0x8107_8107, // s_add_i32 s7, s7, 1
            0xBF06_0107, // s_cmp_eq_u32 s7, s1
            0xBF84_FFF6, // s_cbranch_scc0 loop
            0xD8D8_0000,
            0x0600_0002, // ds_read_b32 v6, v2
            0xE070_1000,
            0x8002_0601, // buffer_store_dword v6, v1, s[8:11], 0 offen
            0x7E08_0206, // v_mov_b32 v4, s6
            0xE070_1100,
            0x8002_0401, // buffer_store_dword v4, v1, s[8:11], 0 offen offset:256
        ],
        64,
    )
}

/// Only console lane s0 writes, then the complementary EXEC reads. The
/// producer and consumers can be in different host halves. The output slot
/// of the producer retains the sentinel because its read is disabled.
fn complementary() -> NativeProgram {
    synthetic(
        &[
            0x3402_0082, // v_lshlrev_b32 v1, 2, v0
            0x7E04_0280, // v_mov_b32 v2, 0
            0x7E0C_02FF,
            0xA5A5_5A5A, // v_mov_b32 v6, sentinel
            0xBE8C_047E, // s_mov_b64 s[12:13], exec
            0x7DA4_0000, // v_cmpx_eq_u32 exec, s0, v0
            0x7E08_0201, // v_mov_b32 v4, s1
            0xD834_0000,
            0x0000_0402, // ds_write_b32 v2, v4
            0x8AFE_7E0C, // s_andn2_b64 exec, s[12:13], exec
            0xD8D8_0000,
            0x0600_0002, // ds_read_b32 v6, v2
            0xBEFE_040C, // s_mov_b64 exec, s[12:13]
            0xE070_1000,
            0x8002_0601, // buffer_store_dword v6, v1, s[8:11], 0 offen
        ],
        64,
    )
}

/// Lanes leave EXEC one at a time. The lower half finishes 32 iterations
/// before the upper half: a per-host-wave loop exit would deadlock the LDS
/// rendezvous. All threads must retain the scalar trip count of the last
/// enabled console lane, not their own lane's trip count.
fn mask_loop() -> NativeProgram {
    synthetic(
        &[
            0xBE86_0380, // s_mov_b32 s6, 0
            0x3402_0082, // v_lshlrev_b32 v1, 2, v0
            0x7E04_0280, // v_mov_b32 v2, 0
            0xBE8C_047E, // s_mov_b64 s[12:13], exec
            0x7DA4_0080, // v_cmpx_eq_u32 exec, 0, v0
            0xD834_0000,
            0x0000_0202, // ds_write_b32 v2, v2
            0xBEFE_040C, // s_mov_b64 exec, s[12:13]
            0x4A0E_0081, // v_add_nc_u32 v7, 1, v0
            0x7DA8_0000, // v_cmpx_gt_u32 exec, s0, v0
            0xBF88_0007, // s_cbranch_execz restore
            // loop:
            0x8106_8106, // s_add_i32 s6, s6, 1
            0x7E08_0281, // v_mov_b32 v4, 1
            0xD880_0000,
            0x0500_0402, // ds_add_rtn_u32 v5, v2, v4
            0x4E0E_0E81, // v_subrev_nc_u32 v7, 1, v7
            0x7DAA_0E80, // v_cmpx_ne_u32 exec, 0, v7
            0xBF89_FFF9, // s_cbranch_execnz loop
            // restore:
            0xBEFE_040C, // s_mov_b64 exec, s[12:13]
            0xD8D8_0000,
            0x0600_0002, // ds_read_b32 v6, v2
            0xE070_1000,
            0x8002_0601, // buffer_store_dword v6, v1, s[8:11], 0 offen
            0x7E08_0206, // v_mov_b32 v4, s6
            0xE070_1100,
            0x8002_0401, // buffer_store_dword v4, v1, s[8:11], 0 offen offset:256
        ],
        64,
    )
}

fn nested_scalar_skips() -> NativeProgram {
    synthetic(
        &[
            0x3402_0082, // v_lshlrev_b32 v1, 2, v0
            0xBE86_0391, // s_mov_b32 s6, 17
            0xBE8C_047E, // s_mov_b64 s[12:13], exec
            0x7DA8_0000, // v_cmpx_gt_u32 exec, s0, v0
            0xBF88_0002, // s_cbranch_execz join
            0xBF88_0001, // s_cbranch_execz join (initially seems always false)
            0xBE86_0392, // s_mov_b32 s6, 18
            0xBEFE_040C, // s_mov_b64 exec, s[12:13]
            0x7E08_0206, // v_mov_b32 v4, s6
            0xE070_1000,
            0x8002_0401, // buffer_store_dword v4, v1, s[8:11], 0 offen
        ],
        64,
    )
}

fn lowered(native: &NativeProgram) -> (super::super::HostShader, Program, Structured) {
    let original = crate::ir::build(&native.code, true, native.vgprs).unwrap();
    let (p, st) = one_wave(native, &original).unwrap();
    check_ssa(&p);
    let io = io::raise(&p, native).unwrap();
    let res = resource::track(&p, &native.layout, native.stage, Some(&st)).unwrap();
    let bytes = crate::dxbc::build(&[]);
    let c = Container::parse(&bytes).unwrap();
    let was = super::super::lower::TEST_HOST_WAVE.replace(Some(32));
    let result = super::super::emit_program(&c, native, &p, &st, &io, &res);
    super::super::lower::TEST_HOST_WAVE.set(was);
    let (shader, wave) = result.unwrap();
    assert_eq!(wave, Some(32));
    (shader, p, st)
}

fn check_ssa(p: &Program) {
    use std::collections::HashMap;
    let order = crate::ir::reverse_postorder(&p.blocks);
    let mut index = vec![usize::MAX; p.blocks.len()];
    for (n, &b) in order.iter().enumerate() {
        index[b] = n;
    }
    let idoms = crate::ir::idoms(&p.blocks, &order, &index);
    let mut defs = HashMap::new();
    for &(_, value) in &p.inputs {
        assert!(defs.insert(value, None).is_none());
    }
    for &b in &order {
        for phi in &p.blocks[b].phis {
            assert_eq!(phi.args.len(), p.blocks[b].preds.len());
            assert!(defs.insert(phi.value, Some((b, 0))).is_none());
        }
        for (i, inst) in p.blocks[b].insts.iter().enumerate() {
            for &(_, value) in &inst.writes {
                assert!(defs.insert(value, Some((b, i + 1))).is_none());
            }
        }
    }
    let check = |value: crate::ir::Value, at: usize, position: usize| {
        let Some((definition, before)) = defs[&value] else {
            return;
        };
        if definition == at {
            assert!(
                before < position,
                "{value:?} does not precede its use in block {at}"
            );
        } else {
            let mut ancestor = at;
            while ancestor != definition && idoms[ancestor] != ancestor {
                ancestor = idoms[ancestor];
            }
            assert_eq!(
                ancestor, definition,
                "{value:?} does not dominate block {at}"
            );
        }
    };
    for &b in &order {
        for phi in &p.blocks[b].phis {
            for (&arg, &pred) in phi.args.iter().zip(&p.blocks[b].preds) {
                if index[pred] != usize::MAX {
                    check(arg, pred, usize::MAX);
                }
            }
        }
        for (i, inst) in p.blocks[b].insts.iter().enumerate() {
            for &(_, value) in &inst.reads {
                check(value, b, i + 1);
            }
        }
    }
}

#[test]
fn live_scalar_skip_keeps_native_phi_and_predicates_lds() {
    let native = scalar_loop();
    let (shader, p, st) = lowered(&native);
    assert!(
        p.wave64 && native.wave64,
        "never change the native execution width"
    );
    let skip = p
        .blocks
        .iter()
        .position(|b| {
            b.insts
                .last()
                .is_some_and(|i| i.name() == "s_cbranch_execz")
        })
        .unwrap();
    assert!(
        st.wide.contains(&skip),
        "live scalar effects forbid deleting this skip"
    );
    let Exit::Branch { taken, not_taken } = p.blocks[skip].exit else {
        panic!()
    };
    let scalar = p.blocks[taken]
        .phis
        .iter()
        .find(|phi| phi.reg == Reg::S(6))
        .expect("live scalar join");
    assert_eq!(scalar.args.len(), p.blocks[taken].preds.len());
    assert_ne!(
        scalar.args[0], scalar.args[1],
        "do not replace both paths by the body value"
    );
    let atomic = p.blocks[not_taken]
        .insts
        .iter()
        .position(|i| i.name() == "ds_add_rtn_u32")
        .unwrap();
    assert!(
        st.guards[not_taken][atomic].is_some(),
        "inactive lanes must not issue atomics"
    );
    assert_eq!(st.wide.len(), 1);
    #[cfg(windows)]
    validate(&shader);
}

#[test]
fn multi_console_wave_groups_are_rejected() {
    let mut native = scalar_loop();
    native.io = StageIo::Cs(ComputeIo {
        rsrc2: 5 << 1 | 1 << 15,
        threads: [128, 1, 1],
    });
    let p = crate::ir::build(&native.code, true, native.vgprs).unwrap();
    assert!(one_wave(&native, &p).is_err());
}

#[test]
fn complementary_masks_keep_memory_effects_guarded() {
    let (shader, p, st) = lowered(&complementary());
    assert!(st.wide.is_empty());
    let guards = &st.guards;
    let memory: Vec<_> = p
        .blocks
        .iter()
        .enumerate()
        .flat_map(|(b, block)| {
            block
                .insts
                .iter()
                .enumerate()
                .filter(|(_, i)| i.name().starts_with("ds_"))
                .map(move |(i, _)| guards[b][i])
        })
        .collect();
    assert_eq!(memory.len(), 2);
    assert!(memory.iter().all(Option::is_some));
    #[cfg(windows)]
    validate(&shader);
}

#[test]
fn mask_loop_exit_converges_both_halves_and_preserves_scalar_iteration_ssa() {
    let (shader, p, st) = lowered(&mask_loop());
    let back = p
        .blocks
        .iter()
        .position(|b| {
            b.insts
                .last()
                .is_some_and(|i| i.name() == "s_cbranch_execnz")
        })
        .unwrap();
    assert!(st.wide.contains(&back));
    assert!(
        p.blocks
            .iter()
            .flat_map(|b| &b.phis)
            .any(|phi| phi.reg == Reg::S(6))
    );
    #[cfg(windows)]
    validate(&shader);
}

#[test]
fn convergence_revisits_branches_after_path_knowledge_changes() {
    let native = nested_scalar_skips();
    let original = crate::ir::build(&native.code, true, native.vgprs).unwrap();
    let st = structure::structure(&original).unwrap();
    let initially_nonuniform: HashSet<_> = st
        .exits
        .iter()
        .enumerate()
        .filter_map(|(b, e)| match *e {
            LaneExit::Branch { when, .. }
                if st
                    .bools
                    .support(when)
                    .iter()
                    .any(|a| !matches!(a, Atom::Uniform(_))) =>
            {
                Some(b)
            }
            _ => None,
        })
        .collect();
    assert_eq!(initially_nonuniform.len(), 1);
    let (shader, p, converged) = lowered(&native);
    assert_eq!(
        converged.wide.len(),
        2,
        "one inspection is not a convergence proof"
    );
    assert_eq!(
        p.blocks
            .iter()
            .filter(|b| matches!(b.exit, Exit::Branch { .. }))
            .count(),
        2,
        "neither live scalar skip can be removed"
    );
    #[cfg(windows)]
    validate(&shader);
}

#[cfg(windows)]
fn validate(shader: &super::super::HostShader) {
    let Ok(mut validator) = super::super::validator::Validator::load(
        super::super::validator::Validator::default_path().as_deref(),
    ) else {
        eprintln!("no dxil.dll; portable structural assertions still run");
        return;
    };
    validator.validate(&mut shader.dxil.clone()).unwrap();
}

#[test]
#[cfg(windows)]
#[ignore = "explicit RTX4050 dispatch; no title/cache access"]
fn scalar_loop_and_cross_half_lds_execute_on_rtx4050() {
    use super::super::host::{Dispatch, Root, View};
    let host = rtx4050();
    for (kind, native) in [
        ("loop", scalar_loop()),
        ("complement", complementary()),
        ("mask-loop", mask_loop()),
        ("nested", nested_scalar_skips()),
    ] {
        let (shader, _, _) = lowered(&native);
        let mut validator = super::super::validator::Validator::load(
            super::super::validator::Validator::default_path().as_deref(),
        )
        .expect("GPU proof requires DXIL validation");
        validator.validate(&mut shader.dxil.clone()).unwrap();
        let (root, pso) = host.compute_pipeline(&shader).unwrap();
        let cases: Vec<[u32; 2]> = if kind == "loop" {
            [0, 1, 31, 32, 33, 63, 64]
                .into_iter()
                .flat_map(|n| [1, 3].map(|trip| [n, trip]))
                .collect()
        } else if kind == "complement" {
            [0, 31, 32, 63]
                .map(|producer| [producer, 0x1379_2468 + producer])
                .to_vec()
        } else {
            [0, 1, 31, 32, 33, 63, 64].map(|n| [n, 0]).to_vec()
        };
        for [a, b] in cases {
            let mut user = vec![0; shader.user_data_dwords().max(1) as usize];
            for binding in &shader.user_data {
                if binding.root == RootType::Constants && binding.slot == 0 && binding.dword < 2 {
                    user[binding.index as usize] = [a, b][binding.dword as usize];
                }
            }
            let result = host
                .dispatch(
                    &root,
                    &pso,
                    &Dispatch {
                        roots: vec![(0, Root::Constants(user))],
                        views: &[View::RawUav(128)],
                        samplers: &[],
                        groups: [1, 1, 1],
                    },
                )
                .unwrap();
            for lane in 0..64 {
                let expected = if kind == "loop" {
                    a * (17 * b + b * (b + 1) / 2)
                } else if kind == "mask-loop" {
                    a * (a + 1) / 2
                } else if kind == "nested" {
                    if a == 0 { 17 } else { 18 }
                } else if lane as u32 == a {
                    0xA5A5_5A5A
                } else {
                    b
                };
                assert_eq!(
                    result[0][lane], expected,
                    "{kind}, args=({a},{b}), lane {lane}"
                );
                if kind == "loop" {
                    assert_eq!(
                        result[0][lane + 64],
                        17 + if a == 0 { 0 } else { b },
                        "scalar PHI {kind}, args=({a},{b}), lane {lane}"
                    );
                } else if kind == "mask-loop" {
                    assert_eq!(
                        result[0][lane + 64],
                        a,
                        "all lanes retain the full console-wave trip count"
                    );
                }
            }
            eprintln!("{kind}: args=({a},{b}) all 64 lanes exact");
        }
    }
}

#[cfg(windows)]
const PAIRED_MASK_CASES: [[[u32; 4]; 2]; 4] = {
    let full = u32::MAX;
    [
        [[0, 0, full, full], [0, 1 << 31, full, full]],
        [[full, full, full, full], [1, 0, full, full]],
        [[full, 0xff, 0xf00, full], [0xf000, full, full, 0x9000_0001]],
        [[0x8000_0000, 1, full, 0], [full, full, 0, 0]],
    ]
};

/// A 64-lane scalar decision, popcount, and first-lane read using two virtual
/// lane banks. Unlike the LDS rendezvous test, the two native waves run
/// independently; this fixture does not exercise the production translator.
#[test]
#[cfg(windows)]
#[ignore = "explicit RTX4050 dispatch; requires Windows SDK DXC"]
fn paired_lane_banks_keep_independent_wave64_votes_on_rtx4050() {
    use super::super::{
        HostShader, UserData,
        host::{Dispatch, Root, View},
        validator::Validator,
    };
    let dll = Validator::default_path().expect("Windows SDK dxil.dll");
    let dxc = std::env::var_os("VICTORY_PC_DXC")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| dll.with_file_name("dxc.exe"));
    let mut out = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map_or_else(std::env::temp_dir, |p| p.join("Temp").join("opencode"));
    std::fs::create_dir_all(&out).unwrap();
    out.push(format!("paired-lanes-wave32-{}.dxil", std::process::id()));
    let result = std::process::Command::new(dxc)
        .args(["-T", "cs_6_6", "-E", "main", "-Fo"])
        .arg(&out)
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/paired_lanes_wave32.hlsl"
        ))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "DXC: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut shader = HostShader {
        stage: HwStage::Cs,
        dxil: std::fs::read(&out).unwrap(),
        user_data: (0..8)
            .map(|index| UserData {
                index,
                slot: 0,
                root: RootType::Constants,
                dword: index,
            })
            .collect(),
        samplers: vec![],
        static_samplers: vec![],
        root_cbvs: vec![],
        root_srvs: vec![],
        root_uavs: vec![],
        root_indexed_views: vec![],
    };
    Validator::load(Some(&dll))
        .unwrap()
        .validate(&mut shader.dxil)
        .unwrap();
    let host = rtx4050();
    let (root, pso) = host.compute_pipeline(&shader).unwrap();
    for masks in PAIRED_MASK_CASES {
        let result = host
            .dispatch(
                &root,
                &pso,
                &Dispatch {
                    roots: vec![(0, Root::Constants(masks.into_iter().flatten().collect()))],
                    views: &[View::RawUav(4 * 128 * 12)],
                    samplers: &[],
                    groups: [2, 2, 1],
                },
            )
            .unwrap();
        for group in 0..4u32 {
            for (wave, [exec_lo, exec_hi, pred_lo, pred_hi]) in masks.into_iter().enumerate() {
                let low = exec_lo & pred_lo;
                let high = exec_hi & pred_hi;
                let linear_base = (wave * 64) as u32;
                let mut lanes = std::array::from_fn::<_, 64, _>(|i| {
                    1024 * group + linear_base + i as u32 + 1
                });
                let write_lane = if wave == 0 { 63 } else { 31 };
                let scalar = 0xBADC_0000 + group * 4 + wave as u32;
                lanes[write_lane] = scalar;
                let initial_mask = u64::from(low) | u64::from(high) << 32;
                let mut mask = initial_mask;
                let iterations = mask.count_ones();
                let mut checksum = 0u32;
                while mask != 0 {
                    let count = mask.count_ones();
                    let leader = mask.trailing_zeros() as usize;
                    let selected = lanes[leader];
                    for (i, value) in lanes.iter_mut().enumerate() {
                        if mask & (1u64 << i) != 0 {
                            *value = value.wrapping_add(selected ^ count);
                        }
                    }
                    checksum = checksum.wrapping_add(selected.wrapping_add(count * 257));
                    mask &= mask - 1;
                }
                for (i, value) in lanes.into_iter().enumerate() {
                    let local = linear_base + i as u32;
                    let at = ((group * 128 + local) * 12) as usize;
                    let prefix = (initial_mask & ((1u64 << i) - 1)).count_ones();
                    assert_eq!(
                        &result[0][at..at + 12],
                        &[group, local, low, high, iterations, checksum, value, 32,
                            prefix, scalar, write_lane as u32, 0],
                        "masks={masks:?}, native group {group}, wave {wave}, lane {i}",
                    );
                }
            }
        }
        eprintln!("512 virtual lanes matched native wave64 reference for {masks:?}");
    }
}

#[test]
#[cfg(windows)]
#[ignore = "requires original containers in VICTORY_WAVE32_FIXTURES"]
fn original_one_wave_convergence_probe() {
    let dir = std::env::var_os("VICTORY_WAVE32_FIXTURES").unwrap();
    let paths: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|p| p.unwrap().path())
        .collect();
    for hash in ["369cbe906ea72764", "2cc4c197ee29e291"] {
        let path = paths
            .iter()
            .find(|p| p.to_string_lossy().contains(hash))
            .unwrap();
        let bytes = std::fs::read(path).unwrap();
        let c = Container::parse(&bytes).unwrap();
        let native = crate::native_programs(c.part(FourCC::DXIL).unwrap().data)
            .unwrap()
            .into_iter()
            .find(|n| n.stage == HwStage::Cs)
            .unwrap();
        let p = crate::spill::build(&native).unwrap();
        let (p, st) = one_wave(&native, &p).unwrap();
        check_ssa(&p);
        let mut branches: Vec<_> = st
            .wide
            .iter()
            .map(|&b| p.blocks[b].insts.last().unwrap().machine.offset)
            .collect();
        branches.sort_unstable();
        eprintln!(
            "{hash}: {} branches converged, {} predicated instructions",
            st.wide.len(),
            st.guards.iter().flatten().filter(|g| g.is_some()).count()
        );
        eprintln!("{hash}: converged branch ISA offsets {branches:x?}");
        let io = io::raise(&p, &native).unwrap();
        let res = resource::track(&p, &native.layout, native.stage, Some(&st)).unwrap();
        let was = super::super::lower::TEST_HOST_WAVE.replace(Some(32));
        let result = super::super::emit_program(&c, &native, &p, &st, &io, &res);
        super::super::lower::TEST_HOST_WAVE.set(was);
        let (shader, wave) = result.unwrap();
        assert_eq!(wave, Some(32));
        let mut validator = super::super::validator::Validator::load(
            super::super::validator::Validator::default_path().as_deref(),
        )
        .unwrap();
        validator.validate(&mut shader.dxil.clone()).unwrap();
        let host = rtx4050();
        host.compute_pipeline(&shader).unwrap();
        eprintln!("{hash}: DXIL validated; hardware compute PSO created");
        if let Some(dir) = std::env::var_os("VICTORY_CONVERGE_OUTPUT") {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                std::path::Path::new(&dir).join(format!("{hash}.dxil")),
                &shader.dxil,
            )
            .unwrap();
        }
    }
}

#[cfg(windows)]
fn rtx4050() -> super::super::host::Host {
    use windows::Win32::Graphics::{Direct3D::*, Direct3D12::*, Dxgi::*};
    unsafe {
        let factory: IDXGIFactory4 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)).unwrap();
        for n in 0..32 {
            let Ok(adapter) = factory.EnumAdapters1(n) else {
                break;
            };
            let desc = adapter.GetDesc1().unwrap();
            let name = String::from_utf16_lossy(&desc.Description)
                .trim_end_matches('\0')
                .to_owned();
            if desc.VendorId != 0x10de || !name.contains("4050") {
                continue;
            }
            let mut device: Option<ID3D12Device> = None;
            D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_12_0, &mut device).unwrap();
            let device = device.unwrap();
            let mut options = D3D12_FEATURE_DATA_D3D12_OPTIONS1::default();
            device
                .CheckFeatureSupport(
                    D3D12_FEATURE_D3D12_OPTIONS1,
                    (&raw mut options).cast(),
                    std::mem::size_of_val(&options) as u32,
                )
                .unwrap();
            eprintln!(
                "hardware: {name}; vendor={:#06x} device={:#06x}; WaveLaneCountMin={} Max={}",
                desc.VendorId, desc.DeviceId, options.WaveLaneCountMin, options.WaveLaneCountMax
            );
            assert_eq!(
                options.WaveLaneCountMax, 32,
                "this proof requires an actual max32 device"
            );
            return super::super::host::Host { device };
        }
    }
    panic!("RTX4050 hardware required");
}
