// Gives the Windows executable its icon and display name, which is what the
// taskbar, Explorer, Task Manager and "Open with" lists show.
fn main() {
    println!("cargo:rerun-if-changed=assets/brand/MulchLauncher.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/brand/MulchLauncher.ico");
        resource.set("FileDescription", "MulchLauncher");
        resource.set("ProductName", "MulchLauncher");
        resource.set("InternalName", "MulchLauncher");
        resource.set("OriginalFilename", "MulchLauncher.exe");
        resource.compile().expect("failed to embed Windows resources");
    }
}
