/// Stable UTF-8 name hash → bright hue. No dependency on declaration/lane order.
/// RGB travels with the event so every client uses exactly the same color.
pub fn named_color(name: &str) -> [u8; 3] {
    let hash = name.bytes().fold(2166136261_u32, |h, b| (h ^ u32::from(b)).wrapping_mul(16777619));
    let hue = hash % 1536;
    let ramp = (hue % 256) as u8;
    let rgb = match hue / 256 {
        0 => [255, ramp, 0], 1 => [255 - ramp, 255, 0],
        2 => [0, 255, ramp], 3 => [0, 255 - ramp, 255],
        4 => [ramp, 0, 255], _ => [255, 0, 255 - ramp],
    };
    rgb.map(|v| 64 + (u16::from(v) * 180 / 255) as u8)
}

