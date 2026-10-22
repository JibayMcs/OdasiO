//! Rendu GPU de l'aperçu (wgpu, partagé avec egui) : chaque vue est dessinée dans sa propre
//! texture (multi-échantillonnée, avec profondeur), affichée ensuite comme une image egui.

use std::collections::HashMap;

use eframe::egui;
use eframe::egui_wgpu::{RenderState, wgpu};
use odasio_render::mesh::{Atlas, MeshData, Vertex};
use wgpu::util::DeviceExt;

const SHADER: &str = r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    // Couleur du ciel (rgb) et distance où le brouillard est total.
    fog: vec4<f32>,
    // Zone détaillée (x0, z0, x1, z1) : le relief lointain y est masqué.
    near_rect: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var atlas: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;

struct VertexIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};
struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
};

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    var out: VertexOut;
    var pos = v.pos;
    // Relief lointain : légèrement abaissé pour que les blocs détaillés passent devant.
    if (v.uv.x < 0.0 && v.uv.x > -1.5) {
        pos.y = pos.y - 0.2;
    }
    out.clip = u.view_proj * vec4<f32>(pos, 1.0);
    out.uv = v.uv;
    out.color = v.color;
    out.world = pos;
    return out;
}

@fragment
fn fs_main(i: VertexOut) -> @location(0) vec4<f32> {
    let tint = pow(i.color.rgb, vec3<f32>(2.2));
    // Échantillonné hors des branches : les mipmaps ont besoin de dérivées uniformes.
    let t = textureSample(atlas, atlas_sampler, max(i.uv, vec2<f32>(0.0)));
    var base: vec4<f32>;
    if (i.uv.x < 0.0) {
        if (i.uv.x > -1.5
            && i.world.x > u.near_rect.x && i.world.x < u.near_rect.z
            && i.world.z > u.near_rect.y && i.world.z < u.near_rect.w) {
            discard;
        }
        base = vec4<f32>(tint, i.color.a);
    } else {
        if (i.color.a > 0.99 && t.a < 0.5) {
            discard;
        }
        base = vec4<f32>(t.rgb * tint, t.a * i.color.a);
    }
    let d = distance(i.world, u.camera.xyz);
    let f = clamp((d - u.fog.w * 0.45) / (u.fog.w * 0.55), 0.0, 1.0);
    // Cible non sRGB : egui l'affiche telle quelle, on y écrit des couleurs gamma.
    let lit = mix(base.rgb, u.fog.rgb, f * f);
    return vec4<f32>(pow(lit, vec3<f32>(1.0 / 2.2)), base.a);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    pub view_proj: [[f32; 4]; 4],
    pub camera: [f32; 4],
    pub fog: [f32; 4],
    pub near_rect: [f32; 4],
}

pub struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
}

impl GpuMesh {
    pub fn new(device: &wgpu::Device, mesh: &MeshData) -> Option<Self> {
        if mesh.is_empty() {
            return None;
        }
        Some(Self {
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("odasio_vertices"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("odasio_indices"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            count: mesh.indices.len() as u32,
        })
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.count, 0, 0..1);
    }
}

/// Maillages d'un chunk sur le GPU : partie opaque et liquides.
#[derive(Default)]
pub struct ChunkGpu {
    pub opaque: Option<GpuMesh>,
    pub translucent: Option<GpuMesh>,
}

/// Données GPU d'un des deux mondes.
pub struct SideGpu {
    atlas: wgpu::Texture,
    atlas_group: wgpu::BindGroup,
    atlas_version: u64,
    pub chunks: HashMap<(i32, i32), ChunkGpu>,
    /// Relief lointain par région, avec son pas.
    pub far: HashMap<(i32, i32), (usize, Option<GpuMesh>)>,
    /// Plan d'eau (océan de Vintage Story).
    pub water: Option<GpuMesh>,
}

struct Target {
    size: [u32; 2],
    msaa: wgpu::TextureView,
    resolve: wgpu::TextureView,
    depth: wgpu::TextureView,
    id: egui::TextureId,
}

pub struct Gpu {
    opaque: wgpu::RenderPipeline,
    translucent: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    uniform_group: wgpu::BindGroup,
    pub sides: [SideGpu; 2],
    targets: [Option<Target>; 2],
    pub highlight: Option<GpuMesh>,
}

/// Cibles de rendu en couleurs gamma (affichées telles quelles par egui) ; l'atlas est en
/// sRGB pour que l'échantillonnage donne des couleurs linéaires.
const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const ATLAS: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SAMPLES: u32 = 4;

impl Gpu {
    pub fn new(rs: &RenderState, atlas_size: u32) -> Self {
        let device = &rs.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("odasio_preview"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("odasio_uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let atlas_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("odasio_atlas"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("odasio_preview"),
            bind_group_layouts: &[&uniform_layout, &atlas_layout],
            push_constant_ranges: &[],
        });
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Unorm8x4],
        };
        let pipeline = |translucent: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(if translucent {
                    "odasio_translucent"
                } else {
                    "odasio_opaque"
                }),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: std::slice::from_ref(&vertex_layout),
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: !translucent,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLES,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR,
                        blend: translucent.then_some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview: None,
                cache: None,
            })
        };
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("odasio_uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("odasio_uniforms"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("odasio_atlas"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let side = || {
            let atlas = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("odasio_atlas"),
                size: wgpu::Extent3d {
                    width: atlas_size,
                    height: atlas_size,
                    depth_or_array_layers: 1,
                },
                mip_level_count: atlas_size.ilog2() + 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: ATLAS,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = atlas.create_view(&Default::default());
            let atlas_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("odasio_atlas"),
                layout: &atlas_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });
            SideGpu {
                atlas,
                atlas_group,
                atlas_version: u64::MAX,
                chunks: HashMap::new(),
                far: HashMap::new(),
                water: None,
            }
        };
        Self {
            opaque: pipeline(false),
            translucent: pipeline(true),
            uniforms,
            uniform_group,
            sides: [side(), side()],
            targets: [None, None],
            highlight: None,
        }
    }

    /// Renvoie l'atlas au GPU s'il a changé.
    pub fn sync_atlas(&mut self, rs: &RenderState, side: usize, atlas: &Atlas) {
        let gpu = &mut self.sides[side];
        if gpu.atlas_version == atlas.version {
            return;
        }
        gpu.atlas_version = atlas.version;
        // Niveau 0 puis mipmaps (moyenne 2 × 2), contre le scintillement au loin.
        let mut level = atlas.rgba.clone();
        let mut size = atlas.size;
        for mip in 0..=atlas.size.ilog2() {
            rs.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &gpu.atlas,
                    mip_level: mip,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &level,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size * 4),
                    rows_per_image: Some(size),
                },
                wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
            );
            if size == 1 {
                break;
            }
            let half = size / 2;
            let mut next = vec![0u8; (half * half * 4) as usize];
            for y in 0..half as usize {
                for x in 0..half as usize {
                    for c in 0..4 {
                        let at = |dx: usize, dy: usize| {
                            level[((2 * y + dy) * size as usize + 2 * x + dx) * 4 + c] as u32
                        };
                        next[(y * half as usize + x) * 4 + c] =
                            ((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1)) / 4) as u8;
                    }
                }
            }
            level = next;
            size = half;
        }
    }

    /// (Re)crée la cible de rendu d'une vue à la bonne taille ; renvoie son image egui.
    fn target(&mut self, rs: &RenderState, view: usize, size: [u32; 2]) -> egui::TextureId {
        if let Some(t) = &self.targets[view]
            && t.size == size
        {
            return t.id;
        }
        let device = &rs.device;
        let texture = |format, samples, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("odasio_target"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let msaa = texture(COLOR, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let resolve = texture(
            COLOR,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let depth = texture(DEPTH, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let mut renderer = rs.renderer.write();
        let id = match &self.targets[view] {
            Some(old) => {
                renderer.update_egui_texture_from_wgpu_texture(
                    device,
                    &resolve,
                    wgpu::FilterMode::Linear,
                    old.id,
                );
                old.id
            }
            None => renderer.register_native_texture(device, &resolve, wgpu::FilterMode::Linear),
        };
        self.targets[view] = Some(Target {
            size,
            msaa,
            resolve,
            depth,
            id,
        });
        id
    }

    /// Dessine les deux vues ; `views[i]` = (monde, taille en pixels). Renvoie les images.
    pub fn render(
        &mut self,
        rs: &RenderState,
        uniforms: &Uniforms,
        views: [(usize, [u32; 2]); 2],
        sky: [f64; 3],
    ) -> [egui::TextureId; 2] {
        rs.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(uniforms));
        let ids = [
            self.target(rs, 0, views[0].1),
            self.target(rs, 1, views[1].1),
        ];
        let mut encoder = rs
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("odasio_preview"),
            });
        for (view, (side, _)) in views.iter().enumerate() {
            let target = self.targets[view].as_ref().expect("cible créée");
            let gpu = &self.sides[*side];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("odasio_preview"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.msaa,
                    resolve_target: Some(&target.resolve),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: sky[0],
                            g: sky[1],
                            b: sky[2],
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.uniform_group, &[]);
            pass.set_bind_group(1, &gpu.atlas_group, &[]);
            pass.set_pipeline(&self.opaque);
            for chunk in gpu.chunks.values() {
                if let Some(mesh) = &chunk.opaque {
                    mesh.draw(&mut pass);
                }
            }
            for (_, mesh) in gpu.far.values() {
                if let Some(mesh) = mesh {
                    mesh.draw(&mut pass);
                }
            }
            pass.set_pipeline(&self.translucent);
            for chunk in gpu.chunks.values() {
                if let Some(mesh) = &chunk.translucent {
                    mesh.draw(&mut pass);
                }
            }
            if let Some(water) = &gpu.water {
                water.draw(&mut pass);
            }
            if let Some(highlight) = &self.highlight {
                highlight.draw(&mut pass);
            }
        }
        rs.queue.submit([encoder.finish()]);
        ids
    }
}
