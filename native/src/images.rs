//! Avatars, custom emoji and image attachments.
//!
//! Everything the instance serves is fetched once and kept as a decoded
//! `slint::Image`. The cache is bounded: an instance decides what it serves,
//! so it must not be able to decide how much memory the client spends. A
//! miss renders the fallback (initials, or a file card) rather than blocking
//! a repaint, and the fetch happens off the UI thread.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

/// The largest single image that will be decoded, in bytes.
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
/// The largest number of decoded images kept at once. An avatar at 36px is a
/// few KB; an attachment preview is larger, so this is deliberately modest.
const MAX_ENTRIES: usize = 512;
/// Attachment previews are scaled down to this before decoding is kept, so a
/// 6000px photo does not sit in memory at full size for a 400px preview.
pub const MAX_PREVIEW: u32 = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Missing,
    Loading,
    Ready,
    /// Fetched or decoded and failed. Never retried on its own, so a broken
    /// URL costs one request rather than one per repaint.
    Failed,
}

#[derive(Default)]
struct Inner {
    images: HashMap<String, Image>,
    state: HashMap<String, State>,
    /// Insertion order, for evicting the oldest when the cache is full.
    order: Vec<String>,
}

#[derive(Clone, Default)]
pub struct Cache {
    inner: Rc<RefCell<Inner>>,
}

impl Cache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, url: &str) -> Option<Image> {
        if url.is_empty() {
            return None;
        }
        self.inner.borrow().images.get(url).cloned()
    }

    /// The image if it is loaded, or an empty one. The UI treats a zero-width
    /// image as "show the fallback", so this never needs an Option in markup.
    pub fn get_or_blank(&self, url: &str) -> Image {
        self.get(url).unwrap_or_default()
    }

    pub fn state(&self, url: &str) -> State {
        if url.is_empty() {
            return State::Missing;
        }
        self.inner
            .borrow()
            .state
            .get(url)
            .copied()
            .unwrap_or(State::Missing)
    }

    /// Claim a URL for fetching. Returns false when it is already loaded, in
    /// flight, or known bad — which is what stops a repaint from queueing the
    /// same avatar a hundred times.
    pub fn claim(&self, url: &str) -> bool {
        if url.is_empty() {
            return false;
        }
        let mut inner = self.inner.borrow_mut();
        match inner.state.get(url) {
            Some(State::Loading | State::Ready | State::Failed) => false,
            _ => {
                inner.state.insert(url.to_string(), State::Loading);
                true
            }
        }
    }

    pub fn fail(&self, url: &str) {
        self.inner
            .borrow_mut()
            .state
            .insert(url.to_string(), State::Failed);
    }

    /// Decode fetched bytes and store the result.
    pub fn insert(&self, url: &str, bytes: &[u8], max_side: u32) -> bool {
        let Ok(decoded) = image::load_from_memory(bytes) else {
            self.fail(url);
            return false;
        };

        // Scale before keeping: the cache holds what is drawn, not what was
        // served.
        let decoded = if decoded.width() > max_side || decoded.height() > max_side {
            decoded.thumbnail(max_side, max_side)
        } else {
            decoded
        };

        let rgba = decoded.to_rgba8();
        let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
        );

        let mut inner = self.inner.borrow_mut();
        if inner.images.len() >= MAX_ENTRIES {
            // Oldest first. A chat scrolls forward, so the images least
            // likely to be wanted again are the ones fetched longest ago.
            let evict: Vec<String> = inner.order.drain(..MAX_ENTRIES / 4).collect();
            for key in evict {
                inner.images.remove(&key);
                inner.state.remove(&key);
            }
        }
        inner
            .images
            .insert(url.to_string(), Image::from_rgba8(buffer));
        inner.state.insert(url.to_string(), State::Ready);
        inner.order.push(url.to_string());
        true
    }

    pub fn len(&self) -> usize {
        self.inner.borrow().images.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2×2 PNG, so the decode path is exercised without a fixture file.
    fn tiny_png() -> Vec<u8> {
        let mut bytes = Vec::new();
        let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([90, 110, 232, 255]));
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn an_unknown_url_reports_missing_and_renders_blank() {
        let cache = Cache::new();
        assert_eq!(cache.state("https://x/a.png"), State::Missing);
        assert_eq!(cache.get_or_blank("https://x/a.png").size().width, 0);
    }

    #[test]
    fn a_url_is_claimed_once() {
        let cache = Cache::new();
        assert!(cache.claim("https://x/a.png"));
        // Already in flight — a second repaint must not queue it again.
        assert!(!cache.claim("https://x/a.png"));
        assert_eq!(cache.state("https://x/a.png"), State::Loading);
    }

    #[test]
    fn a_decoded_image_becomes_available_and_is_not_refetched() {
        let cache = Cache::new();
        assert!(cache.claim("https://x/a.png"));
        assert!(cache.insert("https://x/a.png", &tiny_png(), 64));
        assert_eq!(cache.state("https://x/a.png"), State::Ready);
        assert_eq!(cache.get_or_blank("https://x/a.png").size().width, 2);
        assert!(!cache.claim("https://x/a.png"));
    }

    #[test]
    fn undecodable_bytes_fail_once_rather_than_forever() {
        let cache = Cache::new();
        cache.claim("https://x/bad.png");
        assert!(!cache.insert("https://x/bad.png", b"not an image", 64));
        assert_eq!(cache.state("https://x/bad.png"), State::Failed);
        // A failed URL is never claimed again, so it costs one request.
        assert!(!cache.claim("https://x/bad.png"));
    }

    #[test]
    fn an_empty_url_is_never_fetched() {
        let cache = Cache::new();
        assert!(!cache.claim(""));
        assert_eq!(cache.state(""), State::Missing);
    }

    #[test]
    fn a_large_image_is_scaled_before_it_is_kept() {
        let cache = Cache::new();
        let mut bytes = Vec::new();
        let big = image::RgbaImage::from_pixel(400, 200, image::Rgba([0, 0, 0, 255]));
        image::DynamicImage::ImageRgba8(big)
            .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();

        cache.claim("https://x/big.png");
        assert!(cache.insert("https://x/big.png", &bytes, 100));
        let kept = cache.get_or_blank("https://x/big.png");
        assert!(kept.size().width <= 100, "kept at {}", kept.size().width);
        assert!(kept.size().height <= 100);
    }
}
