#![windows_subsystem = "windows"]

extern crate cgmath;
#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate glium;
extern crate image;
extern crate sys_info;
extern crate backtrace;

use std::env;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::cell::RefCell;
use std::thread;
use std::time::Duration;

use glium::glutin::{VirtualKeyCode, WindowEvent};
use glium::{glutin, Surface};
use glium::texture::{RawImage2d, SrgbTexture2d};

use cgmath::Vector2;

mod image_cache;
mod handle_panic;
mod ui;
mod shaders;

mod picture_panel;
use picture_panel::PicturePanel;

mod playback_manager;
use playback_manager::{PlaybackManager, LoadRequest};

mod window;
use window::*;


// ========================================================
// Glorious main function
// ========================================================
fn main() {
    use std::panic;
    use std::boxed::Box;

    panic::set_hook(Box::new(handle_panic::handle_panic));

    Program::start();
}
// ========================================================


fn load_texture_without_cache(
    display: &glium::Display,
    image_path: &Path,
) -> SrgbTexture2d {
    let image = image::open(image_path).unwrap().to_rgba();

    texture_from_image(display, image)
}

fn texture_from_image(
    display: &glium::Display,
    image: image::RgbaImage,
) -> SrgbTexture2d {
    let image_dimensions = image.dimensions();
    let image = RawImage2d::from_raw_rgba(image.into_raw(), image_dimensions);

    SrgbTexture2d::with_mipmaps(
        display,
        image,
        glium::texture::MipmapsOption::NoMipmap,
    ).unwrap()
}


trait OptionRefClone {
    fn ref_clone(&self) -> Self;
}

impl OptionRefClone for Option<Rc<glium::texture::SrgbTexture2d>> {
    fn ref_clone(&self) -> Option<Rc<glium::texture::SrgbTexture2d>> {
        match *self {
            Some(ref image) => Some(image.clone()),
            None => None,
        }
    }
}


struct Program<'a> {
    bottom_panel_height: f64,
    window: &'a mut Window,
    picture_panel: &'a mut PicturePanel,
    playback_manager: &'a RefCell<PlaybackManager>,
    ui: ui::Ui<'a>,
}

impl<'a> Program<'a> {
    fn draw_picture(window: &mut Window, picture_controller: &mut PicturePanel) {
        let mut target = window.display().draw();

        target.clear_color(0.9, 0.9, 0.9, 0.0);
        picture_controller.draw(&mut target, window);
        target.finish().unwrap();
    }

    fn start() {
        let bottom_panel_height = 32;

        let mut events_loop = glutin::EventsLoop::new();
        let mut window = Window::init(&events_loop);
        let mut picture_panel = PicturePanel::new(window.display(), bottom_panel_height);
        let playback_manager = RefCell::new(PlaybackManager::new());

        // Load image
        if let Some(img_path) = env::args().skip(1).next() {
            let img_path = PathBuf::from(img_path);
            let mut playback_manager = playback_manager.borrow_mut();
            playback_manager.request_load(LoadRequest::LoadSpecific(img_path));
            playback_manager.update_image(&mut window);
            picture_panel.set_image(playback_manager.image_texture().ref_clone());
        } else {
            window.set_title_filename("Drag and drop an image on the window.");
        }

        // Just quickly display the loaded image here before we load the remaining parts of the program
        Self::draw_picture(&mut window, &mut picture_panel);
        
        let mut ui = ui::Ui::new(window.display());
        
        Self::init_ui(&mut ui, &mut window, &playback_manager);

        let mut program = Program {
            bottom_panel_height: bottom_panel_height as f64,
            window: &mut window,
            picture_panel: &mut picture_panel,
            playback_manager: &playback_manager,
            ui: ui,
        };

        program.start_event_loop(&mut events_loop);
    }

    fn init_ui<'b>(
        ui: &mut ui::Ui<'b>,
        window: &mut Window,
        playback_manager: &'b RefCell<PlaybackManager>,
    ) {
        let exe_parent = std::env::current_exe().unwrap().parent().unwrap().to_owned();
        let button_texture = Rc::new(
            load_texture_without_cache(
                window.display(),
                &exe_parent.join("cogs.png")
            )
        );
        let light_texture = Rc::new(
            load_texture_without_cache(
                window.display(),
                &exe_parent.join("light.png")
            )
        );
        let moon_texture = Rc::new(
            load_texture_without_cache(
                window.display(),
                &exe_parent.join("moon.png")
            )
        );

        let button = ui.create_button(button_texture, Vector2::new(32f32, 4f32), Box::new(||()));
        {
            if let Some(button) = ui.get_button_mut(button) {
                button.set_callback(Box::new(move || {
                    playback_manager.borrow_mut().request_load(LoadRequest::LoadNext);
                }));
            }
        }
        let _ = ui.create_toggle(moon_texture, light_texture, Vector2::new(4f32, 4f32), true, Box::new(move |is_light| {
            playback_manager.borrow_mut().request_load(LoadRequest::LoadNext);
        }));
    }


    fn start_event_loop(&mut self, events_loop: &mut glutin::EventsLoop) {
        let mut running = true;
        let mut mouse_y = 0f64;
        // the main loop
        while running {
            events_loop.poll_events(|event| {
                use glutin::Event;
                if let Event::WindowEvent { ref event, .. } = event {
                    match event {
                        // Break from the main loop when the window is closed.
                        WindowEvent::CloseRequested => running = false,
                        WindowEvent::KeyboardInput { input, .. } => {
                            if let Some(keycode) = input.virtual_keycode {
                                if input.state == glutin::ElementState::Pressed {
                                    if keycode == VirtualKeyCode::Escape {
                                        running = false
                                    }
                                }
                            }
                        },
                        WindowEvent::CursorMoved { position, .. } => {
                            mouse_y = position.y;
                        }
                        _ => (),
                    }
                }

                // Pre events
                self.picture_panel.pre_events();

                // Dispatch event
                let window_size = self.window.display().gl_window().get_inner_size().unwrap();
                match event {
                    Event::WindowEvent {event: WindowEvent::MouseInput {..}, ..} => {
                        if mouse_y < (window_size.height - self.bottom_panel_height) {
                            self.picture_panel.handle_event(&event, &mut self.window, &mut self.playback_manager.borrow_mut());
                        } else {
                            if let Event::WindowEvent { ref event, .. } = event {
                                let window_size = self.window.display().gl_window().get_inner_size().unwrap();
                                self.ui.window_event(&event, window_size);
                            }
                        }
                    }
                    _ => {
                        if let Event::WindowEvent { ref event, .. } = event {
                            self.ui.window_event(&event, window_size);
                        }
                        self.picture_panel.handle_event(&event, &mut self.window, &mut self.playback_manager.borrow_mut());
                    }
                }
                

                // Update screen after a resize event or refresh
                if let Event::WindowEvent { event, .. } = event {
                    match event {
                        WindowEvent::Resized(..) | WindowEvent::Refresh => self.draw(),
                        _ => (),
                    }
                }
            });

            let load_requested = {
                let mut playback_manager = self.playback_manager.borrow_mut();
                playback_manager.update_image(&mut self.window);
                self.picture_panel.set_image(playback_manager.image_texture().ref_clone());

                *playback_manager.load_request() != LoadRequest::None
            };
            self.draw();

            let mut playback_manager = self.playback_manager.borrow_mut();
            // Update dirctory after draw
            if load_requested {
                playback_manager.update_directory().unwrap();
            }

            let should_sleep = {
                playback_manager.should_sleep()
                && self.picture_panel.should_sleep()
                && !load_requested
            };

            // Let other processes run for a bit.
            //thread::yield_now();
            if should_sleep {
                thread::sleep(Duration::from_millis(1));
            }
        }
    }

    fn draw(&mut self) {
        let mut target = self.window.display().draw();

        target.clear_color(0.9, 0.9, 0.9, 0.0);

        self.picture_panel.draw(&mut target, &self.window);
        self.ui.draw(&mut target);

        target.finish().unwrap();
    }
}

