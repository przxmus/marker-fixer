mod app;
mod error;
mod ffprobe;
mod mp4;
mod xmp;

use app::App;

fn main() {
    if let Err(err) = App::run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
