use image::GenericImageView;

pub fn load_tray_icon() -> tray_icon::Icon {
    const ICON_BYTES: &[u8] = include_bytes!("../assets/erdos_alpha.png");

    let img = image::load_from_memory(ICON_BYTES)
        .expect("Failed to decode embedded icon PNG");

    let (width, height) = img.dimensions();
    let rgba = img.into_rgba8().into_raw();

    tray_icon::Icon::from_rgba(rgba, width, height)
        .expect("Failed to create tray icon from RGBA data")
}
