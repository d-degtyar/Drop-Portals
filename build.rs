//! Вшиває іконку і опис у DropPortals.exe (лише під час збирання для Windows).
//! Іконку малює той самий код, що й іконку в треї: `src/icon.rs`.

#[path = "src/icon.rs"]
mod icon;

use std::path::Path;

/// ICO з кількома розмірами; кожен - 32-бітний BMP з альфа-каналом.
fn write_ico(path: &Path, sizes: &[u32]) -> std::io::Result<()> {
    let mut images = Vec::new();
    for &size in sizes {
        let rgba = icon::rgba(size);
        let mask_row = size.div_ceil(32) as usize * 4;
        let mut data = Vec::new();
        for value in [40, size, size * 2] {
            data.extend_from_slice(&value.to_le_bytes()); // розмір заголовка, ширина, висота (з маскою)
        }
        data.extend_from_slice(&1u16.to_le_bytes()); // площини
        data.extend_from_slice(&32u16.to_le_bytes()); // біт на піксель
        data.extend_from_slice(&[0; 24]); // без стиснення, решта - нулі
        for row in (0..size as usize).rev() {
            // BMP зберігає рядки знизу вгору і кольори як BGRA
            for pixel in rgba[row * size as usize * 4..][..size as usize * 4].chunks(4) {
                data.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
            }
        }
        data.resize(data.len() + mask_row * size as usize, 0);
        images.push((size, data));
    }

    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, data) in &images {
        let side = if *size >= 256 { 0 } else { *size as u8 };
        ico.extend_from_slice(&[side, side, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(data.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in &images {
        ico.extend_from_slice(data);
    }
    std::fs::write(path, ico)
}

fn main() {
    println!("cargo:rerun-if-changed=src/icon.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    let ico = Path::new(&out).join("DropPortals.ico");
    write_ico(&ico, &[16, 24, 32, 48, 64, 256]).expect("не вдалося записати іконку");

    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon(ico.to_str().expect("шлях до іконки"))
        .set("ProductName", "Drop Portals")
        .set("FileDescription", "Drop Portals - плаваючі портали в папки");
    if let Err(error) = resource.compile() {
        // без іконки програма все одно працює - лише попереджаємо
        println!("cargo:warning=іконку не вшито: {error}");
    }
}
