fn main() {
    // Build scripts run on the host; the icon resource is only embeddable when
    // building on Windows, which is how release Windows builds are produced.
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/favicon.ico");
        winresource::WindowsResource::new()
            .set_icon("assets/favicon.ico")
            .compile()
            .expect("embed Windows exe icon resource");
    }
}
