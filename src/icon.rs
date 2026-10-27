//! Іконка-портал (синє кільце з бірюзовою точкою), намальована кодом.
//! Цей самий файл використовує `build.rs`, щоб вшити іконку в `.exe`.

const RING: [u8; 3] = [0x4F, 0x7C, 0xFF];
const DOT: [u8; 3] = [0x12, 0xA5, 0x94];

/// Пікселі RGBA (без премультиплікації), рядок за рядком згори вниз.
pub fn rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let center = s / 2.0;
    let outer = center - 1.0;
    let inner = outer - (s * 0.16).max(2.0);
    let dot = s * 0.16;

    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let d = (dx * dx + dy * dy).sqrt();
            // м'які краї: покриття пікселя по відстані до межі
            let ring = (outer - d + 0.5).clamp(0.0, 1.0).min((d - inner + 0.5).clamp(0.0, 1.0));
            let core = (dot - d + 0.5).clamp(0.0, 1.0);
            let (color, coverage) = if core > 0.0 { (DOT, core) } else { (RING, ring) };
            pixels.extend_from_slice(&color);
            pixels.push((coverage * 255.0).round() as u8);
        }
    }
    pixels
}
