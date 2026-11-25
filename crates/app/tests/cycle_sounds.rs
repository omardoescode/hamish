use std::sync::{Arc, Mutex};

use app::{SoundSink, Sounds, Workspace};
use gpui::{
    Bounds, Entity, EntityInputHandler, Modifiers, TestAppContext, VisualTestContext, point, px,
};
use hamish_core::{Document, Level, LoadedSettings, Provenance, Settings, Sound};

/// A fake audio device: it keeps the sounds the app asks for.
#[derive(Default)]
struct HeardSounds(Mutex<Vec<Sound>>);

impl SoundSink for HeardSounds {
    fn play(&self, sound: Sound) {
        if let Ok(mut heard) = self.0.lock() {
            heard.push(sound);
        }
    }
}

impl HeardSounds {
    fn take(&self) -> Vec<Sound> {
        self.0
            .lock()
            .map_or_else(|_| Vec::new(), |mut heard| heard.drain(..).collect())
    }
}

/// "Title\nThe customer waits." with a Word Slot on "customer": "customer", "client", "patron".
#[expect(clippy::expect_used, reason = "test fixture")]
fn slot_document() -> Document {
    let mut document = Document::new("Title\nThe customer waits.");
    let slot = document
        .create_slot(10..18, Level::Word, "client", Provenance::Writer)
        .expect("valid range");
    document
        .add_variant(slot, "patron", Provenance::Writer)
        .expect("the Slot exists");
    document
}

fn open(
    cx: &mut TestAppContext,
    settings: Settings,
) -> (Entity<Workspace>, Arc<HeardSounds>, &mut VisualTestContext) {
    assert!(cx.update(app::init).is_ok(), "bundled font loads");
    let heard = Arc::new(HeardSounds::default());
    let sink = Arc::clone(&heard);
    cx.update(|cx| {
        app::use_settings(
            None,
            LoadedSettings {
                settings,
                problem: None,
            },
            cx,
        );
        cx.set_global(Sounds(sink));
    });
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::new(slot_document(), window, cx));
    cx.run_until_parked();
    (workspace, heard, cx)
}

#[expect(clippy::expect_used, reason = "test fixture")]
fn hover(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, offset: usize) {
    let editor = cx.read(|cx| workspace.read(cx).editor().clone());
    let bounds = cx
        .update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.bounds_for_range(offset..offset, Bounds::default(), window, cx)
            })
        })
        .expect("the editor painted the text");
    let position = bounds.origin + point(px(3.), bounds.size.height / 2.);
    cx.simulate_mouse_move(position, None, Modifiers::none());
}

#[gpui::test]
fn up_down_over_a_slot_play_the_high_tone_and_the_low_tone_back_at_the_original(
    cx: &mut TestAppContext,
) {
    let (workspace, heard, cx) = open(cx, Settings::default());
    hover(&workspace, cx, 12);

    cx.simulate_keystrokes("down down down up");

    assert_eq!(
        heard.take(),
        [
            Sound::HighTone,
            Sound::HighTone,
            Sound::LowTone,
            Sound::HighTone
        ]
    );
}

/// Moves the caret into "customer".
const INTO_CUSTOMER: &str = "down right right right right right";

#[gpui::test]
fn alternatives_panel_previews_use_the_same_tones(cx: &mut TestAppContext) {
    let (_, heard, cx) = open(cx, Settings::default());
    cx.simulate_keystrokes(INTO_CUSTOMER);
    assert_eq!(heard.take(), []);

    cx.simulate_keystrokes("ctrl-shift-a up up up");

    assert_eq!(
        heard.take(),
        [Sound::HighTone, Sound::HighTone, Sound::LowTone]
    );
}

#[gpui::test]
fn return_in_the_panel_that_makes_a_new_variant_active_plays_the_high_tone(
    cx: &mut TestAppContext,
) {
    let (_, heard, cx) = open(cx, Settings::default());
    cx.simulate_keystrokes(INTO_CUSTOMER);
    cx.simulate_keystrokes("ctrl-shift-a");
    cx.simulate_input("buyer");
    assert_eq!(heard.take(), []);

    cx.simulate_keystrokes("enter");

    assert_eq!(heard.take(), [Sound::HighTone]);
}

#[gpui::test]
fn return_in_the_panel_on_text_with_no_slot_plays_the_high_tone(cx: &mut TestAppContext) {
    let (_, heard, cx) = open(cx, Settings::default());
    cx.simulate_keystrokes("down end left left");
    cx.simulate_keystrokes("ctrl-shift-a");
    cx.simulate_input("sits");

    cx.simulate_keystrokes("enter");

    assert_eq!(heard.take(), [Sound::HighTone]);
}

#[gpui::test]
fn a_variant_delete_plays_the_explosion(cx: &mut TestAppContext) {
    let (_, heard, cx) = open(cx, Settings::default());
    cx.simulate_keystrokes(INTO_CUSTOMER);

    cx.simulate_keystrokes("ctrl-shift-backspace");

    assert_eq!(heard.take(), [Sound::Explosion]);
}

#[gpui::test]
fn no_sound_plays_with_cycle_sounds_off(cx: &mut TestAppContext) {
    let settings = Settings {
        cycle_sounds: false,
        ..Settings::default()
    };
    let (workspace, heard, cx) = open(cx, settings);
    hover(&workspace, cx, 12);

    cx.simulate_keystrokes("down down down");

    assert_eq!(heard.take(), []);
}

#[expect(clippy::expect_used, reason = "test fixture")]
fn click(cx: &mut VisualTestContext, id: &'static str) {
    let bounds = cx.debug_bounds(id).expect("the window shows the element");
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn the_cycle_sounds_toggle_in_settings_turns_the_sounds_off(cx: &mut TestAppContext) {
    let (workspace, heard, cx) = open(cx, Settings::default());

    cx.simulate_keystrokes("ctrl-,");
    click(cx, "settings-section-editor");
    click(cx, "settings-cycle-sounds");
    cx.simulate_keystrokes("escape");
    hover(&workspace, cx, 12);
    cx.simulate_keystrokes("down");

    assert_eq!(heard.take(), []);
}
