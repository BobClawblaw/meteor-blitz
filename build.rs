//! Windows: embeds the app icon (assets/icon/icon.ico, tools/make-icon.py)
//! and version info in the executable. Other targets need nothing.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/icon.ico");
        res.set("ProductName", "Meteor Blitz");
        res.set("FileDescription", "Meteor Blitz");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon not embedded: {e}");
        }
    }
}
