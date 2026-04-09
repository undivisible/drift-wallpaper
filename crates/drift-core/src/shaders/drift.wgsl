// drift.wgsl – Drift fluid-aesthetic shader using domain-warped fractal Brownian motion.
//
// Technique: three levels of domain warping (Quilez 2003) applied to 2-D
// value-noise fractals.  The result is a slowly evolving, organic swirling
// pattern that mirrors the aesthetic of sandydoo/flux without requiring
// compute shaders or simulation state.
//
// Vertex stage: full-screen triangle drawn from a vertex_index builtin –
// no vertex buffer required.
//
// Fragment stage: computes the drift colour for each fragment.

// ---------------------------------------------------------------------------
// Uniform block
// ---------------------------------------------------------------------------

struct Uniforms {
    time:    f32,
    width:   f32,
    height:  f32,
    speed:   f32,
    scale:   f32,
    // Padding is required: vec3<f32> is laid out as 16 bytes in WGSL uniform buffers.
    color_a: vec3<f32>,
    _pad0:   f32,
    color_b: vec3<f32>,
    _pad1:   f32,
    color_c: vec3<f32>,
    _pad2:   f32,
}

@group(0) @binding(0)
var<uniform> u: Uniforms;

// ---------------------------------------------------------------------------
// Vertex stage – single large triangle that covers NDC [-1, 1]²
// ---------------------------------------------------------------------------

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    // Vertices of a single triangle that over-covers the unit square.
    //   vi=0 → (-1, -1)
    //   vi=1 → ( 3, -1)
    //   vi=2 → (-1,  3)
    let x = f32(i32(vi & 1u) * 4 - 1);
    let y = f32(i32(vi >> 1u) * 4 - 1);
    var out: VertexOutput;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    return out;
}

// ---------------------------------------------------------------------------
// Noise primitives
// ---------------------------------------------------------------------------

// Cheap integer-like hash in [0,1) from a vec2 lattice coordinate.
fn hash2(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

// Smooth value noise with bicubic-smoothed (C1) interpolation.
fn noise2d(p: vec2<f32>) -> f32 {
    let i  = floor(p);
    let f  = fract(p);
    // Smoothstep (Ken Perlin's improvement: 6t⁵ − 15t⁴ + 10t³)
    let u  = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let v00 = hash2(i + vec2<f32>(0.0, 0.0));
    let v10 = hash2(i + vec2<f32>(1.0, 0.0));
    let v01 = hash2(i + vec2<f32>(0.0, 1.0));
    let v11 = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(v00, v10, u.x), mix(v01, v11, u.x), u.y);
}

// Fractal Brownian motion: 5 octaves of value noise.
fn fbm(p: vec2<f32>) -> f32 {
    var value  = 0.0;
    var weight = 0.5;
    var pp     = p;
    for (var i = 0; i < 5; i++) {
        value  += weight * noise2d(pp);
        pp      = pp * 2.07 + vec2<f32>(3.13, 1.97);   // offset per octave
        weight *= 0.5;
    }
    return value;
}

// ---------------------------------------------------------------------------
// Fragment stage
// ---------------------------------------------------------------------------

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Normalised UV with aspect-ratio correction.
    let res   = vec2<f32>(u.width, u.height);
    let uv    = in.position.xy / res;
    let aspect = u.width / u.height;

    // Base position scaled and aspect-corrected.
    var p = (uv - 0.5) * vec2<f32>(aspect, 1.0) * u.scale * 2.5;

    let t = u.time * u.speed;

    // ── Level 0: base noise coordinates ──────────────────────────────────
    let base_offset = vec2<f32>(t * 0.07, t * 0.05);

    // ── Level 1 (q): warp p by two fbm samples ───────────────────────────
    let q = vec2<f32>(
        fbm(p + base_offset),
        fbm(p + vec2<f32>(5.2, 1.3) + base_offset * 0.8),
    );

    // ── Level 2 (r): warp by q ────────────────────────────────────────────
    let r = vec2<f32>(
        fbm(p + 4.0 * q + vec2<f32>(1.7, 9.2) + base_offset * 0.6),
        fbm(p + 4.0 * q + vec2<f32>(8.3, 2.8) + base_offset * 0.5),
    );

    // ── Level 3 (f): the final scalar field ──────────────────────────────
    let f = clamp(fbm(p + 4.0 * r + base_offset * 0.3), 0.0, 1.0);

    // ── Three-stop colour gradient ────────────────────────────────────────
    // Map f ∈ [0,1] through: color_a → color_b (lower half)
    //                         color_b → color_c (upper half)
    var color: vec3<f32>;
    if f < 0.5 {
        color = mix(u.color_a, u.color_b, smoothstep(0.0, 0.5, f));
    } else {
        color = mix(u.color_b, u.color_c, smoothstep(0.5, 1.0, f));
    }

    // Subtle vignette to darken edges.
    let vignette = 1.0 - 0.35 * dot(uv - 0.5, uv - 0.5) * 4.0;
    color *= clamp(vignette, 0.0, 1.0);

    return vec4<f32>(color, 1.0);
}
