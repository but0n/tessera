//! GPU renderer: two instanced SDF pipelines (quads, convex cells) plus
//! glyphon text, drawn in a fixed number of layers (ADR-0003).

pub mod icons;
pub mod text;

use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::ui::Frame;
use text::{Fonts, TextSystem};

pub const LAYERS: usize = 3;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Quad {
    pub rect: [f32; 4],
    pub color: [f32; 4],
    pub color2: [f32; 4],
    pub border: [f32; 4],
    /// radius, border width, softness, mode
    pub params: [f32; 4],
    pub clip: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Cell {
    pub bbox: [f32; 4],
    pub color: [f32; 4],
    pub accent: [f32; 4],
    /// start, count into the shared vertex storage buffer
    pub range: [u32; 2],
    /// radius, flags, rim light, opacity
    pub params: [f32; 4],
    pub clip: [f32; 4],
}

pub mod cell_flags {
    pub const HOVER: u32 = 1;
    pub const SELECTED: u32 = 2;
    pub const MARKED: u32 = 4;
    pub const COVERED: u32 = 8;
    pub const SCOPE: u32 = 16;
    /// "Small items" aggregate: dot texture.
    pub const OTHERS: u32 = 32;
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Globals {
    size: [f32; 2],
    scale: f32,
    time: f32,
}

/// A growable GPU buffer.
struct DynBuf {
    buf: wgpu::Buffer,
    cap: u64,
    usage: wgpu::BufferUsages,
    label: &'static str,
}

impl DynBuf {
    fn new(device: &wgpu::Device, label: &'static str, usage: wgpu::BufferUsages) -> Self {
        let cap = 64 * 1024;
        let buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: cap, usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        DynBuf { buf, cap, usage, label }
    }

    /// Uploads `data`; returns true if the buffer was reallocated.
    fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8]) -> bool {
        let mut grew = false;
        if data.len() as u64 > self.cap {
            self.cap = (data.len() as u64).next_power_of_two();
            self.buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size: self.cap,
                usage: self.usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            grew = true;
        }
        if !data.is_empty() {
            queue.write_buffer(&self.buf, 0, data);
        }
        grew
    }
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    globals: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    quad_pipeline: wgpu::RenderPipeline,
    cell_pipeline: wgpu::RenderPipeline,
    quads: DynBuf,
    cells: DynBuf,
    verts: DynBuf,
    pub text: TextSystem,
}

impl Renderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, format: wgpu::TextureFormat, fonts: Fonts) -> Renderer {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("shapes"), source: wgpu::ShaderSource::Wgsl(include_str!("shapes.wgsl").into()) });
        let globals = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("globals"),
            contents: bytemuck::bytes_of(&Globals { size: [1.0, 1.0], scale: 1.0, time: 0.0 }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shapes"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("shapes"), bind_group_layouts: &[Some(&layout)], ..Default::default() });

        let quad_attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4];
        let cell_attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Uint32x2, 4 => Float32x4, 5 => Float32x4];
        let make = |label: &str, vs: &str, fs: &str, stride: usize, attrs: &[wgpu::VertexAttribute]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    buffers: &[Some(wgpu::VertexBufferLayout { array_stride: stride as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: attrs })],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..Default::default() },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                cache: None,
                multiview_mask: None,
            })
        };
        let quad_pipeline = make("quads", "vs_quad", "fs_quad", size_of::<Quad>(), &quad_attrs);
        let cell_pipeline = make("cells", "vs_cell", "fs_cell", size_of::<Cell>(), &cell_attrs);

        let quads = DynBuf::new(&device, "quads", wgpu::BufferUsages::VERTEX);
        let cells = DynBuf::new(&device, "cells", wgpu::BufferUsages::VERTEX);
        let verts = DynBuf::new(&device, "cell verts", wgpu::BufferUsages::STORAGE);
        let bind_group = Self::bind(&device, &layout, &globals, &verts.buf);
        let text = TextSystem::new(&device, &queue, format, fonts);
        Renderer { device, queue, globals, layout, bind_group, quad_pipeline, cell_pipeline, quads, cells, verts, text }
    }

    fn bind(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, globals: &wgpu::Buffer, verts: &wgpu::Buffer) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shapes"),
            layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: verts.as_entire_binding() }],
        })
    }

    /// Encodes `frame` into `target` (physical size `phys`).
    pub fn render(&mut self, frame: &Frame, target: &wgpu::TextureView, phys: (u32, u32), clear: [f32; 4]) {
        let g = Globals { size: [frame.size.0, frame.size.1], scale: frame.scale, time: frame.time };
        self.queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&g));

        let mut quads: Vec<Quad> = Vec::new();
        let mut cells: Vec<Cell> = Vec::new();
        let mut ranges: [(std::ops::Range<u32>, std::ops::Range<u32>); LAYERS] = std::array::from_fn(|_| (0..0, 0..0));
        for (i, l) in frame.layers.iter().enumerate() {
            let q0 = quads.len() as u32;
            quads.extend_from_slice(&l.quads);
            let c0 = cells.len() as u32;
            cells.extend_from_slice(&l.cells);
            ranges[i] = (q0..quads.len() as u32, c0..cells.len() as u32);
        }
        self.quads.write(&self.device, &self.queue, bytemuck::cast_slice(&quads));
        self.cells.write(&self.device, &self.queue, bytemuck::cast_slice(&cells));
        let verts: &[[f32; 2]] = if frame.verts.is_empty() { &[[0.0, 0.0]] } else { &frame.verts };
        if self.verts.write(&self.device, &self.queue, bytemuck::cast_slice(verts)) {
            self.bind_group = Self::bind(&self.device, &self.layout, &self.globals, &self.verts.buf);
        }

        self.text.prepare(&self.device, &self.queue, frame, phys);

        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: clear[0] as f64, g: clear[1] as f64, b: clear[2] as f64, a: clear[3] as f64 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            for (i, (qr, cr)) in ranges.iter().enumerate() {
                if !qr.is_empty() {
                    pass.set_pipeline(&self.quad_pipeline);
                    pass.set_vertex_buffer(0, self.quads.buf.slice(..));
                    pass.draw(0..4, qr.clone());
                }
                if !cr.is_empty() {
                    pass.set_pipeline(&self.cell_pipeline);
                    pass.set_vertex_buffer(0, self.cells.buf.slice(..));
                    pass.draw(0..4, cr.clone());
                }
                self.text.render(i, &mut pass);
                pass.set_bind_group(0, &self.bind_group, &[]);
            }
        }
        self.queue.submit(Some(enc.finish()));
        self.text.trim();
    }
}
