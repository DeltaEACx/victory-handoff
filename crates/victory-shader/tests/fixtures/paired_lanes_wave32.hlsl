// HLSL cs_6_6 | exact dual-bank wave64 primitives on one physical wave32.
// One 64-thread physical group represents one original 128-thread group.
// Independent native waves only: no source group memory or barriers.
cbuffer Inputs : register(b0) {
    uint4 masks0; // EXEC low/high, predicate low/high for native wave 0
    uint4 masks1; // same for native wave 1
};
groupshared uint next_native_wave;

uint2 ballot_pair(uint2 mask, uint own_bit) {
    return uint2(WaveActiveBallot((mask.x & own_bit) != 0).x,
                 WaveActiveBallot((mask.y & own_bit) != 0).x);
}

uint read_lane_pair(uint low, uint high, uint index) {
    uint from_low = WaveReadLaneAt(low, index & 31);
    uint from_high = WaveReadLaneAt(high, index & 31);
    return index < 32 ? from_low : from_high;
}

[WaveSize(32)]
[numthreads(64, 1, 1)]
void main(uint3 dispatch_group : SV_GroupID, uint group_index : SV_GroupIndex) {
    // One converged initialization barrier, before either native wave can
    // diverge. Elect each wave without assuming SV_GroupIndex-to-wave order.
    if (group_index == 0) next_native_wave = 0;
    GroupMemoryBarrierWithGroupSync();
    uint native_wave = 0;
    if (WaveIsFirstLane()) InterlockedAdd(next_native_wave, 1, native_wave);
    native_wave = WaveReadLaneFirst(native_wave);
    uint native_group = dispatch_group.x + dispatch_group.y * 2;
    uint lane = WaveGetLaneIndex();
    uint local = native_wave * 64 + lane;
    uint own_bit = 1u << lane;
    uint4 masks = native_wave == 0 ? masks0 : masks1;
    uint2 active = ballot_pair(masks.xy & masks.zw, own_bit);
    uint2 initial = active;
    uint low = native_group * 1024 + local + 1;
    uint high = low + 32;
    uint write_lane = native_wave == 0 ? 63 : 31;
    uint scalar = 0xBADC0000 + native_group * 4 + native_wave;
    // v_writelane and v_readlane are scalar-addressed operations, not
    // per-bank EXEC-guarded vector arithmetic.
    if (lane == (write_lane & 31)) {
        if (write_lane < 32) low = scalar;
        else high = scalar;
    }
    uint read_back = read_lane_pair(low, high, write_lane);
    uint below = own_bit - 1;
    uint prefix_lo = countbits(initial.x & below);
    uint prefix_hi = countbits(initial.x) + countbits(initial.y & below);
    uint iterations = 0;
    uint checksum = 0;

    // Every physical lane participates. A virtual hardware EXEC mask
    // predicates a bank's updates, not the subgroup vote or shuffle.
    [loop]
    while (any(active != 0)) {
        bool low_leader = active.x != 0;
        uint leader = low_leader ? firstbitlow(active.x) : 32 + firstbitlow(active.y);
        uint selected = read_lane_pair(low, high, leader);
        uint count = countbits(active.x) + countbits(active.y);
        if (active.x & own_bit) low += selected ^ count;
        if (active.y & own_bit) high += selected ^ count;
        iterations++;
        checksum += selected + count * 257;
        if (low_leader) active.x &= active.x - 1;
        else active.y &= active.y - 1;
        active = ballot_pair(active, own_bit);
    }

    RWByteAddressBuffer output = ResourceDescriptorHeap[0];
    uint offset = (native_group * 128 + local) * 48;
    output.Store4(offset, uint4(native_group, local, initial));
    output.Store4(offset + 16, uint4(iterations, checksum, low, WaveGetLaneCount()));
    output.Store4(offset + 32, uint4(prefix_lo, read_back, write_lane, 0));
    offset += 32 * 48;
    output.Store4(offset, uint4(native_group, local + 32, initial));
    output.Store4(offset + 16, uint4(iterations, checksum, high, WaveGetLaneCount()));
    output.Store4(offset + 32, uint4(prefix_hi, read_back, write_lane, 0));
}
