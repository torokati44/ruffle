use crate::backends::DesktopUiBackend;
use crate::cli::Opt;
use crate::custom_event::RuffleEvent;
use crate::gui::movie::{MovieView, MovieViewRenderer};
use crate::gui::{RuffleGui, MENU_HEIGHT};
use crate::player::{PlayerController, PlayerOptions};
use anyhow::anyhow;
use fontdb::{Database, Family, Query, Source};
use ruffle_core::Player;
use ruffle_render_wgpu::backend::{request_adapter_and_device, WgpuRenderBackend};
use ruffle_render_wgpu::descriptors::Descriptors;
use ruffle_render_wgpu::utils::{format_list, get_backend_names};
use std::rc::Rc;
use std::sync::{Arc, MutexGuard};
use std::time::{Duration, Instant};
use unic_langid::LanguageIdentifier;
use url::Url;
use winit::dpi::PhysicalSize;
use winit::event_loop::EventLoop;
use winit::window::{Theme, Window};

/// Integration layer connecting wgpu+winit to egui.
pub struct GuiController {
    descriptors: Arc<Descriptors>,
    gui: RuffleGui,
    window: Rc<Window>,
    last_update: Instant,
    repaint_after: Duration,
    surface: wgpu::Surface,
    surface_format: wgpu::TextureFormat,
    movie_view_renderer: Arc<MovieViewRenderer>,
    // Note that `window.get_inner_size` can change at any point on x11, even between two lines of code.
    // Use this instead.
    size: PhysicalSize<u32>,
    /// If this is set, we should not render the main menu.
    no_gui: bool,
}

impl GuiController {
    pub fn new(
        window: Rc<Window>,
        event_loop: &EventLoop<RuffleEvent>,
        opt: &Opt,
    ) -> anyhow::Result<Self> {
        let backend: wgpu::Backends = opt.graphics.into();
        if wgpu::Backends::SECONDARY.contains(backend) {
            tracing::warn!(
                "{} graphics backend support may not be fully supported.",
                format_list(&get_backend_names(backend), "and")
            );
        }
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: backend,
            dx12_shader_compiler: wgpu::Dx12Compiler::default(),
        });
        let surface = unsafe { instance.create_surface(window.as_ref()) }?;
        let (adapter, device, queue) = futures::executor::block_on(request_adapter_and_device(
            backend,
            &instance,
            Some(&surface),
            opt.power.into(),
            opt.trace_path(),
        ))
        .map_err(|e| anyhow!(e.to_string()))?;
        let surface_format = surface
            .get_capabilities(&adapter)
            .formats
            .first()
            .cloned()
            .expect("At least one format should be supported");
        let size = window.inner_size();
        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: surface_format,
                width: size.width,
                height: size.height,
                present_mode: Default::default(),
                alpha_mode: Default::default(),
                view_formats: Default::default(),
            },
        );
        let descriptors = Descriptors::new(instance, adapter, device, queue);


        let movie_view_renderer = Arc::new(MovieViewRenderer::new(
            &descriptors.device,
            surface_format,
            window.fullscreen().is_none() && !opt.no_gui,
            size.height,
            window.scale_factor(),
        ));
        let event_loop = event_loop.create_proxy();
        let gui = RuffleGui::new(event_loop, opt.movie_url.clone(), PlayerOptions::from(opt));
        Ok(Self {
            descriptors: Arc::new(descriptors),
            gui,
            window,
            last_update: Instant::now(),
            repaint_after: Duration::ZERO,
            surface,
            surface_format,
            movie_view_renderer,
            size,
            no_gui: opt.no_gui,
        })
    }

    pub fn descriptors(&self) -> &Arc<Descriptors> {
        &self.descriptors
    }

    #[must_use]
    pub fn handle_event(&mut self, event: &winit::event::WindowEvent) -> bool {
        if let winit::event::WindowEvent::Resized(size) = &event {
            if size.width > 0 && size.height > 0 {
                self.surface.configure(
                    &self.descriptors.device,
                    &wgpu::SurfaceConfiguration {
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        format: self.surface_format,
                        width: size.width,
                        height: size.height,
                        present_mode: Default::default(),
                        alpha_mode: Default::default(),
                        view_formats: Default::default(),
                    },
                );
                self.movie_view_renderer.update_resolution(
                    &self.descriptors,
                    self.window.fullscreen().is_none() && !self.no_gui,
                    size.height,
                    self.window.scale_factor(),
                );
                self.size = *size;
            }
        }

        false


    }

    pub fn create_movie(
        &mut self,
        player: &mut PlayerController,
        opt: PlayerOptions,
        movie_url: Url,
    ) {
        let movie_view = MovieView::new(
            self.movie_view_renderer.clone(),
            &self.descriptors.device,
            self.size.width,
            self.size.height,
        );
        player.create(&opt, &movie_url, movie_view);
        self.gui.on_player_created(
            opt,
            movie_url,
            player
                .get()
                .expect("Player must exist after being created."),
        );
    }

    pub fn render(&mut self, mut player: Option<MutexGuard<Player>>) {
        let surface_texture = self
            .surface
            .get_current_texture()
            .expect("Surface became unavailable");

        let show_menu = self.window.fullscreen().is_none() && !self.no_gui;

        let scale_factor = self.window.scale_factor() as f32;

        let mut encoder =
            self.descriptors
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("egui encoder"),
                });

        let movie_view = if let Some(player) = player.as_deref_mut() {
            let renderer = player
                .renderer_mut()
                .downcast_mut::<WgpuRenderBackend<MovieView>>()
                .expect("Renderer must be correct type");
            Some(renderer.target())
        } else {
            None
        };

        {
            let surface_view = surface_texture.texture.create_view(&Default::default());

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
                label: Some("egui_render"),
            });

            if let Some(movie_view) = movie_view {
                movie_view.render(&self.movie_view_renderer, &mut render_pass);
            }

        }



                self.descriptors.queue.submit([encoder.finish()]);
        surface_texture.present();
    }

    pub fn show_context_menu(&mut self, menu: Vec<ruffle_core::ContextMenuItem>) {
        self.gui.show_context_menu(menu);
    }

    pub fn is_context_menu_visible(&self) -> bool {
        self.gui.is_context_menu_visible()
    }

    pub fn needs_render(&self) -> bool {
        Instant::now().duration_since(self.last_update) >= self.repaint_after
    }

    pub fn show_open_dialog(&mut self) {
        self.gui.open_file_advanced()
    }
}
