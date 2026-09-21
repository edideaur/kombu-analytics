#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../webui/dist/index.html");
    let out_dir = env::var("OUT_DIR")?;
    let dest_path = Path::new(&out_dir).join("index.html");
    let dist_path = Path::new("../../webui/dist/index.html");
    let content = if dist_path.exists() {
        fs::read_to_string(dist_path).unwrap_or_else(|_| default_html())
    } else {
        default_html()
    };
    fs::write(dest_path, content)?;
    Ok(())
}

fn default_html() -> String {
    "<!doctype html><html lang=\"en\"><head><meta charset=\"UTF-8\"/><title>Kombu</title></head><body><div id=\"root\"></div></body></html>".to_string()
}
