// Tessera shape shaders. Everything is analytic SDF, so edges are crisp at
// any scale without MSAA. Coordinates are logical pixels; `g.scale` converts
// to physical pixels for anti-aliasing width. Output is premultiplied alpha.

struct Globals {
    size: vec2<f32>,
    scale: f32,
    time: f32,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var<storage, read> verts: array<vec2<f32>>;

fn to_ndc(p: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(p.x / g.size.x * 2.0 - 1.0, 1.0 - p.y / g.size.y * 2.0, 0.0, 1.0);
}

fn corner(vi: u32) -> vec2<f32> {
    return vec2<f32>(f32(vi & 1u), f32((vi >> 1u) & 1u));
}

fn clipped(p: vec2<f32>, clip: vec4<f32>) -> bool {
    return p.x < clip.x || p.y < clip.y || p.x > clip.z || p.y > clip.w;
}

// ---------------------------------------------------------------- quads ---

struct QuadIn {
    @location(0) rect: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) color2: vec4<f32>,
    @location(3) border: vec4<f32>,
    // radius, border width, softness, mode (0 h-gradient, 1 stripes, 2 v-gradient)
    @location(4) params: vec4<f32>,
    @location(5) clip: vec4<f32>,
};

struct QuadOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) p: vec2<f32>,
    @location(1) @interpolate(flat) rect: vec4<f32>,
    @location(2) @interpolate(flat) color: vec4<f32>,
    @location(3) @interpolate(flat) color2: vec4<f32>,
    @location(4) @interpolate(flat) border: vec4<f32>,
    @location(5) @interpolate(flat) params: vec4<f32>,
    @location(6) @interpolate(flat) clip: vec4<f32>,
};

@vertex
fn vs_quad(@builtin(vertex_index) vi: u32, q: QuadIn) -> QuadOut {
    let pad = q.params.z + 2.0;
    let p = q.rect.xy - vec2<f32>(pad) + corner(vi) * (q.rect.zw + vec2<f32>(2.0 * pad));
    var o: QuadOut;
    o.pos = to_ndc(p);
    o.p = p;
    o.rect = q.rect;
    o.color = q.color;
    o.color2 = q.color2;
    o.border = q.border;
    o.params = q.params;
    o.clip = q.clip;
    return o;
}

fn sd_round_box(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

fn premul(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(c.rgb * c.a, c.a);
}

@fragment
fn fs_quad(i: QuadOut) -> @location(0) vec4<f32> {
    if clipped(i.p, i.clip) {
        discard;
    }
    let half = i.rect.zw * 0.5;
    let r = min(i.params.x, min(half.x, half.y));
    let d = sd_round_box(i.p - (i.rect.xy + half), half, r);
    let aa = 0.7 / g.scale;
    let soft = max(i.params.z, aa);
    let cov = 1.0 - smoothstep(-soft, soft, d);
    if cov <= 0.0 {
        discard;
    }
    let uv = clamp((i.p - i.rect.xy) / max(i.rect.zw, vec2<f32>(0.001)), vec2<f32>(0.0), vec2<f32>(1.0));
    var c = i.color;
    let mode = i.params.w;
    if mode < 0.5 {
        c = mix(i.color, i.color2, uv.x);
    } else if mode < 1.5 {
        let s = fract((i.p.x + i.p.y) / 7.0);
        c = select(i.color, i.color2, s < 0.45);
    } else {
        c = mix(i.color, i.color2, uv.y);
    }
    var o = premul(c);
    let bw = i.params.y;
    if bw > 0.0 {
        let k = smoothstep(-aa, aa, d + bw);
        let b = premul(i.border) * k;
        o = b + o * (1.0 - b.a);
    }
    return o * cov;
}

// ---------------------------------------------------------------- cells ---

struct CellIn {
    @location(0) bbox: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) accent: vec4<f32>,
    @location(3) range: vec2<u32>,
    // corner radius, flags, rim light, opacity
    @location(4) params: vec4<f32>,
    @location(5) clip: vec4<f32>,
};

struct CellOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) p: vec2<f32>,
    @location(1) @interpolate(flat) color: vec4<f32>,
    @location(2) @interpolate(flat) accent: vec4<f32>,
    @location(3) @interpolate(flat) range: vec2<u32>,
    @location(4) @interpolate(flat) params: vec4<f32>,
    @location(5) @interpolate(flat) clip: vec4<f32>,
    @location(6) @interpolate(flat) bbox: vec4<f32>,
};

const F_HOVER: u32 = 1u;
const F_SELECTED: u32 = 2u;
const F_MARKED: u32 = 4u;
const F_COVERED: u32 = 8u;
const F_SCOPE: u32 = 16u;
const F_OTHERS: u32 = 32u;

@vertex
fn vs_cell(@builtin(vertex_index) vi: u32, c: CellIn) -> CellOut {
    let pad = c.params.x + 2.0;
    let lo = c.bbox.xy - vec2<f32>(pad);
    let hi = c.bbox.zw + vec2<f32>(pad);
    let p = lo + corner(vi) * (hi - lo);
    var o: CellOut;
    o.pos = to_ndc(p);
    o.p = p;
    o.color = c.color;
    o.accent = c.accent;
    o.range = c.range;
    o.params = c.params;
    o.clip = c.clip;
    o.bbox = c.bbox;
    return o;
}

@fragment
fn fs_cell(i: CellOut) -> @location(0) vec4<f32> {
    if clipped(i.p, i.clip) {
        discard;
    }
    // Exact signed distance to a convex polygon (positive-area winding).
    let start = i.range.x;
    let n = i.range.y;
    var d2 = 1e30;
    var qn = vec2<f32>(0.0, 1.0);
    var inside = true;
    var a = verts[start + n - 1u];
    for (var k = 0u; k < n; k = k + 1u) {
        let b = verts[start + k];
        let e = b - a;
        let w = i.p - a;
        let t = clamp(dot(w, e) / max(dot(e, e), 1e-6), 0.0, 1.0);
        let q = w - e * t;
        let qq = dot(q, q);
        if qq < d2 {
            d2 = qq;
            qn = q;
        }
        if e.x * w.y - e.y * w.x < 0.0 {
            inside = false;
        }
        a = b;
    }
    let dist = sqrt(d2);
    let d = select(1.0, -1.0, inside) * dist - i.params.x;
    let aa = 0.7 / g.scale;
    let cov = 1.0 - smoothstep(-aa, aa, d);
    if cov <= 0.0 {
        discard;
    }
    // Outward normal of the nearest edge (radial around rounded corners).
    let nrm = select(qn, -qn, inside) / max(dist, 1e-4);
    let flags = u32(i.params.y + 0.5);
    let light = i.params.z;
    var rgb = i.color.rgb;
    // Lit from above: a gentle top-to-bottom falloff across the cell, and a
    // faint cushion toward the rim so cells read as soft pebbles.
    let v = clamp((i.p.y - i.bbox.y) / max(i.bbox.w - i.bbox.y, 1.0), 0.0, 1.0);
    rgb = rgb * (1.045 - 0.09 * v);
    let inner = smoothstep(0.0, 1.0, clamp(-d / 22.0, 0.0, 1.0));
    rgb = rgb * (0.90 + 0.10 * inner);
    // Bevel: rims facing up catch the light, rims facing down sit in shade.
    let rim = 1.0 - smoothstep(0.0, 1.4, -d);
    let up = -nrm.y;
    rgb = mix(rgb, vec3<f32>(1.0), rim * max(up, 0.0) * 0.20 * light);
    rgb = rgb * (1.0 - rim * max(-up, 0.0) * 0.22 * light);
    // Small items: a fine dot grid, "many things too small to draw".
    if (flags & F_OTHERS) != 0u {
        let gp = i.p - floor(i.p / 6.0) * 6.0 - vec2<f32>(3.0);
        let dotm = 1.0 - smoothstep(0.85 - aa, 0.85 + aa, length(gp));
        rgb = mix(rgb, vec3<f32>(1.0), dotm * 0.13);
    }
    if (flags & (F_MARKED | F_COVERED)) != 0u {
        let s = step(0.5, fract((i.p.x - i.p.y) / 7.0));
        let k = select(0.38, 0.62, (flags & F_MARKED) != 0u);
        rgb = mix(rgb, i.accent.rgb, 0.30);
        rgb = mix(rgb, i.accent.rgb, s * k);
    }
    if (flags & F_HOVER) != 0u {
        rgb = mix(rgb, vec3<f32>(1.0), 0.10);
    }
    // Scope: the folder a click would enter. Lifted, with a bright edge.
    if (flags & F_SCOPE) != 0u {
        rgb = mix(rgb, vec3<f32>(1.0), 0.05);
        let edge = 1.0 - smoothstep(0.0, 1.0 + 2.0 * aa, -d);
        rgb = mix(rgb, vec3<f32>(1.0), edge * 0.38);
    }
    if (flags & F_SELECTED) != 0u {
        let ring = 1.0 - smoothstep(0.7, 0.7 + 2.0 * aa, abs(d + 1.6));
        rgb = mix(rgb, vec3<f32>(1.0), ring * 0.95);
    }
    let alpha = i.color.a * i.params.w * cov;
    return vec4<f32>(rgb * alpha, alpha);
}
