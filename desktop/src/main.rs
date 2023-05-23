#![deny(clippy::unwrap_used)]
// By default, Windows creates an additional console window for our program.
//
//
// This is silently ignored on non-windows systems.
// See https://docs.microsoft.com/en-us/cpp/build/reference/subsystem?view=msvc-160 for details.
#![windows_subsystem = "windows"]

mod audio;
mod custom_event;
mod executor;
mod gui;
mod navigator;
mod storage;
mod task;
mod ui;

use crate::custom_event::RuffleEvent;
use crate::executor::GlutinAsyncExecutor;
use crate::gui::MovieView;
use anyhow::{anyhow, Context, Error};
use clap::Parser;
use gui::GuiController;
use isahc::{config::RedirectPolicy, prelude::*, HttpClient};
use rfd::FileDialog;
use ruffle_core::backend::audio::AudioBackend;
use ruffle_core::backend::navigator::OpenURLMode;
use ruffle_core::events::{KeyCode, TextControlCode};
use ruffle_core::{
    config::Letterbox, tag_utils::SwfMovie, LoadBehavior, Player, PlayerBuilder, PlayerEvent,
    StageDisplayState, StageScaleMode, StaticCallstack, ViewportDimensions,
};
use ruffle_render::backend::RenderBackend;
use ruffle_render::quality::StageQuality;
use ruffle_render_wgpu::backend::WgpuRenderBackend;
use ruffle_render_wgpu::clap::{GraphicsBackend, PowerPreference};
use std::cell::RefCell;
use std::io::Read;
use std::panic::PanicInfo;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use url::Url;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize, Size};
use winit::event::{
    ElementState, KeyboardInput, ModifiersState, MouseButton, MouseScrollDelta, VirtualKeyCode,
    WindowEvent,
};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};
use winit::window::{Fullscreen, Icon, Window, WindowBuilder};

thread_local! {
    static CALLSTACK: RefCell<Option<StaticCallstack>> = RefCell::default();
    static RENDER_INFO: RefCell<Option<String>> = RefCell::default();
    static SWF_INFO: RefCell<Option<String>> = RefCell::default();
}

#[cfg(feature = "tracy")]
#[global_allocator]
static GLOBAL: tracing_tracy::client::ProfiledAllocator<std::alloc::System> =
    tracing_tracy::client::ProfiledAllocator::new(std::alloc::System, 0);

static RUFFLE_VERSION: &str = include_str!(concat!(env!("OUT_DIR"), "/version-info.txt"));

#[derive(Parser, Debug)]
#[clap(
    name = "Ruffle",
    author,
    version = RUFFLE_VERSION,
)]
struct Opt {
    /// Path or URL of a Flash movie (SWF) to play.
    #[clap(name = "FILE")]
    input_path: Option<PathBuf>,

    /// A "flashvars" parameter to provide to the movie.
    /// This can be repeated multiple times, for example -Pkey=value -Pfoo=bar.
    #[clap(short = 'P', action = clap::ArgAction::Append)]
    parameters: Vec<String>,

    /// Type of graphics backend to use. Not all options may be supported by your current system.
    /// Default will attempt to pick the most supported graphics backend.
    #[clap(long, short, default_value = "default")]
    graphics: GraphicsBackend,

    /// Power preference for the graphics device used. High power usage tends to prefer dedicated GPUs,
    /// whereas a low power usage tends prefer integrated GPUs.
    #[clap(long, short, default_value = "high")]
    power: PowerPreference,

    /// Width of window in pixels.
    #[clap(long, display_order = 1)]
    width: Option<f64>,

    /// Height of window in pixels.
    #[clap(long, display_order = 2)]
    height: Option<f64>,

    /// Maximum number of seconds a script can run before scripting is disabled.
    #[clap(long, short, default_value = "15.0")]
    max_execution_duration: f64,

    /// Base directory or URL used to resolve all relative path statements in the SWF file.
    /// The default is the current directory.
    #[clap(long)]
    base: Option<Url>,

    /// Default quality of the movie.
    #[clap(long, short, default_value = "high")]
    quality: StageQuality,

    /// The scale mode of the stage.
    #[clap(long, short, default_value = "show-all")]
    scale: StageScaleMode,

    /// Audio volume as a number between 0 (muted) and 1 (full volume)
    #[clap(long, short, default_value = "1.0")]
    volume: f32,

    /// Prevent movies from changing the stage scale mode.
    #[clap(long, action)]
    force_scale: bool,

    /// Location to store a wgpu trace output
    #[clap(long)]
    #[cfg(feature = "render_trace")]
    trace_path: Option<PathBuf>,

    /// Proxy to use when loading movies via URL.
    #[clap(long)]
    proxy: Option<Url>,

    /// Replace all embedded HTTP URLs with HTTPS.
    #[clap(long, action)]
    upgrade_to_https: bool,

    /// Start application in fullscreen.
    #[clap(long, action)]
    fullscreen: bool,

    #[clap(long, action)]
    timedemo: bool,

    /// Start application without ActionScript 3 warning.
    #[clap(long, action)]
    dont_warn_on_unsupported_content: bool,

    #[clap(long, default_value = "streaming")]
    load_behavior: LoadBehavior,

    /// Specify how Ruffle should handle areas outside the movie stage.
    #[clap(long, default_value = "on")]
    letterbox: Letterbox,

    /// Spoofs the root SWF URL provided to ActionScript.
    #[clap(long, value_parser)]
    spoof_url: Option<Url>,

    /// The version of the player to emulate
    #[clap(long)]
    player_version: Option<u8>,

    /// Set and lock the player's frame rate, overriding the movie's frame rate.
    #[clap(long)]
    frame_rate: Option<f64>,

    /// The handling mode of links opening a new website.
    #[clap(long, default_value = "allow")]
    open_url_mode: OpenURLMode,
}

#[cfg(feature = "render_trace")]
fn trace_path(opt: &Opt) -> Option<&Path> {
    if let Some(path) = &opt.trace_path {
        let _ = std::fs::create_dir_all(path);
        Some(path)
    } else {
        None
    }
}

#[cfg(not(feature = "render_trace"))]
fn trace_path(_opt: &Opt) -> Option<&Path> {
    None
}

fn parse_url(path: &Path) -> Result<Url, Error> {
    if path.exists() {
        let absolute_path = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        Url::from_file_path(absolute_path)
            .map_err(|_| anyhow!("Path must be absolute and cannot be a URL"))
    } else {
        Url::parse(path.to_str().unwrap_or_default())
            .ok()
            .filter(|url| url.host().is_some() || url.scheme() == "file")
            .ok_or_else(|| anyhow!("Input path is not a file and could not be parsed as a URL."))
    }
}

fn parse_parameters(opt: &Opt) -> impl '_ + Iterator<Item = (String, String)> {
    opt.parameters.iter().map(|parameter| {
        let mut split = parameter.splitn(2, '=');
        if let (Some(key), Some(value)) = (split.next(), split.next()) {
            (key.to_owned(), value.to_owned())
        } else {
            (parameter.clone(), "".to_string())
        }
    })
}

fn pick_file() -> Option<PathBuf> {
    FileDialog::new()
        .add_filter("Flash Files", &["swf", "spl"])
        .add_filter("All Files", &["*"])
        .set_title("Load a Flash File")
        .pick_file()
}

fn load_movie(url: &Url, opt: &Opt) -> Result<SwfMovie, Error> {
    let mut movie = if url.scheme() == "file" {
        SwfMovie::from_path(
            url.to_file_path()
                .map_err(|_| anyhow!("Invalid swf path"))?,
            None,
        )
        .map_err(|e| anyhow!(e.to_string()))
        .context("Couldn't load swf")?
    } else {
        let proxy = opt.proxy.as_ref().and_then(|url| url.as_str().parse().ok());
        let builder = HttpClient::builder()
            .proxy(proxy)
            .redirect_policy(RedirectPolicy::Follow);
        let client = builder.build().context("Couldn't create HTTP client")?;
        let response = client
            .get(url.to_string())
            .with_context(|| format!("Couldn't load URL {url}"))?;
        let mut buffer: Vec<u8> = Vec::new();
        response
            .into_body()
            .read_to_end(&mut buffer)
            .context("Couldn't read response from server")?;

        SwfMovie::from_data(&buffer, url.to_string(), None)
            .map_err(|e| anyhow!(e.to_string()))
            .context("Couldn't load swf")?
    };

    movie.append_parameters(parse_parameters(opt));

    Ok(movie)
}

fn get_screen_size(event_loop: &EventLoop<RuffleEvent>) -> PhysicalSize<u32> {
    let mut min_x = 0;
    let mut min_y = 0;
    let mut max_x = 0;
    let mut max_y = 0;

    for monitor in event_loop.available_monitors() {
        let size = monitor.size();
        let position = monitor.position();
        min_x = min_x.min(position.x);
        min_y = min_y.min(position.y);
        max_x = max_x.max(position.x + size.width as i32);
        max_y = max_y.max(position.y + size.height as i32);
    }

    let width = max_x - min_x;
    let height = max_y - min_y;

    if width <= 32 || height <= 32 {
        return (i16::MAX as u32, i16::MAX as u32).into();
    }

    (width, height).into()
}

struct App {
    opt: Opt,
    window: Rc<Window>,
    event_loop: Option<EventLoop<RuffleEvent>>,
    event_loop_proxy: EventLoopProxy<RuffleEvent>,
    executor: Arc<Mutex<GlutinAsyncExecutor>>,
    gui: Arc<Mutex<GuiController>>,
    player: Arc<Mutex<Player>>,
    min_window_size: LogicalSize<u32>,
    max_window_size: PhysicalSize<u32>,
}

impl App {
    fn new(opt: Opt) -> Result<Self, Error> {
        let movie_url = if let Some(path) = &opt.input_path {
            Some(parse_url(path).context("Couldn't load specified path")?)
        } else {
            None
        };

        let icon_bytes = include_bytes!("../assets/favicon-32.rgba");
        let icon =
            Icon::from_rgba(icon_bytes.to_vec(), 32, 32).context("Couldn't load app icon")?;

        let event_loop = EventLoopBuilder::with_user_event().build();

        let min_window_size = (16, 16).into();
        let max_window_size = get_screen_size(&event_loop);

        let window = WindowBuilder::new()
            .with_visible(false)
            .with_title("Ruffle")
            .with_window_icon(Some(icon))
            .with_min_inner_size(min_window_size)
            .with_max_inner_size(max_window_size)
            .build(&event_loop)?;

        let mut builder = PlayerBuilder::new();

        match audio::CpalAudioBackend::new() {
            Ok(mut audio) => {
                audio.set_volume(opt.volume);
                builder = builder.with_audio(audio);
            }
            Err(e) => {
                tracing::error!("Unable to create audio device: {}", e);
            }
        };

        let (executor, channel) = GlutinAsyncExecutor::new(event_loop.create_proxy());
        let navigator = navigator::ExternalNavigatorBackend::new(
            opt.base.to_owned().unwrap_or(
                movie_url
                    .clone()
                    .unwrap_or_else(|| Url::parse("file:///empty").expect("Dummy Url")),
            ),
            channel,
            event_loop.create_proxy(),
            opt.proxy.clone(),
            opt.upgrade_to_https,
            opt.open_url_mode,
        );

        let window = Rc::new(window);

        if cfg!(feature = "software_video") {
            builder =
                builder.with_video(ruffle_video_software::backend::SoftwareVideoBackend::new());
        }

        let gui = GuiController::new(
            window.clone(),
            &event_loop,
            trace_path(&opt),
            opt.graphics.into(),
            opt.power.into(),
        )?;

        let renderer = WgpuRenderBackend::new(gui.descriptors().clone(), gui.create_movie_view())
            .map_err(|e| anyhow!(e.to_string()))
            .context("Couldn't create wgpu rendering backend")?;
        RENDER_INFO.with(|i| *i.borrow_mut() = Some(renderer.debug_info().to_string()));

        builder = builder
            .with_navigator(navigator)
            .with_renderer(renderer)
            .with_storage(storage::DiskStorageBackend::new()?)
            .with_ui(ui::DesktopUiBackend::new(window.clone())?)
            .with_autoplay(true)
            .with_letterbox(opt.letterbox)
            .with_max_execution_duration(Duration::from_secs_f64(opt.max_execution_duration))
            .with_quality(opt.quality)
            .with_warn_on_unsupported_content(!opt.dont_warn_on_unsupported_content)
            .with_scale_mode(opt.scale, opt.force_scale)
            .with_fullscreen(opt.fullscreen)
            .with_load_behavior(opt.load_behavior)
            .with_spoofed_url(opt.spoof_url.clone().map(|url| url.to_string()))
            .with_player_version(opt.player_version)
            .with_frame_rate(opt.frame_rate);

        let player = builder.build();

        CALLSTACK.with(|callstack| {
            *callstack.borrow_mut() = Some(player.lock().expect("Cannot reenter").callstack());
        });

        let mut app = Self {
            opt,
            window,
            event_loop_proxy: event_loop.create_proxy(),
            event_loop: Some(event_loop),
            executor,
            gui: Arc::new(Mutex::new(gui)),
            player,
            min_window_size,
            max_window_size,
        };

        if let Some(movie_url) = movie_url {
            app.load_swf(movie_url)?;
        }

        Ok(app)
    }

    fn load_swf(&mut self, url: Url) -> Result<(), Error> {
        let filename = url
            .path_segments()
            .and_then(|segments| segments.last())
            .unwrap_or_else(|| url.as_str());
        let title = format!("Ruffle - {filename}");
        self.window.set_title(&title);
        SWF_INFO.with(|i| *i.borrow_mut() = Some(filename.to_string()));

        let event_loop_proxy = self.event_loop_proxy.clone();
        let on_metadata = move |swf_header: &ruffle_core::swf::HeaderExt| {
            let _ = event_loop_proxy.send_event(RuffleEvent::OnMetadata(swf_header.clone()));
        };

        let mut parameters: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        parameters.extend(parse_parameters(&self.opt));
        self.player.lock().expect("Player lock").fetch_root_movie(
            url.to_string(),
            parameters,
            Box::new(on_metadata),
        );

        Ok(())
    }

    fn run(mut self) -> ! {
        enum LoadingState {
            Loading,
            WaitingForResize,
            Loaded,
        }
        let mut loaded = LoadingState::Loading;
        let mut mouse_pos = PhysicalPosition::new(0.0, 0.0);
        let mut time = Instant::now();
        let mut next_frame_time = Instant::now();
        let mut minimized = false;
        let mut modifiers = ModifiersState::empty();
        let mut fullscreen_down = false;

        if self.opt.input_path.is_none() {
            // No SWF provided on command line; show window with dummy movie immediately.
            self.window.set_visible(true);
            self.gui.lock().expect("Gui lock").set_ui_visible(true);
            loaded = LoadingState::Loaded;
        }

        // Poll UI events.
        let event_loop = self.event_loop.take().expect("App already running");
        event_loop.run(move |event, _window_target, control_flow| {
            let mut check_redraw = false;
            match event {
                winit::event::Event::LoopDestroyed => {
                    self.player
                        .lock()
                        .expect("Cannot reenter")
                        .flush_shared_objects();
                    shutdown();
                    return;
                }

                // Core loop
                winit::event::Event::MainEventsCleared
                    if matches!(loaded, LoadingState::Loaded) =>
                {
                    let new_time = Instant::now();
                    let dt = new_time.duration_since(time).as_micros();
                    if dt > 0 {
                        time = new_time;
                        let mut player_lock = self.player.lock().expect("Cannot reenter");
                        player_lock.tick(dt as f64 / 1000.0);
                        next_frame_time = new_time + player_lock.time_til_next_frame();
                        check_redraw = true;
                    }
                }

                // Render
                winit::event::Event::RedrawRequested(_) => {
                    // Don't render when minimized to avoid potential swap chain errors in `wgpu`.
                    if !minimized {
                        let mut player = self.player.lock().expect("Cannot reenter");
                        player.render();
                        let renderer = player
                            .renderer_mut()
                            .downcast_mut::<WgpuRenderBackend<MovieView>>()
                            .expect("Renderer must be correct type");
                        self.gui.lock().expect("Gui lock").render(renderer.target());
                        #[cfg(feature = "tracy")]
                        tracing_tracy::client::Client::running()
                            .expect("tracy client must be running")
                            .frame_mark();
                    }
                }

                winit::event::Event::WindowEvent { event, .. } => {
                    if self.gui.lock().expect("Gui lock").handle_event(&event) {
                        // Event consumed by GUI.
                        return;
                    }
                    match event {
                        WindowEvent::CloseRequested => {
                            *control_flow = ControlFlow::Exit;
                            return;
                        }
                        WindowEvent::Resized(size) => {
                            // TODO: Change this when winit adds a `Window::minimzed` or `WindowEvent::Minimize`.
                            minimized = size.width == 0 && size.height == 0;

                            let viewport_scale_factor = self.window.scale_factor();
                            let mut player_lock = self.player.lock().expect("Cannot reenter");
                            player_lock.set_viewport_dimensions(ViewportDimensions {
                                width: size.width,
                                height: size.height,
                                scale_factor: viewport_scale_factor,
                            });
                            self.window.request_redraw();
                            if matches!(loaded, LoadingState::WaitingForResize) {
                                loaded = LoadingState::Loaded;
                            }
                        }
                        WindowEvent::CursorMoved { position, .. } => {
                            if self.gui.lock().expect("Gui lock").is_context_menu_visible() {
                                return;
                            }

                            let mut player_lock = self.player.lock().expect("Cannot reenter");
                            mouse_pos = position;
                            let event = PlayerEvent::MouseMove {
                                x: position.x,
                                y: position.y,
                            };
                            player_lock.handle_event(event);
                            check_redraw = true;
                        }
                        WindowEvent::MouseInput { button, state, .. } => {
                            if self.gui.lock().expect("Gui lock").is_context_menu_visible() {
                                return;
                            }

                            use ruffle_core::events::MouseButton as RuffleMouseButton;
                            let mut player_lock = self.player.lock().expect("Cannot reenter");
                            let x = mouse_pos.x;
                            let y = mouse_pos.y;
                            let button = match button {
                                MouseButton::Left => RuffleMouseButton::Left,
                                MouseButton::Right => RuffleMouseButton::Right,
                                MouseButton::Middle => RuffleMouseButton::Middle,
                                MouseButton::Other(_) => RuffleMouseButton::Unknown,
                            };
                            let event = match state {
                                ElementState::Pressed => PlayerEvent::MouseDown { x, y, button },
                                ElementState::Released => PlayerEvent::MouseUp { x, y, button },
                            };
                            if state == ElementState::Pressed && button == RuffleMouseButton::Right
                            {
                                // Show context menu.
                                // TODO: Should be squelched if player consumes the right click event.
                                let context_menu = player_lock.prepare_context_menu();
                                self.gui
                                    .lock()
                                    .expect("Gui lock")
                                    .show_context_menu(context_menu);
                            }
                            player_lock.handle_event(event);
                            check_redraw = true;
                        }
                        WindowEvent::MouseWheel { delta, .. } => {
                            use ruffle_core::events::MouseWheelDelta;
                            let mut player_lock = self.player.lock().expect("Cannot reenter");
                            let delta = match delta {
                                MouseScrollDelta::LineDelta(_, dy) => {
                                    MouseWheelDelta::Lines(dy.into())
                                }
                                MouseScrollDelta::PixelDelta(pos) => MouseWheelDelta::Pixels(pos.y),
                            };
                            let event = PlayerEvent::MouseWheel { delta };
                            player_lock.handle_event(event);
                            check_redraw = true;
                        }
                        WindowEvent::CursorEntered { .. } => {
                            let mut player_lock = self.player.lock().expect("Cannot reenter");
                            player_lock.set_mouse_in_stage(true);
                            if player_lock.needs_render() {
                                self.window.request_redraw();
                            }
                        }
                        WindowEvent::CursorLeft { .. } => {
                            let mut player_lock = self.player.lock().expect("Cannot reenter");
                            player_lock.set_mouse_in_stage(false);
                            player_lock.handle_event(PlayerEvent::MouseLeave);
                            check_redraw = true;
                        }
                        WindowEvent::ModifiersChanged(new_modifiers) => {
                            modifiers = new_modifiers;
                        }
                        WindowEvent::KeyboardInput { input, .. } => {
                            // Handle fullscreen keyboard shortcuts: Alt+Return, Escape.
                            match input {
                                KeyboardInput {
                                    state: ElementState::Pressed,
                                    virtual_keycode: Some(VirtualKeyCode::Return),
                                    ..
                                } if modifiers.alt() => {
                                    if !fullscreen_down {
                                        self.player.lock().expect("Cannot reenter").update(|uc| {
                                            uc.stage.toggle_display_state(uc);
                                        });
                                    }
                                    fullscreen_down = true;
                                    return;
                                }
                                KeyboardInput {
                                    state: ElementState::Released,
                                    virtual_keycode: Some(VirtualKeyCode::Return),
                                    ..
                                } if fullscreen_down => {
                                    fullscreen_down = false;
                                }
                                KeyboardInput {
                                    state: ElementState::Pressed,
                                    virtual_keycode: Some(VirtualKeyCode::Escape),
                                    ..
                                } => self.player.lock().expect("Cannot reenter").update(|uc| {
                                    uc.stage.set_display_state(uc, StageDisplayState::Normal);
                                }),
                                _ => (),
                            }

                            let mut player_lock = self.player.lock().expect("Cannot reenter");
                            if let Some(key) = input.virtual_keycode {
                                let key_code = winit_to_ruffle_key_code(key);
                                let key_char = winit_key_to_char(key, modifiers.shift());
                                match input.state {
                                    ElementState::Pressed => {
                                        player_lock.handle_event(PlayerEvent::KeyDown {
                                            key_code,
                                            key_char,
                                        });
                                        if let Some(control_code) =
                                            winit_to_ruffle_text_control(key, modifiers)
                                        {
                                            player_lock.handle_event(PlayerEvent::TextControl {
                                                code: control_code,
                                            });
                                        }
                                    }
                                    ElementState::Released => {
                                        player_lock.handle_event(PlayerEvent::KeyUp {
                                            key_code,
                                            key_char,
                                        });
                                    }
                                };
                                if player_lock.needs_render() {
                                    self.window.request_redraw();
                                }
                            }
                        }
                        WindowEvent::ReceivedCharacter(codepoint) => {
                            let mut player_lock = self.player.lock().expect("Cannot reenter");
                            let event = PlayerEvent::TextInput { codepoint };
                            player_lock.handle_event(event);
                            check_redraw = true;
                        }
                        _ => (),
                    }
                }
                winit::event::Event::UserEvent(RuffleEvent::TaskPoll) => self
                    .executor
                    .lock()
                    .expect("active executor reference")
                    .poll_all(),
                winit::event::Event::UserEvent(RuffleEvent::OnMetadata(swf_header)) => {
                    let movie_width = swf_header.stage_size().width().to_pixels();
                    let movie_height = swf_header.stage_size().height().to_pixels();

                    let window_size: Size = match (self.opt.width, self.opt.height) {
                        (None, None) => LogicalSize::new(movie_width, movie_height).into(),
                        (Some(width), None) => {
                            let scale = width / movie_width;
                            let height = movie_height * scale;
                            PhysicalSize::new(width.max(1.0), height.max(1.0)).into()
                        }
                        (None, Some(height)) => {
                            let scale = height / movie_height;
                            let width = movie_width * scale;
                            PhysicalSize::new(width.max(1.0), height.max(1.0)).into()
                        }
                        (Some(width), Some(height)) => {
                            PhysicalSize::new(width.max(1.0), height.max(1.0)).into()
                        }
                    };

                    let window_size = Size::clamp(
                        window_size,
                        self.min_window_size.into(),
                        self.max_window_size.into(),
                        self.window.scale_factor(),
                    );

                    self.window.set_inner_size(window_size);
                    self.window.set_fullscreen(if self.opt.fullscreen {
                        Some(Fullscreen::Borderless(None))
                    } else {
                        None
                    });
                    self.window.set_visible(true);

                    let viewport_size = self.window.inner_size();

                    // On X11 (and possibly other platforms), the window size is not updated immediately.
                    // Wait for the window to be resized to the requested size before we start running
                    // the SWF (which can observe the viewport size in "noScale" mode)
                    if window_size != viewport_size.into() {
                        loaded = LoadingState::WaitingForResize;
                    } else {
                        loaded = LoadingState::Loaded;
                    }

                    let viewport_scale_factor = self.window.scale_factor();
                    let mut player_lock = self.player.lock().expect("Cannot reenter");
                    player_lock.set_viewport_dimensions(ViewportDimensions {
                        width: viewport_size.width,
                        height: viewport_size.height,
                        scale_factor: viewport_scale_factor,
                    });
                }

                winit::event::Event::UserEvent(RuffleEvent::ContextMenuItemClicked(index)) => {
                    self.player
                        .lock()
                        .expect("Cannot reenter")
                        .run_context_menu_callback(index);
                }

                winit::event::Event::UserEvent(RuffleEvent::OpenFile) => {
                    if let Some(path) = pick_file() {
                        // TODO: Show dialog on error.
                        let url = parse_url(&path).expect("Couldn't load specified path");
                        let _ = self.load_swf(url);
                        self.gui.lock().expect("Gui lock").set_ui_visible(false);
                    }
                }

                winit::event::Event::UserEvent(RuffleEvent::OpenURL(url)) => {
                    let _ = self.load_swf(url);
                    self.gui.lock().expect("Gui lock").set_ui_visible(false);
                }

                winit::event::Event::UserEvent(RuffleEvent::ExitRequested) => {
                    *control_flow = ControlFlow::Exit;
                    return;
                }

                _ => (),
            }

            // Check for a redraw request.
            if check_redraw {
                let player = self.player.lock().expect("Player lock");
                let gui = self.gui.lock().expect("Gui lock");
                if player.needs_render() || gui.needs_render() {
                    self.window.request_redraw();
                }
            }

            // After polling events, sleep the event loop until the next event or the next frame.
            *control_flow = if matches!(loaded, LoadingState::Loaded) {
                ControlFlow::WaitUntil(next_frame_time)
            } else {
                ControlFlow::Wait
            };
        });
    }
}

/// Convert a winit `VirtualKeyCode` into a Ruffle `KeyCode`.
/// Return `KeyCode::Unknown` if there is no matching Flash key code.
fn winit_to_ruffle_key_code(key_code: VirtualKeyCode) -> KeyCode {
    match key_code {
        VirtualKeyCode::Back => KeyCode::Backspace,
        VirtualKeyCode::Tab => KeyCode::Tab,
        VirtualKeyCode::Return => KeyCode::Return,
        VirtualKeyCode::LShift | VirtualKeyCode::RShift => KeyCode::Shift,
        VirtualKeyCode::LControl | VirtualKeyCode::RControl => KeyCode::Control,
        VirtualKeyCode::LAlt | VirtualKeyCode::RAlt => KeyCode::Alt,
        VirtualKeyCode::Capital => KeyCode::CapsLock,
        VirtualKeyCode::Escape => KeyCode::Escape,
        VirtualKeyCode::Space => KeyCode::Space,
        VirtualKeyCode::Key0 => KeyCode::Key0,
        VirtualKeyCode::Key1 => KeyCode::Key1,
        VirtualKeyCode::Key2 => KeyCode::Key2,
        VirtualKeyCode::Key3 => KeyCode::Key3,
        VirtualKeyCode::Key4 => KeyCode::Key4,
        VirtualKeyCode::Key5 => KeyCode::Key5,
        VirtualKeyCode::Key6 => KeyCode::Key6,
        VirtualKeyCode::Key7 => KeyCode::Key7,
        VirtualKeyCode::Key8 => KeyCode::Key8,
        VirtualKeyCode::Key9 => KeyCode::Key9,
        VirtualKeyCode::A => KeyCode::A,
        VirtualKeyCode::B => KeyCode::B,
        VirtualKeyCode::C => KeyCode::C,
        VirtualKeyCode::D => KeyCode::D,
        VirtualKeyCode::E => KeyCode::E,
        VirtualKeyCode::F => KeyCode::F,
        VirtualKeyCode::G => KeyCode::G,
        VirtualKeyCode::H => KeyCode::H,
        VirtualKeyCode::I => KeyCode::I,
        VirtualKeyCode::J => KeyCode::J,
        VirtualKeyCode::K => KeyCode::K,
        VirtualKeyCode::L => KeyCode::L,
        VirtualKeyCode::M => KeyCode::M,
        VirtualKeyCode::N => KeyCode::N,
        VirtualKeyCode::O => KeyCode::O,
        VirtualKeyCode::P => KeyCode::P,
        VirtualKeyCode::Q => KeyCode::Q,
        VirtualKeyCode::R => KeyCode::R,
        VirtualKeyCode::S => KeyCode::S,
        VirtualKeyCode::T => KeyCode::T,
        VirtualKeyCode::U => KeyCode::U,
        VirtualKeyCode::V => KeyCode::V,
        VirtualKeyCode::W => KeyCode::W,
        VirtualKeyCode::X => KeyCode::X,
        VirtualKeyCode::Y => KeyCode::Y,
        VirtualKeyCode::Z => KeyCode::Z,
        VirtualKeyCode::Semicolon => KeyCode::Semicolon,
        VirtualKeyCode::Equals => KeyCode::Equals,
        VirtualKeyCode::Comma => KeyCode::Comma,
        VirtualKeyCode::Minus => KeyCode::Minus,
        VirtualKeyCode::Period => KeyCode::Period,
        VirtualKeyCode::Slash => KeyCode::Slash,
        VirtualKeyCode::Grave => KeyCode::Grave,
        VirtualKeyCode::LBracket => KeyCode::LBracket,
        VirtualKeyCode::Backslash => KeyCode::Backslash,
        VirtualKeyCode::RBracket => KeyCode::RBracket,
        VirtualKeyCode::Apostrophe => KeyCode::Apostrophe,
        VirtualKeyCode::Numpad0 => KeyCode::Numpad0,
        VirtualKeyCode::Numpad1 => KeyCode::Numpad1,
        VirtualKeyCode::Numpad2 => KeyCode::Numpad2,
        VirtualKeyCode::Numpad3 => KeyCode::Numpad3,
        VirtualKeyCode::Numpad4 => KeyCode::Numpad4,
        VirtualKeyCode::Numpad5 => KeyCode::Numpad5,
        VirtualKeyCode::Numpad6 => KeyCode::Numpad6,
        VirtualKeyCode::Numpad7 => KeyCode::Numpad7,
        VirtualKeyCode::Numpad8 => KeyCode::Numpad8,
        VirtualKeyCode::Numpad9 => KeyCode::Numpad9,
        VirtualKeyCode::NumpadMultiply => KeyCode::Multiply,
        VirtualKeyCode::NumpadAdd => KeyCode::Plus,
        VirtualKeyCode::NumpadSubtract => KeyCode::NumpadMinus,
        VirtualKeyCode::NumpadDecimal => KeyCode::NumpadPeriod,
        VirtualKeyCode::NumpadDivide => KeyCode::NumpadSlash,
        VirtualKeyCode::PageUp => KeyCode::PgUp,
        VirtualKeyCode::PageDown => KeyCode::PgDown,
        VirtualKeyCode::End => KeyCode::End,
        VirtualKeyCode::Home => KeyCode::Home,
        VirtualKeyCode::Left => KeyCode::Left,
        VirtualKeyCode::Up => KeyCode::Up,
        VirtualKeyCode::Right => KeyCode::Right,
        VirtualKeyCode::Down => KeyCode::Down,
        VirtualKeyCode::Insert => KeyCode::Insert,
        VirtualKeyCode::Delete => KeyCode::Delete,
        VirtualKeyCode::Pause => KeyCode::Pause,
        VirtualKeyCode::Scroll => KeyCode::ScrollLock,
        VirtualKeyCode::F1 => KeyCode::F1,
        VirtualKeyCode::F2 => KeyCode::F2,
        VirtualKeyCode::F3 => KeyCode::F3,
        VirtualKeyCode::F4 => KeyCode::F4,
        VirtualKeyCode::F5 => KeyCode::F5,
        VirtualKeyCode::F6 => KeyCode::F6,
        VirtualKeyCode::F7 => KeyCode::F7,
        VirtualKeyCode::F8 => KeyCode::F8,
        VirtualKeyCode::F9 => KeyCode::F9,
        VirtualKeyCode::F10 => KeyCode::F10,
        VirtualKeyCode::F11 => KeyCode::F11,
        VirtualKeyCode::F12 => KeyCode::F12,
        _ => KeyCode::Unknown,
    }
}

/// Return a character for the given key code and shift state.
fn winit_key_to_char(key_code: VirtualKeyCode, is_shift_down: bool) -> Option<char> {
    // We need to know the character that a keypress outputs for both key down and key up events,
    // but the winit keyboard API does not provide a way to do this (winit/#753).
    // CharacterReceived events are insufficent because they only fire on key down, not on key up.
    // This is a half-measure to map from keyboard keys back to a character, but does will not work fully
    // for international layouts.
    Some(match (key_code, is_shift_down) {
        (VirtualKeyCode::Space, _) => ' ',
        (VirtualKeyCode::Key0, _) => '0',
        (VirtualKeyCode::Key1, _) => '1',
        (VirtualKeyCode::Key2, _) => '2',
        (VirtualKeyCode::Key3, _) => '3',
        (VirtualKeyCode::Key4, _) => '4',
        (VirtualKeyCode::Key5, _) => '5',
        (VirtualKeyCode::Key6, _) => '6',
        (VirtualKeyCode::Key7, _) => '7',
        (VirtualKeyCode::Key8, _) => '8',
        (VirtualKeyCode::Key9, _) => '9',
        (VirtualKeyCode::A, false) => 'a',
        (VirtualKeyCode::A, true) => 'A',
        (VirtualKeyCode::B, false) => 'b',
        (VirtualKeyCode::B, true) => 'B',
        (VirtualKeyCode::C, false) => 'c',
        (VirtualKeyCode::C, true) => 'C',
        (VirtualKeyCode::D, false) => 'd',
        (VirtualKeyCode::D, true) => 'D',
        (VirtualKeyCode::E, false) => 'e',
        (VirtualKeyCode::E, true) => 'E',
        (VirtualKeyCode::F, false) => 'f',
        (VirtualKeyCode::F, true) => 'F',
        (VirtualKeyCode::G, false) => 'g',
        (VirtualKeyCode::G, true) => 'G',
        (VirtualKeyCode::H, false) => 'h',
        (VirtualKeyCode::H, true) => 'H',
        (VirtualKeyCode::I, false) => 'i',
        (VirtualKeyCode::I, true) => 'I',
        (VirtualKeyCode::J, false) => 'j',
        (VirtualKeyCode::J, true) => 'J',
        (VirtualKeyCode::K, false) => 'k',
        (VirtualKeyCode::K, true) => 'K',
        (VirtualKeyCode::L, false) => 'l',
        (VirtualKeyCode::L, true) => 'L',
        (VirtualKeyCode::M, false) => 'm',
        (VirtualKeyCode::M, true) => 'M',
        (VirtualKeyCode::N, false) => 'n',
        (VirtualKeyCode::N, true) => 'N',
        (VirtualKeyCode::O, false) => 'o',
        (VirtualKeyCode::O, true) => 'O',
        (VirtualKeyCode::P, false) => 'p',
        (VirtualKeyCode::P, true) => 'P',
        (VirtualKeyCode::Q, false) => 'q',
        (VirtualKeyCode::Q, true) => 'Q',
        (VirtualKeyCode::R, false) => 'r',
        (VirtualKeyCode::R, true) => 'R',
        (VirtualKeyCode::S, false) => 's',
        (VirtualKeyCode::S, true) => 'S',
        (VirtualKeyCode::T, false) => 't',
        (VirtualKeyCode::T, true) => 'T',
        (VirtualKeyCode::U, false) => 'u',
        (VirtualKeyCode::U, true) => 'U',
        (VirtualKeyCode::V, false) => 'v',
        (VirtualKeyCode::V, true) => 'V',
        (VirtualKeyCode::W, false) => 'w',
        (VirtualKeyCode::W, true) => 'W',
        (VirtualKeyCode::X, false) => 'x',
        (VirtualKeyCode::X, true) => 'X',
        (VirtualKeyCode::Y, false) => 'y',
        (VirtualKeyCode::Y, true) => 'Y',
        (VirtualKeyCode::Z, false) => 'z',
        (VirtualKeyCode::Z, true) => 'Z',

        (VirtualKeyCode::Semicolon, false) => ';',
        (VirtualKeyCode::Semicolon, true) => ':',
        (VirtualKeyCode::Equals, false) => '=',
        (VirtualKeyCode::Equals, true) => '+',
        (VirtualKeyCode::Comma, false) => ',',
        (VirtualKeyCode::Comma, true) => '<',
        (VirtualKeyCode::Minus, false) => '-',
        (VirtualKeyCode::Minus, true) => '_',
        (VirtualKeyCode::Period, false) => '.',
        (VirtualKeyCode::Period, true) => '>',
        (VirtualKeyCode::Slash, false) => '/',
        (VirtualKeyCode::Slash, true) => '?',
        (VirtualKeyCode::Grave, false) => '`',
        (VirtualKeyCode::Grave, true) => '~',
        (VirtualKeyCode::LBracket, false) => '[',
        (VirtualKeyCode::LBracket, true) => '{',
        (VirtualKeyCode::Backslash, false) => '\\',
        (VirtualKeyCode::Backslash, true) => '|',
        (VirtualKeyCode::RBracket, false) => ']',
        (VirtualKeyCode::RBracket, true) => '}',
        (VirtualKeyCode::Apostrophe, false) => '\'',
        (VirtualKeyCode::Apostrophe, true) => '"',
        (VirtualKeyCode::NumpadMultiply, _) => '*',
        (VirtualKeyCode::NumpadAdd, _) => '+',
        (VirtualKeyCode::NumpadSubtract, _) => '-',
        (VirtualKeyCode::NumpadDecimal, _) => '.',
        (VirtualKeyCode::NumpadDivide, _) => '/',

        (VirtualKeyCode::Numpad0, false) => '0',
        (VirtualKeyCode::Numpad1, false) => '1',
        (VirtualKeyCode::Numpad2, false) => '2',
        (VirtualKeyCode::Numpad3, false) => '3',
        (VirtualKeyCode::Numpad4, false) => '4',
        (VirtualKeyCode::Numpad5, false) => '5',
        (VirtualKeyCode::Numpad6, false) => '6',
        (VirtualKeyCode::Numpad7, false) => '7',
        (VirtualKeyCode::Numpad8, false) => '8',
        (VirtualKeyCode::Numpad9, false) => '9',
        (VirtualKeyCode::NumpadEnter, _) => '\r',

        (VirtualKeyCode::Tab, _) => '\t',
        (VirtualKeyCode::Return, _) => '\r',
        (VirtualKeyCode::Back, _) => '\u{0008}',

        _ => return None,
    })
}

/// Converts a `VirtualKeyCode` and `ModifiersState` to a Ruffle `TextControlCode`.
/// Returns `None` if there is no match.
/// TODO: Handle Ctrl+Arrows and Home/End keys
fn winit_to_ruffle_text_control(
    key: VirtualKeyCode,
    modifiers: ModifiersState,
) -> Option<TextControlCode> {
    let shift = modifiers.contains(ModifiersState::SHIFT);
    let ctrl_cmd = modifiers.contains(ModifiersState::CTRL)
        || (modifiers.contains(ModifiersState::LOGO) && cfg!(target_os = "macos"));
    if ctrl_cmd {
        match key {
            VirtualKeyCode::A => Some(TextControlCode::SelectAll),
            VirtualKeyCode::C => Some(TextControlCode::Copy),
            VirtualKeyCode::V => Some(TextControlCode::Paste),
            VirtualKeyCode::X => Some(TextControlCode::Cut),
            _ => None,
        }
    } else {
        match key {
            VirtualKeyCode::Back => Some(TextControlCode::Backspace),
            VirtualKeyCode::Delete => Some(TextControlCode::Delete),
            VirtualKeyCode::Left => {
                if shift {
                    Some(TextControlCode::SelectLeft)
                } else {
                    Some(TextControlCode::MoveLeft)
                }
            }
            VirtualKeyCode::Right => {
                if shift {
                    Some(TextControlCode::SelectRight)
                } else {
                    Some(TextControlCode::MoveRight)
                }
            }
            _ => None,
        }
    }
}

fn run_timedemo(opt: Opt) -> Result<(), Error> {
    let path = opt
        .input_path
        .as_ref()
        .ok_or_else(|| anyhow!("Input file necessary for timedemo"))?;
    let movie_url = parse_url(path)?;
    let movie = load_movie(&movie_url, &opt).context("Couldn't load movie")?;
    let movie_frames = Some(movie.num_frames());

    let viewport_width = 1920;
    let viewport_height = 1080;
    let viewport_scale_factor = 1.0;

    let renderer = WgpuRenderBackend::for_offscreen(
        (viewport_width, viewport_height),
        opt.graphics.into(),
        opt.power.into(),
        trace_path(&opt),
    )
    .map_err(|e| anyhow!(e.to_string()))
    .context("Couldn't create wgpu rendering backend")?;

    let mut builder = PlayerBuilder::new();

    if cfg!(feature = "software_video") {
        builder = builder.with_video(ruffle_video_software::backend::SoftwareVideoBackend::new());
    }

    let player = builder
        .with_renderer(renderer)
        .with_movie(movie)
        .with_viewport_dimensions(viewport_width, viewport_height, viewport_scale_factor)
        .with_autoplay(true)
        .build();

    let mut player_lock = player.lock().expect("Cannot reenter");

    println!("Running {}...", path.to_string_lossy());

    let start = Instant::now();
    let mut num_frames = 0;
    const MAX_FRAMES: u32 = 5000;
    while num_frames < MAX_FRAMES && player_lock.current_frame() < movie_frames {
        player_lock.run_frame();
        player_lock.render();
        num_frames += 1;
    }
    let end = Instant::now();
    let duration = end.duration_since(start);

    println!("Ran {num_frames} frames in {}s.", duration.as_secs_f32());

    Ok(())
}

fn init() {
    // When linked with the windows subsystem windows won't automatically attach
    // to the console of the parent process, so we do it explicitly. This fails
    // silently if the parent has no console.
    #[cfg(windows)]
    unsafe {
        use winapi::um::wincon::{AttachConsole, ATTACH_PARENT_PROCESS};
        AttachConsole(ATTACH_PARENT_PROCESS);
    }

    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        prev_hook(info);
        panic_hook(info);
    }));

    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .finish();
    #[cfg(feature = "tracy")]
    let subscriber = {
        use tracing_subscriber::layer::SubscriberExt;
        let tracy_subscriber = tracing_tracy::TracyLayer::new();
        subscriber.with(tracy_subscriber)
    };
    tracing::subscriber::set_global_default(subscriber).expect("Couldn't set up global subscriber");
}

fn panic_hook(info: &PanicInfo) {
    CALLSTACK.with(|callstack| {
        if let Some(callstack) = &*callstack.borrow() {
            callstack.avm2(|callstack| println!("AVM2 stack trace: {callstack}"))
        }
    });

    // [NA] Let me just point out that PanicInfo::message() exists but isn't stable and that sucks.
    let panic_text = info.to_string();
    let message = if let Some(text) = panic_text.strip_prefix("panicked at '") {
        let location = info.location().map(|l| l.to_string()).unwrap_or_default();
        if let Some(text) = text.strip_suffix(&format!("', {location}")) {
            text.trim()
        } else {
            text.trim()
        }
    } else {
        panic_text.trim()
    };
    if rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("Ruffle")
        .set_description(&format!(
            "Ruffle has encountered a fatal error, this is a bug.\n\n\
            {message}\n\n\
            Please report this to us so that we can fix it. Thank you!\n\
            Pressing Yes will open a browser window."
        ))
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
    {
        let mut params = vec![
            ("panic_text", info.to_string()),
            ("platform", "Desktop app".to_string()),
            ("operating_system", os_info::get().to_string()),
            ("ruffle_version", RUFFLE_VERSION.to_string()),
        ];
        let mut extra_info = vec![];
        SWF_INFO.with(|i| {
            if let Some(swf_name) = i.take() {
                extra_info.push(format!("Filename: {swf_name}\n"));
                params.push(("title", format!("Crash on {swf_name}")));
            }
        });
        CALLSTACK.with(|callstack| {
            if let Some(callstack) = &*callstack.borrow() {
                callstack.avm2(|callstack| {
                    extra_info.push(format!("### AVM2 Callstack\n```{callstack}\n```\n"));
                });
            }
        });
        RENDER_INFO.with(|i| {
            if let Some(render_info) = i.take() {
                extra_info.push(format!("### Render Info\n{render_info}\n"));
            }
        });
        if !extra_info.is_empty() {
            params.push(("extra_info", extra_info.join("\n")));
        }
        if let Ok(url) = Url::parse_with_params("https://github.com/ruffle-rs/ruffle/issues/new?assignees=&labels=bug&template=crash_report.yml", &params) {
            let _ = webbrowser::open(url.as_str());
        }
    }
}

fn shutdown() {
    // Without explicitly detaching the console cmd won't redraw it's prompt.
    #[cfg(windows)]
    unsafe {
        winapi::um::wincon::FreeConsole();
    }
}

fn main() -> Result<(), Error> {
    init();
    let opt = Opt::parse();
    let result = if opt.timedemo {
        run_timedemo(opt)
    } else {
        App::new(opt).map(|app| app.run())
    };
    #[cfg(windows)]
    if let Err(error) = &result {
        eprintln!("{:?}", error)
    }
    shutdown();
    result
}
