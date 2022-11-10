#![allow(unused_variables)]
#![feature(stmt_expr_attributes)]
use saire::{BlendingMode, LayerType, Result, SaiDocument};
use std::{cmp::Ordering, collections::HashSet};

#[inline]
fn luminosity(bg: u8, fg: u8) -> u8 {
    f32::min(bg as f32 + fg as f32, 255.0) as u8
}

#[inline]
fn multiply(bg: u8, fg: u8, bg_a: u8, fg_a: u8) -> u8 {
    let diff = |a, b| ((a as f32 * b as f32) / 255.0) as u8;

    normal(bg, diff(bg, fg), diff(bg_a, fg_a))
}

#[inline]
fn normal(bg: u8, fg: u8, fg_a: u8) -> u8 {
    (fg as f32 + bg as f32 * (1.0 - fg_a as f32 / 255.0)) as u8
}

#[inline]
fn overlay(bg: u8, fg: u8, bg_a: u8, fg_a: u8) -> u8 {
    match fg.cmp(&127) {
        Ordering::Less => screen(bg, fg),
        Ordering::Equal => normal(bg, fg, fg_a),
        Ordering::Greater => multiply(bg, fg, bg_a, fg_a),
    }
}

#[inline]
fn screen(bg: u8, fg: u8) -> u8 {
    (255.0 - (((255.0 - bg as f32) * (255.0 - fg as f32)) / 255.0)) as u8
}

fn main() -> Result<()> {
    let sai_file = std::env::args().nth(1).unwrap();
    let doc = SaiDocument::new_unchecked(sai_file);

    let canvas = doc.canvas()?;
    let laytbl = doc.laytbl()?;
    let mut layers = doc.layers()?;
    laytbl.order(&mut layers);

    // TODO: check:
    //   clippling
    //   blending_mode

    // use itertools::Itertools;
    //
    // let hash = layers.iter().into_group_map_by(|l| l.blending_mode);
    // println!("{:?}", hash.keys());
    // todo!();
    // for layer in hash[&BlendingMode::PassThrough].iter() {
    //     println!("{}", layer.name.as_ref().unwrap());
    // }

    // for layer in layers
    //     .iter()
    //     // .filter(|l| l.id == 100)
    // {
    //     println!("name: {}; id: {}", layer.name.as_ref().unwrap(), layer.id);
    //     todo!();
    // }

    // for (i, layer) in layers
    //     .iter()
    //     .filter(|l| l.r#type == LayerType::Layer)
    //     .enumerate()
    // {
    //     let name = format!(
    //         "{:0>4}-{}-{:?}-{}.png",
    //         i,
    //         layer.id,
    //         layer.blending_mode,
    //         layer.name.as_ref().unwrap()
    //     );
    //     layer.to_png(Some(name))?;
    // }

    let mut no_visible: HashSet<u32> = HashSet::new();

    let mut layers = layers
        .into_iter()
        // Filters layers of sets that are hidden
        // Could be better written...
        .filter(|l| {
            if !l.visible && l.r#type == LayerType::Set {
                no_visible.insert(l.id);
            } else {
                if let Some(parent_id) = l.parent_set {
                    if no_visible.get(&parent_id).is_some() {
                        return !no_visible.insert(l.id);
                    }
                };
            }

            l.visible
        })
        .filter(|l| l.r#type == LayerType::Layer)
        .filter(|l| {
            let name = l.name.as_ref().unwrap();

            // bigger
            // [base] x: -20, y: -12
            // [Layer4] x: -64, y: 0
            // if l.bounds.height > canvas.height || l.bounds.width > canvas.width {
            //     println!("[{: >15}] {:?}", name, l.bounds);
            // }

            !name.contains("Layer4") && !name.contains("base") && !name.contains("Layer13")

            // l.id != 2 && l.id != 250 && l.id != 57 && l.id != 58

            // l.id == 75 && l.id == 76

            // l.id != 2 && l.id != 4 && l.id != 7 && l.id != 150 && l.id != 153 && l.id != 154

            // l.id != 2 && l.id != 3 && l.id != 16 && l.id != 58 && l.id != 59 && l.id != 79
        })
        .collect::<Vec<_>>();

    let max_width = layers.iter().map(|layer| layer.bounds.width).max().unwrap();
    let max_height = layers.iter().map(|layer| layer.bounds.height).max().unwrap();
    let bytes_count = (max_width * max_height * 4) as usize;

    println!("{max_width}");
    println!("{max_height}");

    layers.iter_mut().for_each(|l| {
        let data = &mut l.data.as_mut().expect("filter LayerType::Layer");

        if data.len() < bytes_count {
            data.resize(bytes_count, 0);
        }
    });

    let mut image_bytes = vec![0; bytes_count];

    for mut layer in layers {
        let fg_bytes = layer.data.as_mut().expect("filter LayerType::Layer");

        // FIX: Remove `idx`; It is only needed for debugging.
        for (idx, (bg, fg)) in image_bytes
            .chunks_exact_mut(4)
            .zip(fg_bytes.chunks_exact_mut(4))
            .enumerate()
        {
            if fg[3] != 0 {
                // if layer.id == 885 {
                //     println!("{fg:?}")
                // };

                for i in 0..4 {
                    fg[i] = ((fg[i] as f32 * layer.opacity as f32) / 100.0) as u8
                }

                match layer.blending_mode {
                    BlendingMode::Multiply => {
                        for i in 0..4 {
                            bg[i] = multiply(bg[i], fg[i], bg[3], fg[3])
                        }
                    }
                    // FIX: Doesn't quite work well
                    BlendingMode::Overlay => {
                        for i in 0..4 {
                            bg[i] = overlay(bg[i], fg[i], bg[3], fg[3])
                        }
                    }
                    BlendingMode::Luminosity => {
                        for i in 0..4 {
                            bg[i] = luminosity(bg[i], fg[i])
                        }
                    }
                    BlendingMode::Screen => {
                        for i in 0..4 {
                            bg[i] = screen(bg[i], fg[i])
                        }
                    }
                    // BlendingMode::PassThrough => todo!(),
                    // BlendingMode::Shade => todo!(),
                    // BlendingMode::LumiShade => todo!(),
                    // BlendingMode::Binary => todo!(),
                    _ => {
                        for i in 0..4 {
                            bg[i] = normal(bg[i], fg[i], fg[3])
                        }
                    }
                }
            }
        }
    }

    for chunk in image_bytes.chunks_exact_mut(4) {
        // Alpha is pre-multiplied, convert to straight. Get Alpha into
        // [0.0, 1.0] range.
        let scale = chunk[3] as f32 / 255.0;

        let mut quad_pixel = i32::from_le_bytes(chunk.try_into().unwrap());
        for c in 0..3 {
            let mut cur_channel = quad_pixel >> (c * 8);
            cur_channel &= 255;
            let mut channel_float = cur_channel as f32;

            channel_float /= 255.0;
            channel_float /= scale;
            channel_float *= 255.0;

            cur_channel = channel_float as i32;
            cur_channel &= 255;
            cur_channel = cur_channel << (c * 8);

            quad_pixel = !(0xFF << (c * 8)) & quad_pixel;
            quad_pixel |= cur_channel;
        }

        for (dst, src) in chunk.iter_mut().zip(i32::to_le_bytes(quad_pixel)) {
            *dst = src;
        }
    }

    use png::Encoder;
    use std::fs::File;

    let mut png = Encoder::new(File::create("file2.png")?, max_width, max_height);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);
    png.write_header()?.write_image_data(&image_bytes)?;

    Ok(())
}
