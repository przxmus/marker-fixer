mod app;
mod error;
mod ffprobe;
mod mp4;
mod tools;
mod xmp;

use app::App;

fn main() {
    std::process::exit(App::run());
}
