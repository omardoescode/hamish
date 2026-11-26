//! The run of the app GPUI tests: each test app measures text with the system text system.

use std::panic::AssertUnwindSafe;
use std::sync::{Arc, OnceLock};

use gpui::{PlatformTextSystem, TestAppContext};

/// Runs `test` `iterations` times, each time with a new seed, as `#[gpui::test]` does.
/// The test app uses the system text system, so text sizes and positions are real.
pub fn run(iterations: usize, test: impl Fn(&mut TestAppContext)) {
    // One text system for the process: each new platform starts threads that never stop.
    static TEXT_SYSTEM: OnceLock<Arc<dyn PlatformTextSystem>> = OnceLock::new();
    let text_system =
        TEXT_SYSTEM.get_or_init(|| gpui_platform::current_platform(true).text_system());
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
