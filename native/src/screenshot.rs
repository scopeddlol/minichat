//! Headless rendering, for looking at the UI without a display.
//!
//! `minichat-native --screenshot <file.png>` paints one frame through Slint's
//! software renderer and writes it out. The point is to be able to review a
//! layout change the way a person would see it — in CI, over SSH, or on a
//! build machine with no compositor — rather than by reading the markup and
//! hoping.
//!
//! The software renderer is not what ships (Skia is), so this is a preview of
//! layout, colour and type rather than a pixel-exact capture of the release
//! build. Everything it gets wrong, it gets wrong consistently.

use std::rc::Rc;

use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType, TargetPixel,
};
use slint::platform::{Platform, WindowAdapter};
use slint::PhysicalSize;

struct Headless {
    window: Rc<MinimalSoftwareWindow>,
}

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }
}

/// Render one frame of `build()`'s window at `width` × `height` and write a
/// PNG to `path`.
pub fn capture<T>(
    path: &str,
    width: u32,
    height: u32,
    build: impl FnOnce() -> Result<T, slint::PlatformError>,
) -> Result<(), Box<dyn std::error::Error>>
where
    T: slint::ComponentHandle,
{
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless {
        window: window.clone(),
    }))
    .map_err(|e| format!("the platform was already set: {e}"))?;

    let component = build()?;
    component.show()?;
    window.set_size(PhysicalSize::new(width, height));

    // Two passes: the first settles any binding that depends on a measured
    // size (wrapped text, flex children), the second paints the result.
    let mut buffer = vec![
        Rgba8 {
            r: 0,
            g: 0,
            b: 0,
            a: 255
        };
        (width * height) as usize
    ];
    for _ in 0..2 {
        slint::platform::update_timers_and_animations();
        window.draw_if_needed(|renderer| {
            renderer.render(&mut buffer, width as usize);
        });
        window.request_redraw();
    }

    let mut rgb = image::RgbImage::new(width, height);
    for (index, pixel) in buffer.iter().enumerate() {
        rgb.put_pixel(
            index as u32 % width,
            index as u32 / width,
            image::Rgb([pixel.r, pixel.g, pixel.b]),
        );
    }
    rgb.save(path)?;
    Ok(())
}

/// An 8-bit-per-channel target, so a preview shows the gradients the release
/// renderer will show rather than the banding a 16-bit buffer invents.
#[derive(Clone, Copy, Default, PartialEq)]
#[repr(C)]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl TargetPixel for Rgba8 {
    fn blend(&mut self, colour: PremultipliedRgbaColor) {
        let inverse = 255 - colour.alpha as u32;
        let channel = |source: u8, existing: u8| {
            (source as u32 + (existing as u32 * inverse) / 255).min(255) as u8
        };
        self.r = channel(colour.red, self.r);
        self.g = channel(colour.green, self.g);
        self.b = channel(colour.blue, self.b);
        self.a = 255;
    }

    fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}
