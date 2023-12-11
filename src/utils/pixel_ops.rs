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
