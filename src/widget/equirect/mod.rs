use std::sync::Arc;

use glam::{vec2, vec3, Quat, Vec2, Vec3};
use iced::advanced::graphics::core::event;
use iced::mouse::{Button, ScrollDelta};
use iced::widget::shader::{self, wgpu};
use iced::{mouse, Rectangle};
use image::{EncodableLayout, GenericImageView};

pub fn equirect<'a, Message>(
    image: Arc<image::DynamicImage>,
    on_aov_change: impl Fn(f32) -> Message + 'a,
    on_look_at_change: impl Fn(glam::Vec2) -> Message + 'a,
) -> Equirect<'a, Message>
where
    Message: Clone,
{
    Equirect::new(image, on_aov_change, on_look_at_change)
}

pub struct Equirect<'a, Message> {
    pub image: Arc<image::DynamicImage>,
    on_aov_change: Box<dyn Fn(f32) -> Message + 'a>,
    on_look_at_change: Box<dyn Fn(glam::Vec2) -> Message + 'a>,
}

impl<'a, Message: Clone> Equirect<'a, Message> {
    pub fn new<F, G>(
        image: Arc<image::DynamicImage>,
        on_aov_change: F,
        on_look_at_cahnge: G,
    ) -> Self
    where
        F: 'a + Fn(f32) -> Message,
        G: 'a + Fn(glam::Vec2) -> Message,
    {
        Equirect {
            image: image,
            on_aov_change: Box::new(on_aov_change),
            on_look_at_change: Box::new(on_look_at_cahnge),
        }
    }

    fn on_aov_changed(
        &self,
        state: &EquirectState,
        shell: &mut iced::advanced::Shell<'_, Message>,
    ) {
        shell.publish((self.on_aov_change)(state.aov));
    }

    fn on_look_at_changed(
        &self,
        state: &EquirectState,
        shell: &mut iced::advanced::Shell<'_, Message>,
    ) {
        let x = state.look_at.z.atan2(state.look_at.x);
        let y = state
            .look_at
            .y
            .atan2((state.look_at.x.powi(2) + state.look_at.z.powi(2)).sqrt());

        shell.publish((self.on_look_at_change)(vec2(x, y)));
    }
}

impl<'a, Message: Clone> shader::Program<Message> for Equirect<'a, Message> {
    type State = EquirectState;
    type Primitive = EquirectPrimitive;

    fn draw(
        &self,
        state: &Self::State,
        _cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> Self::Primitive {
        EquirectPrimitive::new(
            bounds,
            EquirectUniforms {
                aov: state.aov,
                look_at: state.look_at,
                up: state.up,
                right: state.right,
                ..Default::default()
            },
            self.image.clone(),
        )
    }

    fn update(
        &self,
        state: &mut Self::State,
        event: iced::widget::shader::Event,
        bounds: Rectangle,
        _cursor: iced::advanced::mouse::Cursor,
        shell: &mut iced::advanced::Shell<'_, Message>,
    ) -> (
        iced::advanced::graphics::core::event::Status,
        Option<Message>,
    ) {
        let ret = match event {
            shader::Event::Mouse(mouse::Event::ButtonPressed(Button::Left)) => {
                state.mouse_pressed = true;
                (event::Status::Captured, None)
            }
            shader::Event::Mouse(mouse::Event::ButtonReleased(Button::Left)) => {
                state.mouse_pressed = false;
                (event::Status::Captured, None)
            }
            shader::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                state.mouse_delta = vec2(
                    position.x - state.mouse_point_prev.x,
                    position.y - state.mouse_point_prev.y,
                );
                state.mouse_point_prev = vec2(position.x, position.y);
                (event::Status::Captured, None)
            }
            shader::Event::Mouse(mouse::Event::WheelScrolled {
                delta: ScrollDelta::Lines { x: _, y },
            }) => {
                state.aov = state.aov - y / 10.0;
                self.on_aov_changed(state, shell);
                (event::Status::Captured, None)
            }
            shader::Event::Mouse(mouse::Event::WheelScrolled {
                delta: ScrollDelta::Pixels { x: _, y },
            }) => {
                state.aov = state.aov - y / 50.0;
                self.on_aov_changed(state, shell);
                (event::Status::Captured, None)
            }
            _ => (event::Status::Ignored, None),
        };

        if state.mouse_pressed {
            let yaw = state.mouse_delta.x / bounds.width;
            let pitch = -state.mouse_delta.y / bounds.width;
            let quat = Quat::from_axis_angle(state.up, yaw)
                .mul_quat(Quat::from_axis_angle(state.right, -pitch));

            state.look_at = quat.mul_vec3(state.look_at).normalize();
            state.right = state.look_at.cross(state.up).normalize();
            state.up = state.right.cross(state.look_at).normalize();

            self.on_look_at_changed(state, shell);

            // consume delta
            state.mouse_delta = vec2(0.0, 0.0);
        }

        ret
    }
}

pub struct EquirectState {
    mouse_pressed: bool,
    mouse_point_prev: Vec2,
    mouse_delta: Vec2,
    aov: f32,
    look_at: Vec3,
    up: Vec3,
    right: Vec3,
}

impl Default for EquirectState {
    fn default() -> Self {
        Self {
            mouse_pressed: false,
            mouse_point_prev: vec2(0., 0.),
            mouse_delta: vec2(0., 0.),
            aov: 1.0,
            look_at: vec3(1., 0., 0.),
            up: vec3(0., 1., 0.),
            right: vec3(0., 0., 1.),
        }
    }
}

pub struct EquirectPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    image: Arc<image::DynamicImage>,
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
}

impl EquirectPipeline {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        image: Arc<image::DynamicImage>,
    ) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Equirect Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Equirect Texture"),
            size: wgpu::Extent3d {
                width: image.width(),
                height: image.height(),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Equirect Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("./shaders/equirect.wgsl").into()),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Equirect Uniform Buffer"),
            size: std::mem::size_of::<EquirectUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Equirect BindGroup Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: { wgpu::TextureSampleType::Float { filterable: true } },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shader_quad uniform bind group"),
            layout: &uniform_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Equirect Pipeline Layout"),
            bind_group_layouts: &[&uniform_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Equirect Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });

        Self {
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            image,
            texture,
            texture_view,
            sampler,
        }
    }

    fn update(&mut self, queue: &wgpu::Queue, uniforms: &EquirectUniforms) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniforms));
    }

    pub fn render(
        &self,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        viewport: Rectangle<u32>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_scissor_rect(viewport.x, viewport.y, viewport.width, viewport.height);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// A struct that represents a uniform for the shader.
/// Its members have to be aligned to 16bytes.
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct EquirectUniforms {
    aov: f32,
    _padding1: [f32; 3],
    look_at: glam::Vec3,
    _padding2: [f32; 1],
    up: glam::Vec3,
    _padding3: [f32; 1],
    right: glam::Vec3,
    _padding4: [f32; 1],
}

impl Default for EquirectUniforms {
    fn default() -> Self {
        Self {
            aov: 1.0,
            look_at: glam::vec3(1.0, 0.0, 0.0),
            up: glam::vec3(0.0, 1.0, 0.0),
            right: glam::vec3(0.0, 0.0, 1.0),

            _padding1: [0.0; 3],
            _padding2: [0.0; 1],
            _padding3: [0.0; 1],
            _padding4: [0.0; 1],
        }
    }
}

#[derive(Debug)]
pub struct EquirectPrimitive {
    uniforms: EquirectUniforms,
    image: Arc<image::DynamicImage>,
}

impl EquirectPrimitive {
    pub fn new(
        bounds: Rectangle,
        unifroms: EquirectUniforms,
        image: Arc<image::DynamicImage>,
    ) -> Self {
        Self {
            uniforms: unifroms,
            image: image,
        }
    }
}

impl shader::Primitive for EquirectPrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut shader::Storage,
        _bounds: &Rectangle,
        viewport: &shader::Viewport,
    ) {
        let mut is_texture_invalidated = false;

        if storage.has::<EquirectPipeline>() {
            let pipeline = storage.get_mut::<EquirectPipeline>().unwrap();
            if pipeline.image != self.image {
                let new_pipeline = EquirectPipeline::new(device, format, self.image.clone());
                is_texture_invalidated = true;
                storage.store(new_pipeline)
            }
        } else {
            let pipeline = EquirectPipeline::new(device, format, self.image.clone());
            is_texture_invalidated = true;
            storage.store(pipeline);
        }

        let pipeline = storage.get_mut::<EquirectPipeline>().unwrap();

        if is_texture_invalidated {
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &pipeline.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                pipeline.image.to_rgba8().as_bytes(),
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * self.image.dimensions().0),
                    rows_per_image: Some(self.image.dimensions().1),
                },
                pipeline.texture.size(),
            );
        }

        pipeline.update(queue, &self.uniforms);
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &shader::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let pipeline = storage.get::<EquirectPipeline>().unwrap();
        pipeline.render(target, encoder, *clip_bounds);
    }
}
