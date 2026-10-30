//! Override-redirect surfaces bypass the window manager's focus-on-map policy.
use winit::platform::x11::EventLoopBuilderExtX11;

pub fn configure(options: &mut eframe::NativeOptions) {
    // AT-SPI and RandR use X11 physical pixels. Pin winit's scale so viewport commands use
    // exactly the same coordinates on mixed-DPI desktops; egui zoom remains one as well.
    std::env::set_var("WINIT_X11_SCALE_FACTOR", "1");
    options.event_loop_builder = Some(Box::new(|builder| { builder.with_x11(); }));
    options.window_builder = Some(Box::new(|attributes| {
        attributes.with_override_redirect(true)
            .with_window_type(eframe::egui::viewport::X11WindowType::Tooltip)
    }));
}
