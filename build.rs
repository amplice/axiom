use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-env-changed=AXIOM_EMBED_GAME_DATA_PATH");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path = Path::new(&out_dir).join("axiom_embedded_game_data.json");

    let content = match env::var("AXIOM_EMBED_GAME_DATA_PATH") {
        Ok(path) => {
            println!("cargo:rerun-if-changed={path}");
            fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_string())
        }
        Err(_) => "{}".to_string(),
    };

    fs::write(out_path, content).expect("failed to write embedded game data");
}
