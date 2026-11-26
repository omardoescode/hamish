//! The run of the app GPUI tests: each test app measures text with the system text system.

use std::borrow::Cow;
use std::collections::HashSet;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex, OnceLock};

use gpui::{
    Bounds, DevicePixels, Font, FontId, FontMetrics, FontRun, GlyphId, Hsla, LineLayout,
    MissingGlyphSink, Pixels, PlatformTextSystem, RenderGlyphParams, Size, TestAppContext,
    TextRenderingMode,
};

/// Runs `test` `iterations` times, each time with a new seed, as `#[gpui::test]` does.
/// The test app uses the system text system, so text sizes and positions are real.
pub fn run(iterations: usize, test: impl Fn(&mut TestAppContext)) {
    // One text system for the process: each new platform starts threads that never stop.
    static TEXT_SYSTEM: OnceLock<Arc<dyn PlatformTextSystem>> = OnceLock::new();
    let text_system = TEXT_SYSTEM.get_or_init(|| {
        Arc::new(FontsOnce {
            system: gpui_platform::current_platform(true).text_system(),
            added: Mutex::new(HashSet::new()),
        })
    });
    let shared = AssertUnwindSafe((test, text_system));
    gpui::run_test(
        iterations,
        &[],
        0,
        &mut |dispatcher, _seed| {
            let (test, text_system) = &*shared;
            let mut cx =
                TestAppContext::build_with_text_system(dispatcher, None, Arc::clone(text_system));
            let _entity_refcounts = cx.app.borrow().ref_counts_drop_handle();
            test(&mut cx);
            cx.run_until_parked();
            cx.update(|cx| {
                cx.background_executor().forbid_parking();
                cx.quit();
            });
            cx.run_until_parked();
        },
        None,
    );
}

/// The shared system text system. It adds each static font one time: each test app adds
/// the bundled font again, and each copy makes each later font lookup slower.
struct FontsOnce {
    system: Arc<dyn PlatformTextSystem>,
    added: Mutex<HashSet<usize>>,
}

impl PlatformTextSystem for FontsOnce {
    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> gpui::Result<()> {
        let mut added = self
            .added
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let new: Vec<_> = fonts
            .into_iter()
            .filter(|font| match font {
                Cow::Borrowed(bytes) => added.insert(bytes.as_ptr().addr()),
                Cow::Owned(_) => true,
            })
            .collect();
        if new.is_empty() {
            return Ok(());
        }
        self.system.add_fonts(new)
    }

    fn set_missing_glyph_sink(&self, sink: Option<Arc<dyn MissingGlyphSink>>) {
        self.system.set_missing_glyph_sink(sink);
    }

    fn all_font_names(&self) -> Vec<String> {
        self.system.all_font_names()
    }

    fn font_id(&self, descriptor: &Font) -> gpui::Result<FontId> {
        self.system.font_id(descriptor)
    }

    fn prewarm_fonts(&self, font_ids: &[FontId]) {
        self.system.prewarm_fonts(font_ids);
    }

    fn font_metrics(&self, font_id: FontId) -> FontMetrics {
        self.system.font_metrics(font_id)
    }

    fn typographic_bounds(&self, font_id: FontId, glyph_id: GlyphId) -> gpui::Result<Bounds<f32>> {
        self.system.typographic_bounds(font_id, glyph_id)
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> gpui::Result<Size<f32>> {
        self.system.advance(font_id, glyph_id)
    }

    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        self.system.glyph_for_char(font_id, ch)
    }

    fn glyph_raster_bounds(
        &self,
        params: &RenderGlyphParams,
    ) -> gpui::Result<Bounds<DevicePixels>> {
        self.system.glyph_raster_bounds(params)
    }

    fn rasterize_glyph(
        &self,
        params: &RenderGlyphParams,
        raster_bounds: Bounds<DevicePixels>,
    ) -> gpui::Result<(Size<DevicePixels>, Vec<u8>)> {
        self.system.rasterize_glyph(params, raster_bounds)
    }

    fn layout_line(&self, text: &str, font_size: Pixels, runs: &[FontRun]) -> LineLayout {
        self.system.layout_line(text, font_size, runs)
    }

    fn recommended_rendering_mode(&self, font_id: FontId, font_size: Pixels) -> TextRenderingMode {
        self.system.recommended_rendering_mode(font_id, font_size)
    }

    fn glyph_dilation_for_color(&self, color: Hsla) -> u8 {
        self.system.glyph_dilation_for_color(color)
    }
}
