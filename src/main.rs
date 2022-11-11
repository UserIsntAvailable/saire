#![allow(unused_variables)]
#![feature(stmt_expr_attributes)]

use png::Encoder;
use saire::{utils::pixel_ops::*, BlendingMode, LayerType, Result, SaiDocument};
use std::fs::File;
use std::{cmp::Ordering, collections::HashSet};

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

    let rounded_width = ((canvas.width & !0x1F) + 0x20) as usize;
    let rounded_height = ((canvas.height & !0x1F) + 0x20) as usize;

    let mut no_visible: HashSet<u32> = HashSet::new();

    let layers = layers
        .into_iter()
        // Filters layers of sets that are hidden
        // Could be better written...
        .filter(|layer| {
            if !layer.visible && layer.r#type == LayerType::Set {
                no_visible.insert(layer.id);
            } else {
                if let Some(parent_id) = layer.parent_set {
                    if no_visible.get(&parent_id).is_some() {
                        return !no_visible.insert(layer.id);
                    }
                };
            }

            layer.visible
        })
        .filter(|layer| layer.r#type == LayerType::Layer)
        .filter(|layer| {
            let name = layer.name.as_ref().unwrap();

            // [base] x: -20, y: -12
            // [Layer4] x: -64, y: 0
            // if layer.bounds.height > rounded_height || layer.bounds.width > rounded_width {
            //     println!("[{: >15}] {:?}", name, layer.bounds);
            // }

            !name.contains("Layer4") && !name.contains("base") && !name.contains("Layer13")

            // layer.id != 2 && layer.id != 250 && layer.id != 57 && layer.id != 58

            // layer.id == 75 && layer.id == 76

            // layer.id != 2
            //     && layer.id != 4
            //     && layer.id != 7
            //     && layer.id != 150
            //     && layer.id != 153
            //     && layer.id != 154

            // layer.id != 2
            //     && layer.id != 3
            //     && layer.id != 16
            //     && layer.id != 58
            //     && layer.id != 59
            //     && layer.id != 79
        })
        // Fix x, y positions.
        // .map(|mut layer| {
        //     let x = layer.bounds.x;
        //     let y = layer.bounds.y;
        //     let width = layer.bounds.width as usize;
        //     let height = layer.bounds.height as usize;
        //
        //     let mut data = layer
        //         .data
        //         .as_mut()
        //         .expect("LayerType::Layer")
        //         .as_mut_slices()
        //         .0;
        //
        //     match x.cmp(&0) {
        //         Ordering::Less => {
        //             let bytes_padding = (rounded_width - width) / 2;
        //
        //             data.chunks_exact_mut(width * 4).for_each(|chunk| {
        //                 let padded_chunk = &mut chunk[bytes_padding..chunk.len() - bytes_padding];
        //                 padded_chunk.rotate_left(x.abs() as usize * 4);
        //                 TODO
        //                 padded_chunk[..].fill(0);
        //             });
        //         }
        //         Ordering::Equal => (),
        //         Ordering::Greater => todo!(),
        //     }
        //
        //     layer
        // })
        // Crop to have size of rounded_width and rounded_height.
        // .map(|mut layer| {
        //     let width = layer.bounds.width as usize;
        //     let height = layer.bounds.height as usize;
        //
        //     if width > rounded_width {}
        //
        //     if height > rounded_height {}
        //
        //     layer
        // })
        .collect::<Vec<_>>();

    todo!();

    let max_width = layers.iter().map(|layer| layer.bounds.width).max().unwrap();
    let max_height = layers
        .iter()
        .map(|layer| layer.bounds.height)
        .max()
        .unwrap();
    let bytes_count = (max_width * max_height * 4) as usize;

    // TOOD: VecDeque
    let mut image_bytes = vec![0; bytes_count];

    for mut layer in layers {
        let fg_bytes = layer
            .data
            .as_mut()
            .expect("filter LayerType::Layer")
            .as_mut_slices()
            .0;

        // FIX: Remove `idx`; It is only needed for debugging.
        for (idx, (bg, fg)) in image_bytes
            .chunks_exact_mut(4)
            .zip(fg_bytes.chunks_exact_mut(4))
            .enumerate()
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
                    // FIX: Doesn't quite work yet.
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

    let mut png = Encoder::new(File::create("file2.png")?, max_width, max_height);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);

    png.write_header()?
        .write_image_data(&premultiplied_to_straight(&image_bytes))?;

    Ok(())
}
