fn main() {
    println!("cargo:rerun-if-changed=icons/icon.ico");
    println!("cargo:rerun-if-changed=icons/icon.png");
    if std::env::var("PROFILE").as_deref() == Ok("release") && tauri_build::is_dev() {
        panic!("Release builds must embed the frontend. Use `npm run tauri -- build --no-bundle` or `cargo build -p habitos-desktop --release --features custom-protocol` after `npm run build`.");
    }
    tauri_build::build()
}
