//! Tray icon showing the battery level as a number, drawn with a 5×7 pixel font
//! (StatusNotifierItem has no text label, so the number has to be in the pixmap).

const SIZE: usize = 64;

const DIGITS: [[u8; 7]; 10] = [
    [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
    [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
    [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
    [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110],
    [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
    [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
    [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
    [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
    [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
    [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
];

/// The level in white, green while charging, amber at 25% and below, red at 10%.
pub fn battery_number(level: u8, charging: bool) -> ksni::Icon {
    let color: [u8; 3] = match level {
        _ if charging => [0x4c, 0xc3, 0x8a],
        0..=10 => [0xef, 0x5f, 0x5f],
        11..=25 => [0xf0, 0xb4, 0x4c],
        _ => [0xff, 0xff, 0xff],
    };
    let text: Vec<usize> = level.min(100).to_string().bytes().map(|b| (b - b'0') as usize).collect();
    // biggest whole-pixel scale that fits, with room for the outline
    let cols = text.len() * 6 - 1;
    let scale = ((SIZE - 4) / cols).min((SIZE - 4) / 7);
    let (x0, y0) = ((SIZE - cols * scale) / 2, (SIZE - 7 * scale) / 2);

    let mut on = vec![false; SIZE * SIZE];
    for (i, &d) in text.iter().enumerate() {
        for (row, bits) in DIGITS[d].iter().enumerate() {
            for col in 0..5 {
                if bits & (0b10000 >> col) != 0 {
                    let (x, y) = (x0 + (i * 6 + col) * scale, y0 + row * scale);
                    for dy in 0..scale {
                        on[(y + dy) * SIZE + x..(y + dy) * SIZE + x + scale].fill(true);
                    }
                }
            }
        }
    }
    // ARGB32, network byte order; a dark 2px outline keeps it readable on light panels
    let mut data = vec![0u8; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let px = &mut data[(y * SIZE + x) * 4..][..4];
            if on[y * SIZE + x] {
                px.copy_from_slice(&[0xff, color[0], color[1], color[2]]);
            } else if near(&on, x, y) {
                px.copy_from_slice(&[0xc0, 0x10, 0x12, 0x16]);
            }
        }
    }
    ksni::Icon { width: SIZE as i32, height: SIZE as i32, data }
}

fn near(on: &[bool], x: usize, y: usize) -> bool {
    (y.saturating_sub(2)..(y + 3).min(SIZE)).any(|ny| (x.saturating_sub(2)..(x + 3).min(SIZE)).any(|nx| on[ny * SIZE + nx]))
}

#[cfg(test)]
mod tests {
    #[test]
    fn fits_every_level() {
        for level in [0, 5, 42, 100] {
            let icon = super::battery_number(level, false);
            assert_eq!(icon.data.len(), 64 * 64 * 4);
            assert!(icon.data.chunks(4).any(|p| p == [0xff, 0xff, 0xff, 0xff] || p[0] == 0xff));
        }
    }
}
