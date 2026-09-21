fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("slint build failed");

    embed_windows_icon();
}

/// Put the app icon and version into the Windows executable.
///
/// Without it the installed client is a generic white rectangle in the Start
/// Menu, on the taskbar and in Explorer — the shortcut takes its icon from
/// the binary, so the installer cannot supply one on its own.
///
/// A failure here costs the icon, not the build: it needs `rc.exe` from the
/// Windows SDK, and a build box without one should still produce a working
/// client.
#[cfg(windows)]
fn embed_windows_icon() {
    println!("cargo:rerun-if-changed=assets/icons/icon.ico");
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("assets/icons/icon.ico");
    resource.set("ProductName", "MiniChat");
    resource.set("FileDescription", "MiniChat");
    resource.set("CompanyName", "MiniChat");
    resource.set("LegalCopyright", "MIT");
    if let Err(error) = resource.compile() {
        println!("cargo:warning=could not embed the Windows icon: {error}");
    }
}

#[cfg(not(windows))]
fn embed_windows_icon() {}
