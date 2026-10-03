// Gives the Windows executable its display name, which is what the taskbar,
// Task Manager and "Open with" lists show.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set("FileDescription", "MulchLauncher");
        resource.set("ProductName", "MulchLauncher");
        resource.set("InternalName", "MulchLauncher");
        resource.set("OriginalFilename", "MulchLauncher.exe");
        resource.compile().expect("failed to embed Windows resources");
    }
}
