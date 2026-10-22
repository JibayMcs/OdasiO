//! Sous Windows, embarque l'icône dans l'exécutable.

fn main() {
    println!("cargo:rerun-if-changed=../../packaging/icons/windows/odasio.ico");
    #[cfg(windows)]
    winresource::WindowsResource::new()
        .set_icon("../../packaging/icons/windows/odasio.ico")
        .compile()
        .expect("ressources Windows");
}
