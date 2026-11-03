// Schematics for Horizon's drawing, added to the renderer as
// `horizon_client_render::schematics_ghost`: the meshes the game side
// publishes (one per placement section, block models textured from the
// shared atlas or flat coloured boxes), drawn once the world is drawn,
// see-through and hidden behind the world's own terrain by its depth.
//
// Each mesh is uploaded once; its block origin goes in an instance buffer
// rewritten only when the set of meshes changes, so a frame writes one small
// uniform (the camera) and draws the meshes in view.

use crate::gfx::native_render::{RenderNeeds, WorldDrawn};
use crate::mesher::Vertex;
use glam::{Mat4, Vec4};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use wgpu::util::DeviceExt;

/// `Vertex::layer` of a flat coloured quad (the colour's alpha is its own).
pub const FLAT: u32 = 1;

/// Quads (4 vertices each) at block positions from `origin`.
pub struct Mesh {
    pub origin: [i32; 3],
    pub vertices: Vec<Vertex>,
}

#[derive(Default)]
struct Scene {
    /// By key: the mesh and its revision.
    meshes: HashMap<u64, (u64, Arc<Mesh>)>,
    /// Boxes drawn over everything (the selection, placement bounds).
    lines: Option<(u64, Arc<Mesh>)>,
    /// Bumped by every change above.
    generation: u64,
}

static SCENE: Mutex<Option<Scene>> = Mutex::new(None);
/// Whether there is anything to draw, for `needs` without the lock.
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// The ghosts' opacity, as `f32` bits (0.6 at first).
static OPACITY: AtomicU32 = AtomicU32::new(0x3F19_999A);

fn scene<R>(f: impl FnOnce(&mut Scene) -> R) -> R {
    let mut guard = SCENE.lock().unwrap_or_else(|e| e.into_inner());
    let s = guard.get_or_insert_with(|| Scene { generation: 1, ..Default::default() });
    let out = f(s);
    ACTIVE.store(!s.meshes.is_empty() || s.lines.is_some(), Ordering::Relaxed);
    out
}

/// Sets (or with None takes out) the mesh under `key`.
pub fn set_mesh(key: u64, rev: u64, mesh: Option<Arc<Mesh>>) {
    scene(|s| {
        let changed = match mesh {
            Some(m) if !m.vertices.is_empty() => {
                s.meshes.insert(key, (rev, m));
                true
            }
            _ => s.meshes.remove(&key).is_some(),
        };
        if changed {
            s.generation += 1;
        }
    });
}

/// Keeps only the meshes `keep` says to.
pub fn retain(mut keep: impl FnMut(u64) -> bool) {
    scene(|s| {
        let before = s.meshes.len();
        s.meshes.retain(|k, _| keep(*k));
        if s.meshes.len() != before {
            s.generation += 1;
        }
    });
}

pub fn clear() {
    if !ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    scene(|s| {
        s.meshes.clear();
        s.lines = None;
        s.generation += 1;
    });
}

pub fn set_lines(rev: u64, mesh: Option<Arc<Mesh>>) {
    scene(|s| {
        s.lines = mesh.filter(|m| !m.vertices.is_empty()).map(|m| (rev, m));
        s.generation += 1;
    });
}

pub fn set_opacity(opacity: f32) {
    OPACITY.store(opacity.clamp(0.05, 1.0).to_bits(), Ordering::Relaxed);
}

/// What the frame must keep for us: the world's depth.
pub fn needs(needs: &mut RenderNeeds) {
    if ACTIVE.load(Ordering::Relaxed) {
        needs.world_depth = true;
    }
}

const SHADER: &str = r#"
struct U {
    view_proj: mat4x4<f32>,
    // The camera's block, and its position in that block.
    cam_block: vec4<i32>,
    cam_frac: vec4<f32>,
    // x: opacity, y z: world depth pixels per target pixel.
    params: vec4<f32>,
}
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;
@group(0) @binding(3) var world_depth: texture_depth_2d;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) layer: u32,
    @location(3) @interpolate(flat) over: u32,
}

// `origin`: the mesh's block origin (xyz, exact in integers) and whether it
// draws over the world (w).
@vertex fn vs(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) layer: u32,
    @location(3) color: vec4<f32>, @location(4) origin: vec4<i32>) -> Out {
    var out: Out;
    let base = vec3<f32>(origin.xyz - u.cam_block.xyz) - u.cam_frac.xyz;
    out.clip = u.view_proj * vec4(pos + base, 1.0);
    out.uv = uv;
    out.color = color;
    out.layer = layer;
    out.over = u32(origin.w);
    return out;
}

@fragment fn fs(in: Out) -> @location(0) vec4<f32> {
    // Sampled first, in uniform control flow.
    let t = textureSample(atlas, atlas_sampler, in.uv);
    if in.over == 0u {
        let size = vec2<i32>(textureDimensions(world_depth));
        let at = clamp(vec2<i32>(in.clip.xy * u.params.yz), vec2(0), size - vec2(1));
        // Reversed depth: the world's terrain in front of this pixel hides it.
        let world = textureLoad(world_depth, at, 0);
        if in.clip.z < world * (1.0 - 2e-5) {
            discard;
        }
    }
    if in.layer == 1u {
        return in.color;
    }
    if t.a < 0.1 {
        discard;
    }
    return vec4(t.rgb * in.color.rgb, u.params.x);
}
"#;

const UNIFORM_SIZE: u64 = 112;

/// The atlas and depth the bind group was made for.
struct Bound {
    atlas: wgpu::Texture,
    depth: wgpu::TextureView,
    group: wgpu::BindGroup,
}

struct Gpu {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    ghost: wgpu::RenderPipeline,
    over: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    indices: wgpu::Buffer,
    quads: u32,
    buffers: HashMap<u64, (u64, wgpu::Buffer, u32)>,
    lines: Option<(u64, wgpu::Buffer, u32)>,
    /// The scene generation drawn, the meshes in draw order (key, origin,
    /// instance) and their origins as instances (the lines' last).
    generation: u64,
    order: Vec<(u64, [i32; 3], u32)>,
    origins: Option<wgpu::Buffer>,
    lines_instance: u32,
    bound: Option<Bound>,
    depth: Option<([u32; 2], wgpu::TextureView)>,
}

static GPU: Mutex<Option<Gpu>> = Mutex::new(None);

fn pipeline(device: &wgpu::Device, layout: &wgpu::PipelineLayout, module: &wgpu::ShaderModule, format: wgpu::TextureFormat, over: bool) -> wgpu::RenderPipeline {
    let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Uint32, 3 => Unorm8x4];
    let instance = wgpu::vertex_attr_array![4 => Sint32x4];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(if over { "Schematics boxes" } else { "Schematics ghosts" }),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[
                Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                }),
                Some(wgpu::VertexBufferLayout { array_stride: 16, step_mode: wgpu::VertexStepMode::Instance, attributes: &instance }),
            ],
        },
        primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(!over),
            depth_compare: Some(if over { wgpu::CompareFunction::Always } else { wgpu::CompareFunction::Greater }),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::Zero,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
                write_mask: wgpu::ColorWrites::COLOR,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

impl Gpu {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Schematics"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let texture = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Schematics"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(UNIFORM_SIZE),
                    },
                    count: None,
                },
                texture(1, wgpu::TextureSampleType::Float { filterable: true }),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture(3, wgpu::TextureSampleType::Depth),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Schematics"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let ghost = pipeline(device, &pipeline_layout, &module, format, false);
        let over = pipeline(device, &pipeline_layout, &module, format, true);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Schematics atlas"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Schematics uniforms"),
            size: UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            format,
            layout,
            ghost,
            over,
            sampler,
            uniforms,
            indices: index_buffer(device, 4096),
            quads: 4096,
            buffers: HashMap::new(),
            lines: None,
            generation: 0,
            order: Vec::new(),
            origins: None,
            lines_instance: 0,
            bound: None,
            depth: None,
        }
    }

    /// Uploads what changed in the scene since the last frame.
    fn sync(&mut self, device: &wgpu::Device) {
        let latest = scene(|s| s.generation);
        if latest == self.generation {
            return;
        }
        let (meshes, lines, generation) = scene(|s| {
            let meshes: Vec<(u64, u64, Arc<Mesh>)> = s.meshes.iter().map(|(&k, (rev, m))| (k, *rev, m.clone())).collect();
            (meshes, s.lines.clone(), s.generation)
        });
        self.generation = generation;
        self.buffers.retain(|k, _| meshes.iter().any(|m| m.0 == *k));
        let mut most = 0;
        self.order.clear();
        let mut origins: Vec<[i32; 4]> = Vec::with_capacity(meshes.len() + 1);
        for (key, rev, mesh) in &meshes {
            let quads = (mesh.vertices.len() / 4) as u32;
            most = most.max(quads);
            if self.buffers.get(key).is_none_or(|(r, _, _)| r != rev) {
                self.buffers.insert(*key, (*rev, vertex_buffer(device, mesh), quads));
            }
            self.order.push((*key, mesh.origin, origins.len() as u32));
            origins.push([mesh.origin[0], mesh.origin[1], mesh.origin[2], 0]);
        }
        match &lines {
            Some((rev, mesh)) => {
                let quads = (mesh.vertices.len() / 4) as u32;
                most = most.max(quads);
                if self.lines.as_ref().is_none_or(|(r, _, _)| r != rev) {
                    self.lines = Some((*rev, vertex_buffer(device, mesh), quads));
                }
                self.lines_instance = origins.len() as u32;
                origins.push([mesh.origin[0], mesh.origin[1], mesh.origin[2], 1]);
            }
            None => self.lines = None,
        }
        if most > self.quads {
            self.quads = most.next_power_of_two();
            self.indices = index_buffer(device, self.quads);
        }
        self.origins = (!origins.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Schematics origins"),
                contents: bytemuck::cast_slice(&origins),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
    }
}

fn index_buffer(device: &wgpu::Device, quads: u32) -> wgpu::Buffer {
    let indices: Vec<u32> = (0..quads).flat_map(|q| [0, 1, 2, 0, 2, 3].map(|i| q * 4 + i)).collect();
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Schematics quads"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    })
}

fn vertex_buffer(device: &wgpu::Device, mesh: &Mesh) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Schematics mesh"),
        contents: bytemuck::cast_slice(&mesh.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    })
}

/// Whether a section's box (from `min`, camera-relative, 16 blocks wide)
/// may be in view: it is out when all its corners are past one side.
fn in_view(view_proj: &Mat4, min: [f32; 3]) -> bool {
    let mut outside = [true; 5];
    for i in 0..8 {
        let corner = Vec4::new(
            min[0] + if i & 1 != 0 { 16.0 } else { 0.0 },
            min[1] + if i & 2 != 0 { 16.0 } else { 0.0 },
            min[2] + if i & 4 != 0 { 16.0 } else { 0.0 },
            1.0,
        );
        let c = *view_proj * corner;
        let tests = [c.x < -c.w, c.x > c.w, c.y < -c.w, c.y > c.w, c.w <= 0.0];
        for (o, t) in outside.iter_mut().zip(tests) {
            *o &= t;
        }
    }
    !outside.iter().any(|&o| o)
}

/// Draws the scene into the world's picture.
pub fn draw(world: &mut WorldDrawn<'_>) {
    let mut guard = GPU.lock().unwrap_or_else(|e| e.into_inner());
    if !ACTIVE.load(Ordering::Relaxed) {
        if let Some(gpu) = guard.as_mut()
            && (!gpu.buffers.is_empty() || gpu.lines.is_some())
        {
            gpu.buffers.clear();
            gpu.lines = None;
            gpu.order.clear();
            gpu.generation = 0;
        }
        return;
    }
    let Some(atlas) = world.atlas else { return };
    let device = world.device;
    if guard.as_ref().is_none_or(|g| g.format != world.color_format) {
        *guard = Some(Gpu::new(device, world.color_format));
    }
    let gpu = guard.as_mut().unwrap();
    gpu.sync(device);
    let Some(origins) = &gpu.origins else { return };

    // The camera, split into its block (exact) and the rest.
    let cam = world.camera;
    let block = [cam.x.floor(), cam.y.floor(), cam.z.floor()];
    let frac = [(cam.x - block[0]) as f32, (cam.y - block[1]) as f32, (cam.z - block[2]) as f32, 0.0];
    let block = [block[0] as i32, block[1] as i32, block[2] as i32, 0];
    let scale = [
        world.depth_size[0] as f32 / world.size[0].max(1) as f32,
        world.depth_size[1] as f32 / world.size[1].max(1) as f32,
    ];
    let mut data = [0u8; UNIFORM_SIZE as usize];
    data[..64].copy_from_slice(bytemuck::cast_slice(&world.view_proj.to_cols_array()));
    data[64..80].copy_from_slice(bytemuck::cast_slice(&block));
    data[80..96].copy_from_slice(bytemuck::cast_slice(&frac));
    data[96..112].copy_from_slice(bytemuck::cast_slice(&[f32::from_bits(OPACITY.load(Ordering::Relaxed)), scale[0], scale[1], 0.0]));
    world.queue.write_buffer(&gpu.uniforms, 0, &data);

    if gpu.depth.as_ref().is_none_or(|(s, _)| *s != world.size) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Schematics depth"),
            size: wgpu::Extent3d { width: world.size[0].max(1), height: world.size[1].max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        gpu.depth = Some((world.size, texture.create_view(&Default::default())));
    }
    if gpu.bound.as_ref().is_none_or(|b| b.atlas != *atlas || b.depth != *world.depth) {
        let atlas_view = atlas.create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Schematics"),
            layout: &gpu.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: gpu.uniforms.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&gpu.sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(world.depth) },
            ],
        });
        gpu.bound = Some(Bound { atlas: atlas.clone(), depth: world.depth.clone(), group });
    }
    let group = &gpu.bound.as_ref().unwrap().group;
    let depth_view = &gpu.depth.as_ref().unwrap().1;
    let mut pass = world.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Schematics"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: world.color_view,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth_view,
            depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Discard }),
            stencil_ops: None,
        }),
        ..Default::default()
    });
    pass.set_bind_group(0, group, &[]);
    pass.set_index_buffer(gpu.indices.slice(..), wgpu::IndexFormat::Uint32);
    pass.set_vertex_buffer(1, origins.slice(..));
    pass.set_pipeline(&gpu.ghost);
    for &(key, origin, instance) in &gpu.order {
        let min = [
            (f64::from(origin[0]) - cam.x) as f32,
            (f64::from(origin[1]) - cam.y) as f32,
            (f64::from(origin[2]) - cam.z) as f32,
        ];
        if !in_view(&world.view_proj, min) {
            continue;
        }
        let Some((_, buffer, quads)) = gpu.buffers.get(&key) else { continue };
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw_indexed(0..quads * 6, 0, instance..instance + 1);
    }
    if let Some((_, buffer, quads)) = &gpu.lines {
        pass.set_pipeline(&gpu.over);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw_indexed(0..quads * 6, 0, gpu.lines_instance..gpu.lines_instance + 1);
    }
}
