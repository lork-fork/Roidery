//! Roidery: the realistic view on the GPU. Each stitch is drawn as a thread: a capsule as wide
//! as the thread, shaded as a twisted cylinder lit from the top left (diffuse light, a sheen
//! along its crest and the plies' twist), over a soft shadow on the fabric. Stitches are uploaded
//! once per document revision as instances; panning, zooming and the sew-out player only change
//! a few uniforms.
//!
//! The app calls [`install`] with its wgpu render state; without it (OpenGL builds, tests) the
//! realistic view falls back to the painter in [`crate::stitchview`].

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt;
use vectorcraft_engine::StitchPreview;

static AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Whether the GPU realistic view is installed.
pub fn available() -> bool {
    AVAILABLE.load(Ordering::Relaxed)
}

const SHADER: &str = r#"
struct U {
    // doc → screen points: x' = m.x*x + m.y*y + o.x, y' = m.z*x + m.w*y + o.y
    m: vec4<f32>,
    // o.xy, then the canvas rect's min corner (points)
    o_min: vec4<f32>,
    // rect size (points), half the thread width (points), mode (0 thread, 1 shadow)
    size_hw_mode: vec4<f32>,
    // shadow offset (points), shadow alpha
    shadow: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: U;

struct In {
    @location(0) a: vec2<f32>,
    @location(1) b: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) seed: f32,
};

struct Out {
    @builtin(position) pos: vec4<f32>,
    // along the stitch from its start (points), across from its centre line (points)
    @location(0) local: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) dir: vec2<f32>,
    @location(3) len_hw_seed: vec3<f32>,
};

fn to_screen(p: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(u.m.x * p.x + u.m.y * p.y + u.o_min.x, u.m.z * p.x + u.m.w * p.y + u.o_min.y);
}

@vertex
fn vs(@builtin(vertex_index) vi: u32, i: In) -> Out {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let c = corners[vi];
    let shadow = u.size_hw_mode.w > 0.5;
    var hw = u.size_hw_mode.z;
    if (shadow) { hw = hw * 1.25; }
    let a = to_screen(i.a);
    let b = to_screen(i.b);
    let d = b - a;
    let len = length(d);
    var dir = vec2<f32>(1.0, 0.0);
    if (len > 1e-4) { dir = d / len; }
    let n = vec2<f32>(-dir.y, dir.x);
    var p = mix(a, b, c.x) + dir * (c.x * 2.0 - 1.0) * hw + n * c.y * hw;
    if (shadow) { p = p + u.shadow.xy; }
    let rel = (p - u.o_min.zw) / u.size_hw_mode.xy;
    var o: Out;
    o.pos = vec4<f32>(rel.x * 2.0 - 1.0, 1.0 - rel.y * 2.0, 0.0, 1.0);
    o.local = vec2<f32>(c.x * len + (c.x * 2.0 - 1.0) * hw, c.y * hw);
    o.color = i.color;
    o.dir = dir;
    o.len_hw_seed = vec3<f32>(len, hw, i.seed);
    return o;
}

@fragment
fn fs(i: Out) -> @location(0) vec4<f32> {
    let len = i.len_hw_seed.x;
    let hw = max(i.len_hw_seed.y, 1e-3);
    // Capsule: distance from the centre segment, as a fraction of the half width.
    let q = vec2<f32>(i.local.x - clamp(i.local.x, 0.0, len), i.local.y) / hw;
    let dist = length(q);
    // Coverage from the distance to the edge in pixels, and how many pixels the thread's half
    // width spans: small on screen, the shading flattens instead of shimmering.
    let per_px = max(fwidth(dist), 1e-4);
    let edge = clamp((1.0 - dist) / per_px + 0.5, 0.0, 1.0);
    let detail = smoothstep(1.5, 5.0, 1.0 / per_px);
    if (edge <= 0.0) { discard; }
    if (u.size_hw_mode.w > 0.5) {
        let a = u.shadow.z * edge * (1.0 - smoothstep(0.2, 1.0, dist)) * detail;
        return vec4<f32>(0.0, 0.0, 0.0, a);
    }
    // The thread's surface normal: a cylinder across, rounded at the ends.
    let n2 = i.dir * q.x + vec2<f32>(-i.dir.y, i.dir.x) * q.y;
    let nz = sqrt(max(0.0, 1.0 - dot(q, q)));
    let nrm = normalize(vec3<f32>(n2, nz + 1e-4));
    let light = normalize(vec3<f32>(-0.45, -0.65, 0.62));
    let diffuse = max(dot(nrm, light), 0.0);
    let h = normalize(light + vec3<f32>(0.0, 0.0, 1.0));
    let spec = pow(max(dot(nrm, h), 0.0), 28.0) * 0.42 * detail;
    // The plies twist along the thread.
    let phase = i.local.x / (hw * 2.4) + q.y * 0.9 + i.len_hw_seed.z;
    let ply = mix(1.0, mix(0.86, 1.06, 0.5 + 0.5 * sin(phase * 6.2831853)), detail);
    let shade = mix(0.82, 0.30 + 0.85 * diffuse, detail);
    let rgb = clamp(i.color.rgb * shade * ply + vec3<f32>(spec), vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(rgb * edge, edge);
}
"#;

/// Bytes per instance: a (2 × f32), b (2 × f32), colour (4 × u8), seed (f32).
const STRIDE: u64 = 24;
const UNIFORM_BYTES: u64 = 64;

struct Resources {
    pipeline: wgpu::RenderPipeline,
    uniforms: [wgpu::Buffer; 2],
    binds: [wgpu::BindGroup; 2],
    instances: Option<wgpu::Buffer>,
    /// (document uid, revision) the instances were built from.
    key: Option<(u64, u64)>,
    /// `before[i]`: instances made from the preview's records before record `i`.
    before: Vec<u32>,
}

/// Set up the GPU realistic view on the app's wgpu device (once, at start-up).
pub fn install(rs: &egui_wgpu::RenderState) {
    let device = &rs.device;
    let shader =
        device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("roidery threads"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("roidery threads"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        }],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("roidery threads"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let blend = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    };
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("roidery threads"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: STRIDE,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Unorm8x4, 3 => Float32],
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: rs.target_format,
                blend: Some(wgpu::BlendState { color: blend, alpha: blend }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let uniform = |label| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: UNIFORM_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    };
    let uniforms = [uniform("roidery thread shadows"), uniform("roidery threads")];
    let bind = |b: &wgpu::Buffer| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("roidery threads"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: b.as_entire_binding() }],
        })
    };
    let binds = [bind(&uniforms[0]), bind(&uniforms[1])];
    rs.renderer.write().callback_resources.insert(Resources { pipeline, uniforms, binds, instances: None, key: None, before: vec![] });
    AVAILABLE.store(true, Ordering::Relaxed);
}

/// The instance bytes of `pv` (one per sewn stitch) and `before` (see [`Resources::before`]).
fn instances(pv: &StitchPreview) -> (Vec<u8>, Vec<u32>) {
    let mut bytes = Vec::with_capacity(pv.stitches.len() * STRIDE as usize);
    let mut before = Vec::with_capacity(pv.stitches.len() + 1);
    let mut count: u32 = 0;
    let mut prev: Option<(f64, f64)> = None;
    for (i, s) in pv.stitches.iter().enumerate() {
        before.push(count);
        if s.sew
            && let Some((ax, ay)) = prev
        {
            let rgb = pv.threads.get(s.thread).copied().unwrap_or([0, 0, 0]);
            for v in [ax, ay, s.x, s.y] {
                bytes.extend_from_slice(&(v as f32).to_le_bytes());
            }
            bytes.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            // A stable pseudo-random twist phase per stitch.
            let seed = ((i as u32).wrapping_mul(2_654_435_761) >> 16) as f32 / 65_536.0;
            bytes.extend_from_slice(&seed.to_le_bytes());
            count = count.saturating_add(1);
        }
        prev = Some((s.x, s.y));
    }
    before.push(count);
    (bytes, before)
}

/// The view, for one frame.
pub struct Frame {
    /// doc → screen points, as `[m00, m01, m10, m11]` and the offset.
    pub m: [f32; 4],
    pub offset: [f32; 2],
    pub rect: egui::Rect,
    /// Half the thread's width on screen, points.
    pub half_width: f32,
    /// Records sewn so far (the sew-out player), else everything.
    pub sewn: Option<usize>,
}

struct Callback {
    preview: Arc<StitchPreview>,
    key: (u64, u64),
    frame: Frame,
}

/// The paint callback that draws `preview` (of document `key`) as thread.
pub fn callback(preview: Arc<StitchPreview>, key: (u64, u64), frame: Frame) -> egui::Shape {
    let rect = frame.rect;
    egui::Shape::Callback(egui_wgpu::Callback::new_paint_callback(rect, Callback { preview, key, frame }))
}

fn uniform_bytes(f: &Frame, shadow: bool) -> Vec<u8> {
    let hw = f.half_width;
    let vals = [
        f.m[0],
        f.m[1],
        f.m[2],
        f.m[3],
        f.offset[0],
        f.offset[1],
        f.rect.min.x,
        f.rect.min.y,
        f.rect.width().max(1.0),
        f.rect.height().max(1.0),
        hw,
        if shadow { 1.0 } else { 0.0 },
        hw * 0.25,
        hw * 0.4,
        0.22,
        0.0,
    ];
    vals.iter().flat_map(|v| v.to_le_bytes()).collect()
}

impl egui_wgpu::CallbackTrait for Callback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(r) = resources.get_mut::<Resources>() else { return vec![] };
        if r.key != Some(self.key) {
            let (bytes, before) = instances(&self.preview);
            r.instances = (!bytes.is_empty()).then(|| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("roidery stitches"),
                    contents: &bytes,
                    usage: wgpu::BufferUsages::VERTEX,
                })
            });
            r.before = before;
            r.key = Some(self.key);
        }
        queue.write_buffer(&r.uniforms[0], 0, &uniform_bytes(&self.frame, true));
        queue.write_buffer(&r.uniforms[1], 0, &uniform_bytes(&self.frame, false));
        vec![]
    }

    fn paint(&self, _info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, resources: &egui_wgpu::CallbackResources) {
        let Some(r) = resources.get::<Resources>() else { return };
        let Some(buf) = &r.instances else { return };
        let total = r.before.last().copied().unwrap_or(0);
        let count = self.frame.sewn.map_or(total, |n| r.before.get(n).copied().unwrap_or(total));
        if count == 0 {
            return;
        }
        pass.set_pipeline(&r.pipeline);
        pass.set_vertex_buffer(0, buf.slice(..));
        // All the shadows first, so no thread is shaded by a later one's shadow.
        for bind in &r.binds {
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..6, 0..count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_engine::PreviewStitch;

    #[test]
    fn one_instance_per_sewn_stitch() {
        let s = |x, sew, thread| PreviewStitch { x, y: 0.0, sew, thread, underlay: false, trim: false };
        let pv = StitchPreview {
            stitches: vec![s(0.0, false, 0), s(1.0, true, 0), s(2.0, true, 0), s(9.0, false, 1), s(10.0, true, 1)],
            threads: vec![[255, 0, 0], [0, 0, 255]],
            ..Default::default()
        };
        let (bytes, before) = instances(&pv);
        assert_eq!(bytes.len(), 3 * STRIDE as usize);
        assert_eq!(before, [0, 0, 1, 2, 2, 3]);
        // The third instance: from the jump's end (9, 0) to (10, 0) in blue.
        let third = &bytes[2 * STRIDE as usize..];
        assert_eq!(f32::from_le_bytes([third[0], third[1], third[2], third[3]]), 9.0);
        assert_eq!(&third[16..20], &[0, 0, 255, 255]);
        assert_eq!(
            uniform_bytes(&Frame { m: [1.0; 4], offset: [0.0; 2], rect: egui::Rect::EVERYTHING, half_width: 1.0, sewn: None }, true).len() as u64,
            UNIFORM_BYTES
        );
    }
}
