//! Embed the icon and version details into the Windows .exe files.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("ProductName", "Invoices")
            .set("FileDescription", "Invoices");
        if let Err(e) = res.compile() {
            // Without a resource compiler the app still builds, just without an icon.
            println!("cargo:warning=could not embed the Windows icon: {e}");
        }
    }
}
