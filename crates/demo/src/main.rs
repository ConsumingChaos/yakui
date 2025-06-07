use std::future::Future;
use std::pin::Pin;

use futures::future::FutureExt;
use winit::application::ApplicationHandler;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowAttributes;
use winit::window::WindowId;
use winit::{event::WindowEvent, event_loop::EventLoop, window::Window};
use yakui::{button, row, widgets::List, CrossAxisAlignment, Yakui};
use yakui_app::Graphics;

struct DemoApp;

impl App for DemoApp {
    fn render(&mut self) {
        row(|| {
            button("Not stretched");
            let mut col = List::column();
            col.cross_axis_alignment = CrossAxisAlignment::Stretch;
            col.show(|| {
                button("Button 1");
                button("Button 2");
                button("Button 3");
            });
        });
    }
}

trait App {
    fn render(&mut self);
}

struct AppHandler<T: App> {
    app: T,
    yak: Yakui,

    window: Option<Box<dyn Window>>,
    graphics: GraphicsState,

    // https://github.com/rust-windowing/winit/issues/3406
    #[cfg(target_os = "ios")]
    redraw_requested: bool,
}

enum GraphicsState {
    None,
    Pending(Pin<Box<dyn Future<Output = Graphics>>>),
    Ready(Graphics),
}

impl<T: App> AppHandler<T> {
    fn new(app: T) -> Self {
        Self {
            app: app,
            yak: Yakui::new(),

            window: None,
            graphics: GraphicsState::None,

            #[cfg(target_os = "ios")]
            redraw_requested: false,
        }
    }
}

impl<T: App> ApplicationHandler for AppHandler<T> {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        self.window = Some(
            event_loop
                .create_window(WindowAttributes::default())
                .unwrap(),
        );

        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowExtWebSys;

            // On wasm, append the canvas to the document body
            web_sys::window()
                .and_then(|win| win.document())
                .and_then(|doc| doc.body())
                .and_then(|body| {
                    body.append_child(&web_sys::Element::from(
                        self.window.as_ref().unwrap().canvas().unwrap(),
                    ))
                    .ok()
                })
                .expect("couldn't append canvas to document body");
        }
    }

    fn destroy_surfaces(&mut self, _event_loop: &dyn ActiveEventLoop) {
        self.graphics = GraphicsState::None;
        self.window = None;
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let window = match self.window.as_deref() {
            Some(window) => window,
            None => return,
        };

        // This is going to drop WindowEvents...
        if let GraphicsState::Ready(graphics) = &mut self.graphics {
            if graphics.handle_window_event(&mut self.yak, &event, event_loop) {
                return;
            }
        }

        match event {
            WindowEvent::RedrawRequested => {
                // Graphics State
                if let GraphicsState::None = &self.graphics {
                    // SAFETY: Future will outlive `window`.
                    let window = unsafe {
                        std::mem::transmute::<&'_ dyn Window, &'static dyn Window>(window)
                    };

                    self.graphics = GraphicsState::Pending(Box::pin(Graphics::new(window, 4)));
                }

                if let GraphicsState::Pending(task) = &mut self.graphics {
                    if let Some(graphics) = task.now_or_never() {
                        self.graphics = GraphicsState::Ready(graphics);
                    }
                }

                // Render
                if let GraphicsState::Ready(graphics) = &mut self.graphics {
                    self.yak.start();
                    self.app.render();
                    self.yak.finish();

                    graphics.paint(&mut self.yak, wgpu::Color::BLACK);
                }

                // Request Redraw
                window.request_redraw();

                #[cfg(target_os = "ios")]
                {
                    self.redraw_requested = true;
                }
            }
            _ => (),
        }
    }

    fn about_to_wait(&mut self, _event_loop: &dyn ActiveEventLoop) {
        #[cfg(target_os = "ios")]
        if self.redraw_requested {
            if let Some(window_context) = self.window_context.as_ref() {
                window_context.window.request_redraw();
            }
            self.redraw_requested = false;
        }
    }
}

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
        EventLoop::new()
            .unwrap()
            .run_app(AppHandler::new(DemoApp))
            .unwrap();
    }

    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");

        EventLoop::new()
            .unwrap()
            .spawn_app(AppHandler::new(DemoApp))
            .unwrap();
    }
}
