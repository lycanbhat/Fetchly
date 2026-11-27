//! Build steps:
//! 1. Embed the Geist fonts into a CSS file (base64) so the app needs no runtime asset files.
//! 2. Embed the icon + version info into the Windows executable.
use base64::Engine as _;

fn main() {
    let out = std::env::var("OUT_DIR").unwrap();
    let font = |file: &str| {
        println!("cargo:rerun-if-changed=assets/fonts/{file}");
        let bytes = std::fs::read(format!("assets/fonts/{file}")).expect("font file");
        base64::engine::general_purpose::STANDARD.encode(bytes)
    };
    let css = format!(
        "@font-face {{ font-family: 'Geist'; font-weight: 100 900; font-style: normal; font-display: block; src: url(data:font/woff2;base64,{}) format('woff2'); }}\n\
         @font-face {{ font-family: 'Geist Mono'; font-weight: 100 900; font-style: normal; font-display: block; src: url(data:font/woff2;base64,{}) format('woff2'); }}\n",
        font("Geist-Variable.woff2"),
        font("GeistMono-Variable.woff2")
    );
    std::fs::write(format!("{out}/fonts.css"), css).unwrap();

    // Brand icons as data URLs, plus the window icon pre-decoded to RGBA (what OS windows need).
    let data_url = |file: &str| {
        println!("cargo:rerun-if-changed=assets/brand/{file}");
        let bytes = std::fs::read(format!("assets/brand/{file}")).expect("brand image");
        format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
    };
    let brand = format!(
        "pub const MARK_DARK: &str = {:?};\npub const MARK_LIGHT: &str = {:?};\npub const FAVICON: &str = {:?};\n",
        data_url("mark-dark.png"),
        data_url("mark-light.png"),
        data_url("favicon.png")
    );
    std::fs::write(format!("{out}/brand.rs"), brand).unwrap();
    println!("cargo:rerun-if-changed=assets/brand/window-icon.png");
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open("assets/brand/window-icon.png").unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba, "window icon must be RGBA");
    buf.truncate(info.buffer_size());
    std::fs::write(format!("{out}/window-icon.rgba"), &buf).unwrap();
    std::fs::write(format!("{out}/window-icon.size"), format!("{}", info.width)).unwrap();

    println!("cargo:rerun-if-changed=../../assets/windows/fetchly.rc");
    println!("cargo:rerun-if-changed=../../assets/windows/fetchly.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("../../assets/windows/fetchly.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("failed to embed Windows resources");
    }
}
