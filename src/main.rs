#![allow(unused_variables)]
#![feature(stmt_expr_attributes)]

use png::Encoder;
use saire::{utils::pixel_ops::*, BlendingMode, LayerType, Result, SaiDocument};
use std::collections::HashSet;
use std::fs::File;
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1).take(2);

    let input = args.next().expect("expected input sai file.");
    let doc = SaiDocument::new_unchecked(&input);

    let output = args.next().unwrap_or_else(|| {
        format!(
            "{}.png",
            PathBuf::from(input)
                .file_stem()
                .expect("doc will panic if input is not a valid file.")
                .to_string_lossy()
        )
    });

    let canvas = doc.canvas()?;
    let laytbl = doc.laytbl()?;
    let mut layers = doc.layers()?;
    laytbl.order(&mut layers);

    // TODO: check:
    //   clippling
    //   blending_mode

    // The `witdth` and `height` of layers are always rounded to the nearest multiple of 32.
    let rounded_width = ((canvas.width & !0x1F) + 0x20) as usize;
    let rounded_height = ((canvas.height & !0x1F) + 0x20) as usize;

    let mut no_visible: HashSet<u32> = HashSet::new();
    // TODO: VecDeque
    let mut image_bytes = vec![0; (rounded_width * rounded_height * 4) as usize];

    // TODO: Move layer's pixels to match `Layer::bounds`.
    for mut layer in layers
        .into_iter()
        // If a set is `visible = false`, all its children needs to be also `visible = false`.
        .filter(|layer| {
            if !layer.visible && layer.r#type == LayerType::Set {
                no_visible.insert(layer.id);
            } else if let Some(parent_id) = layer.parent_set {
                if no_visible.get(&parent_id).is_some() {
                    return !no_visible.insert(layer.id);
                }
            };

            layer.visible
        })
        .filter(|layer| layer.r#type == LayerType::Layer)
    {
        let fg_bytes = layer
            .data
            .as_mut()
            .expect("LayerType::Layer")
            .as_mut_slices()
            .0;

        for (bg_chunk, fg_chunk) in image_bytes
            .chunks_exact_mut(rounded_width)
            .zip(fg_bytes.chunks_exact_mut(layer.bounds.width as usize))
        {
            for (bg, fg) in bg_chunk
                .chunks_exact_mut(4)
                .zip(fg_chunk[..rounded_width].chunks_exact_mut(4))
            {
                if fg[3] != 0 {
                    for i in 0..4 {
                        fg[i] = ((fg[i] as f32 * layer.opacity as f32) / 100.0) as u8
                    }

                    match layer.blending_mode {
                        BlendingMode::Multiply => {
                            for i in 0..4 {
                                bg[i] = multiply(bg[i], fg[i], bg[3], fg[3])
                            }
                        }
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
    }

    // TODO: Crop to `canvas.width` x `canvas.height`.

    let mut png = Encoder::new(
        File::create(output)?,
        rounded_width as u32,
        rounded_height as u32,
    );
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);

    png.write_header()?
        .write_image_data(&premultiplied_to_straight(&image_bytes))?;

    Ok(())
}
