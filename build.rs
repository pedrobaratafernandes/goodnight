// Embeds the icon and version information into the Windows .exe.
// On other platforms this does nothing.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("ProductName", "Goodnight")
            .set("FileDescription", "Goodnight - temporizador para adormecer")
            .set("OriginalFilename", "goodnight.exe")
            .set("LegalCopyright", "MIT License");
        if let Err(e) = res.compile() {
            panic!("Falha ao embutir o ícone no executável: {e}");
        }
    }
}
