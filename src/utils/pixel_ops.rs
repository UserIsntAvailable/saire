use std::cmp::Ordering;

/// Converts from RGBA `pre-multiplied alpha` to RGBA `straight` color format.
pub fn premultiplied_to_straight(pixels: &[u8]) -> Vec<u8> {
    pixels
        .chunks_exact(4)
        .flat_map(|chunk| {
            let scale = chunk[3] as f32 / 255.0;

            let mut quad_pixel = i32::from_le_bytes(chunk.try_into().expect("chunk_exact(4)"));
            for c in 0..3 {
                let mut cur_channel = quad_pixel >> (c * 8);
                cur_channel &= 255;
                let mut channel_float = cur_channel as f32;

                channel_float /= 255.0;
                channel_float /= scale;
                channel_float *= 255.0;

                cur_channel = channel_float as i32;
                cur_channel &= 255;
                cur_channel <<= c * 8;

                quad_pixel &= !(0xFF << (c * 8));
                quad_pixel |= cur_channel;
            }

            i32::to_le_bytes(quad_pixel)
        })
        .collect()
}

#[inline]
pub fn luminosity(bg: u8, fg: u8) -> u8 {
    f32::min(bg as f32 + fg as f32, 255.0) as u8
}

#[inline]
pub fn multiply(bg: u8, fg: u8, bg_a: u8, fg_a: u8) -> u8 {
    let diff = |a, b| ((a as f32 * b as f32) / 255.0) as u8;

    normal(bg, diff(bg, fg), diff(bg_a, fg_a))
}

#[inline]
pub fn normal(bg: u8, fg: u8, fg_a: u8) -> u8 {
    (fg as f32 + bg as f32 * (1.0 - fg_a as f32 / 255.0)) as u8
}

#[inline]
pub fn overlay(bg: u8, fg: u8, bg_a: u8, fg_a: u8) -> u8 {
    match fg.cmp(&127) {
        Ordering::Less => {
            // FIX: Kinda works?

            let bg = bg as f32;
            let fg = fg as f32;
            let fg_a = fg_a as f32;

            let diff = bg / 255.0 * (bg + (2.0 * fg / 255.0) * (255.0 - bg));
            let opacity = ((255.0 - fg_a) / 255.0) * 100.0;
            let color = (diff - ((diff * opacity) / 100.0)) as u8;

            normal(bg as u8, color, fg_a as u8)
        }
        Ordering::Equal => normal(bg, fg, fg_a),
        Ordering::Greater => multiply(bg, fg, bg_a, fg_a),
    }
}

#[inline]
pub fn screen(bg: u8, fg: u8) -> u8 {
    (255.0 - (((255.0 - bg as f32) * (255.0 - fg as f32)) / 255.0)) as u8
}
