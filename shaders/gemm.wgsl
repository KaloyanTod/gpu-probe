// Tiled GEMM: C = A * B for square N x N f32 matrices, row-major.
//
// Tile / workgroup size is 16x16 (universal — comfortably fits integrated GPUs'
// workgroup + shared-memory limits). Each workgroup computes one 16x16 tile of
// C. The point of tiling is data reuse: instead of streaming every element of A
// and B from global memory once per multiply-accumulate, we stage a 16x16 tile
// of each into workgroup-shared memory, then every thread in the workgroup
// reads those staged tiles many times from fast shared memory. This turns O(N)
// global reads per output into O(N/16), which is the whole reason a tiled GEMM
// is faster than the naive one.
//
// N is guaranteed by the host to be a positive multiple of 16, so tiles divide
// the matrix exactly and no bounds checks are needed inside the kernel.

const TILE: u32 = 16u;

struct Dims {
    n: u32,
};

@group(0) @binding(0) var<storage, read>       a: array<f32>;
@group(0) @binding(1) var<storage, read>       b: array<f32>;
@group(0) @binding(2) var<storage, read_write> c: array<f32>;
@group(0) @binding(3) var<uniform>             dims: Dims;

// Shared staging tiles, one for A and one for B. `var<workgroup>` memory is
// shared by all 16x16 invocations in the workgroup.
var<workgroup> tile_a: array<array<f32, 16>, 16>;
var<workgroup> tile_b: array<array<f32, 16>, 16>;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>,
        @builtin(local_invocation_id)  lid: vec3<u32>) {
    let n = dims.n;

    // Global row/col of C this invocation is responsible for.
    let row = gid.y;
    let col = gid.x;
    // Position of this invocation within its 16x16 tile.
    let lrow = lid.y;
    let lcol = lid.x;

    var acc: f32 = 0.0;
    let num_tiles = n / TILE;

    // March a tile-strip of A (left→right) against a tile-strip of B (top→down)
    // along the shared K dimension.
    for (var t: u32 = 0u; t < num_tiles; t = t + 1u) {
        // Cooperative load: each invocation pulls exactly one element of the A
        // tile and one of the B tile into shared memory.
        tile_a[lrow][lcol] = a[row * n + (t * TILE + lcol)];
        tile_b[lrow][lcol] = b[(t * TILE + lrow) * n + col];

        // Barrier: every load above must be visible to the whole workgroup
        // before anyone starts multiplying, or threads would read stale/unwritten
        // shared-memory slots.
        workgroupBarrier();

        // Multiply the two staged tiles, accumulating into acc.
        for (var k: u32 = 0u; k < TILE; k = k + 1u) {
            acc = acc + tile_a[lrow][k] * tile_b[k][lcol];
        }

        // Barrier again: all reads of the current tiles must finish before the
        // next iteration overwrites shared memory with the following tiles.
        workgroupBarrier();
    }

    c[row * n + col] = acc;
}
