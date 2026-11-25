use app::{DeviceSounds, SoundSink};
use ka_teb_core::Sound;
#[test]
fn smoke() {
    let sounds = DeviceSounds::start();
    sounds.play(Sound::HighTone);
    std::thread::sleep(std::time::Duration::from_millis(400));
    sounds.play(Sound::Explosion);
    std::thread::sleep(std::time::Duration::from_millis(600));
}
