//! Puts the picture on screen with the GPU (wgpu: Vulkan, or OpenGL as a fallback). From Kadence.
//!
//! The parts are pictures rasterized on the CPU (device faces, LCDs). Each becomes a texture,
//! uploaded once, and every frame draws them as rectangles in order. While zooming, the old
//! pictures are drawn larger or smaller until sharp ones replace them. The cords are triangles
//! (see `mesh`), drawn every frame among the pictures: sharp at any zoom, and nothing to
//! rasterize when they move.

use std::collections::HashMap;
use std::sync::Arc;

use winit::event_loop::OwnedDisplayHandle;
use winit::window::Window;

use resvg::tiny_skia::Pixmap;

use super::mesh::{self, Mesh};

/// A picture to draw, and where: (x, y, w, h) in window pixels.
pub struct Sprite<'a> {
    pub id: u64,
    /// `None` once the GPU has the pixels
    pub pm: Option<&'a Pixmap>,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// how opaque, 0…1
    pub alpha: f32,
    /// only what lies inside this rectangle (window pixels: x, y, w, h) is drawn
    pub clip: Option<[f64; 4]>,
}

/// Behind everything.
const BG: wgpu::Color = wgpu::Color { r: 0x15 as f64 / 255.0, g: 0x16 as f64 / 255.0, b: 0x18 as f64 / 255.0, a: 1.0 };

const SHADER: &str = r#"
struct V { @builtin(position) pos: vec4f, @location(0) uv: vec2f, @location(1) a: f32 }
@vertex fn vs(@location(0) p: vec2f, @location(1) uv: vec2f, @location(2) a: f32) -> V {
    return V(vec4f(p, 0.0, 1.0), uv, a);
}
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
@fragment fn fs(v: V) -> @location(0) vec4f {
    // the pixels are premultiplied, so fading is a plain product
    return textureSample(tex, smp, v.uv) * v.a;
}
"#;

/// Floats per vertex: position in clip space, texture position, opacity.
const VERTEX: usize = 5;

/// The cords: coverage from the distance to the stroke's centre line (or a spot's middle), so
/// edges are smooth without multisampling; colours are premultiplied.
const MESH_SHADER: &str = r#"
struct V { @builtin(position) pos: vec4f, @location(0) e: vec2f, @location(1) c: vec4f, @location(2) hs: vec2f }
@vertex fn vs(@location(0) p: vec2f, @location(1) e: vec2f, @location(2) c: vec4f, @location(3) hs: vec2f) -> V {
    return V(vec4f(p, 0.0, 1.0), e, c, hs);
}
@fragment fn fs(v: V) -> @location(0) vec4f {
    let cover = clamp((v.hs.x - length(v.e)) / max(v.hs.y, 0.001) + 0.5, 0.0, 1.0);
    return v.c * cover;
}
"#;

struct Texture {
    bind: wgpu::BindGroup,
}

pub struct Gpu {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    /// the cords: blended over what is below, and their lights added to it
    mesh_over: wgpu::RenderPipeline,
    mesh_add: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    vertices: wgpu::Buffer,
    mesh_vertices: wgpu::Buffer,
    /// the layers' textures, by layer id
    textures: HashMap<u64, Texture>,
    clear: wgpu::Color,
    pub info: String,
}

impl Gpu {
    pub fn new(window: Arc<Window>, display: OwnedDisplayHandle) -> Result<Self, String> {
        let transparent = false;
        // Vulkan, Metal or DX12 first; OpenGL only where none of those works (old GPUs, some
        // virtual machines), since starting it as well costs memory
        let display = Box::new(display);
        let (surface, adapter) = adapter(display.clone(), &window, wgpu::Backends::PRIMARY)
            .or_else(|_| adapter(display, &window, wgpu::Backends::GL))
            .map_err(|e| format!("no usable graphics adapter: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            ..Default::default()
        }))
        .map_err(|e| format!("graphics device: {e}"))?;

        let caps = surface.get_capabilities(&adapter);
        // keep the alpha channel for the compositor; premultiplied is what the layers hold
        let alpha = [wgpu::CompositeAlphaMode::PreMultiplied, wgpu::CompositeAlphaMode::Inherit]
            .into_iter()
            .find(|m| transparent && caps.alpha_modes.contains(m));
        // 8 bits per channel: some systems offer a 10-bit format first, with only 2 bits of alpha
        let format = [wgpu::TextureFormat::Bgra8Unorm, wgpu::TextureFormat::Rgba8Unorm]
            .into_iter()
            .find(|f| caps.formats.contains(f))
            .unwrap_or(caps.formats[0]);
        // mailbox never makes the app wait for the display; the app paces its own frames
        let present_mode = if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) { wgpu::PresentMode::Mailbox } else { wgpu::PresentMode::Fifo };
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: alpha.unwrap_or(caps.alpha_modes[0]),
            view_formats: vec![],
            color_space: Default::default(),
        };
        surface.configure(&device, &config);
        let a = adapter.get_info();
        let info = format!("{:?} on {} ({}), {format:?}, {present_mode:?}, alpha {:?}", a.backend, a.name, a.driver, config.alpha_mode);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: (VERTEX * 4) as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let mesh_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(MESH_SHADER.into()) });
        let mesh_pipeline = |blend: wgpu::BlendState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: None,
                vertex: wgpu::VertexState {
                    module: &mesh_shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: (mesh::VERTEX * 4) as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32x2],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &mesh_shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: Some(blend), write_mask: wgpu::ColorWrites::ALL })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let add = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add };
        let mesh_over = mesh_pipeline(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        let mesh_add = mesh_pipeline(wgpu::BlendState { color: add, alpha: add });
        // smooth when a picture is drawn at another size (while resizing); exact at its own size
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let vertices = vertex_buffer(&device, 64);
        let mesh_vertices = buffer(&device, 1 << 20);
        Ok(Gpu {
            surface,
            config,
            device,
            queue,
            pipeline,
            mesh_over,
            mesh_add,
            sampler,
            vertices,
            mesh_vertices,
            textures: HashMap::new(),
            clear: if alpha.is_some() { wgpu::Color::TRANSPARENT } else { BG },
            info,
        })
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        (self.config.width, self.config.height) = (w.max(1), h.max(1));
        self.surface.configure(&self.device, &self.config);
    }

    /// Draws the sprites in order, and the cords (`mesh`, in window pixels) just before the
    /// sprite at index `at`. Textures of layers not among them, nor in `keep`, are freed.
    /// Returns how long it waited for the frame, uploaded pictures, and drew (for `prof`).
    pub fn draw(&mut self, sprites: &[Sprite], mesh: Option<(usize, &Mesh)>, keep: &std::collections::HashSet<u64>) -> [std::time::Duration; 3] {
        let t0 = std::time::Instant::now();
        // the frame first: with no frame to draw into (the window hidden, minimized or on another
        // desktop) nothing is uploaded, since only a submit lets the GPU take uploads in and free
        // what was let go; what it has let go meanwhile is freed now
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                let _ = self.device.poll(wgpu::PollType::Poll);
                return [t0.elapsed(), Default::default(), Default::default()];
            }
            _ => {
                let _ = self.device.poll(wgpu::PollType::Poll);
                return [t0.elapsed(), Default::default(), Default::default()];
            }
        };
        let t1 = std::time::Instant::now();
        let shown: std::collections::HashSet<u64> = sprites.iter().map(|s| s.id).collect();
        self.textures.retain(|id, _| keep.contains(id) || shown.contains(id));
        for s in sprites {
            if let (false, Some(pm)) = (self.textures.contains_key(&s.id), s.pm) {
                let t = self.upload(pm);
                self.textures.insert(s.id, t);
            }
        }
        let t2 = std::time::Instant::now();
        // two triangles per sprite (a layer whose pixels were freed before they reached the GPU
        // can't be drawn); `drawn` keeps each one's place in the order
        let (w, h) = (self.config.width as f64, self.config.height as f64);
        let clip = |x: f64, y: f64| [(x / w * 2.0 - 1.0) as f32, (1.0 - y / h * 2.0) as f32];
        let drawn: Vec<(usize, &Sprite)> = sprites.iter().enumerate().filter(|(_, s)| self.textures.contains_key(&s.id)).collect();
        let mut v: Vec<f32> = Vec::with_capacity(drawn.len() * 6 * VERTEX);
        for (_, s) in &drawn {
            // cut to its clip rectangle, the texture with it (nothing left: an empty quad)
            let (mut x0, mut y0, mut x1, mut y1) = (s.x, s.y, s.x + s.w, s.y + s.h);
            if let Some([cx, cy, cw, ch]) = s.clip {
                (x0, y0, x1, y1) = (x0.max(cx), y0.max(cy), x1.min(cx + cw).max(x0.max(cx)), y1.min(cy + ch).max(y0.max(cy)));
            }
            let (u0, v0) = (((x0 - s.x) / s.w.max(1e-9)) as f32, ((y0 - s.y) / s.h.max(1e-9)) as f32);
            let (u1, v1) = (((x1 - s.x) / s.w.max(1e-9)) as f32, ((y1 - s.y) / s.h.max(1e-9)) as f32);
            let c = [clip(x0, y0), clip(x1, y0), clip(x1, y1), clip(x0, y1)];
            let uv = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
            for i in [0, 1, 2, 0, 2, 3] {
                v.extend_from_slice(&[c[i][0], c[i][1], uv[i][0], uv[i][1], s.alpha]);
            }
        }
        if self.vertices.size() < (v.len() * 4) as u64 {
            self.vertices = vertex_buffer(&self.device, drawn.len() * 2);
        }
        self.queue.write_buffer(&self.vertices, 0, floats(&v));
        // the cords: their places to clip space, normal ones first, then the lights
        let (at, counts) = match mesh.filter(|(_, m)| !m.is_empty()) {
            Some((at, m)) => {
                let mut mv: Vec<f32> = Vec::with_capacity(m.normal.len() + m.glow.len());
                for src in [&m.normal, &m.glow] {
                    for vx in src.chunks_exact(mesh::VERTEX) {
                        let c = clip(vx[0] as f64, vx[1] as f64);
                        mv.extend_from_slice(&c);
                        mv.extend_from_slice(&vx[2..]);
                    }
                }
                if self.mesh_vertices.size() < (mv.len() * 4) as u64 {
                    self.mesh_vertices = buffer(&self.device, (mv.len() * 4).next_power_of_two());
                }
                self.queue.write_buffer(&self.mesh_vertices, 0, floats(&mv));
                (at, Some(((m.normal.len() / mesh::VERTEX) as u32, (m.glow.len() / mesh::VERTEX) as u32)))
            }
            None => (usize::MAX, None),
        };

        let view = frame.texture.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(self.clear), store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            });
            let mut cords_done = counts.is_none();
            for (i, (si, s)) in drawn.iter().enumerate() {
                if !cords_done && *si >= at {
                    cords_done = true;
                    self.draw_mesh(&mut pass, counts);
                }
                pass.set_pipeline(&self.pipeline);
                pass.set_vertex_buffer(0, self.vertices.slice(..));
                pass.set_bind_group(0, &self.textures[&s.id].bind, &[]);
                pass.draw(i as u32 * 6..i as u32 * 6 + 6, 0..1);
            }
            if !cords_done {
                self.draw_mesh(&mut pass, counts);
            }
        }
        self.queue.submit([enc.finish()]);
        self.queue.present(frame);
        [t1 - t0, t2 - t1, t2.elapsed()]
    }

    /// The cords' triangles: `counts` vertices blended over, then added (the lights).
    fn draw_mesh(&self, pass: &mut wgpu::RenderPass, counts: Option<(u32, u32)>) {
        let Some((normal, glow)) = counts else { return };
        pass.set_vertex_buffer(0, self.mesh_vertices.slice(..));
        if normal > 0 {
            pass.set_pipeline(&self.mesh_over);
            pass.draw(0..normal, 0..1);
        }
        if glow > 0 {
            pass.set_pipeline(&self.mesh_add);
            pass.draw(normal..normal + glow, 0..1);
        }
    }

    fn upload(&self, pm: &Pixmap) -> Texture {
        let size = wgpu::Extent3d { width: pm.width(), height: pm.height(), depth_or_array_layers: 1 };
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            pm.data(),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(pm.width() * 4), rows_per_image: None },
            size,
        );
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&tex.create_view(&Default::default())) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        Texture { bind }
    }

    /// True when the layer `id` is on the GPU, so its pixels are no longer needed.
    pub fn has(&self, id: u64) -> bool {
        self.textures.contains_key(&id)
    }

}

/// Finds a GPU of the given kinds that can draw into the window.
fn adapter(display: Box<OwnedDisplayHandle>, window: &Arc<Window>, backends: wgpu::Backends) -> Result<(wgpu::Surface<'static>, wgpu::Adapter), String> {
    let mut desc = wgpu::InstanceDescriptor::new_with_display_handle(display);
    desc.backends = backends;
    let instance = wgpu::Instance::new(desc);
    let surface = instance.create_surface(window.clone()).map_err(|e| e.to_string())?;
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: Some(&surface),
        ..Default::default()
    }))
    .map_err(|e| e.to_string())?;
    Ok((surface, adapter))
}

fn vertex_buffer(device: &wgpu::Device, quads: usize) -> wgpu::Buffer {
    buffer(device, quads * 6 * VERTEX * 4)
}

fn buffer(device: &wgpu::Device, bytes: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: None, size: bytes as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
}

fn floats(v: &[f32]) -> &[u8] {
    // SAFETY: f32 has no padding, and every byte pattern is a valid u8
    unsafe { std::slice::from_raw_parts(v.as_ptr().cast::<u8>(), std::mem::size_of_val(v)) }
}
