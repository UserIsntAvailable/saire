#![allow(unused_imports)]

use saire::{
    internals::image::PngImage,
    models::layer::{BlendingMode, LayerKind},
    pixel_ops::*,
    sai_::Sai as NewSai,
    Sai as OldSai,
};
use std::{collections::HashSet, io, path::PathBuf};

// TODO: Instead of using `rotate_{left,right}` I could instead use `slice::ptr_rotate()`.

#[inline]
fn rotate_left(bytes: &mut [u8], mid: usize) {
    bytes.rotate_left(mid);
    let len = bytes.len();
    bytes[len - mid..].fill(0);
}

#[inline]
fn rotate_right(bytes: &mut [u8], mid: usize) {
    bytes.rotate_left(mid);
    bytes[..mid].fill(0);
}

// TODO: clap
// TODO: indicatif
fn main() -> io::Result<()> {
    let mut args = std::env::args().skip(1).take(2);

    let input = args.next().expect("expected input sai file.");
    // let doc = OldSai::new_unchecked(&input);
    let buf = std::fs::read(&input)?;
    let doc = NewSai::new_unchecked(&buf);

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
    laytbl.sort_layers(&mut layers);

    let width = canvas.width as usize;
    let height = canvas.height as usize;
    let mut no_visible: HashSet<u32> = HashSet::new();
    let mut image_bytes = vec![0; width * height * 4];

    // TODO: `LayerType::Set`s can also apply a `BlendingMode` to its childs.
    for mut layer in layers
        .into_iter()
        // If a set is `visible = false`, all its children needs to be also `visible = false`.
        .filter(|layer| {
            if !layer.visible && layer.kind == LayerKind::Set {
                no_visible.insert(layer.id);
            } else if let Some(parent_id) = layer.parent_set {
                if no_visible.get(&parent_id).is_some() {
                    return !no_visible.insert(layer.id);
                }
            };

            layer.visible && layer.kind == LayerKind::Regular
        })
    {
        let fg_bytes = layer.data.as_mut().expect("LayerType::Regular");
        let layer_width_bytes = layer.bounds.width as usize * 4;

        if layer.bounds.y < 0 {
            rotate_left(fg_bytes, layer.bounds.y.abs() as usize * layer_width_bytes)
        } else {
            rotate_right(fg_bytes, layer.bounds.y as usize * layer_width_bytes)
        }

        let fg_chunks = fg_bytes
            .chunks_exact_mut(layer_width_bytes)
            .skip(8)
            .take(height);

        for (bg_chunk, fg_chunk) in image_bytes.chunks_exact_mut(width * 4).zip(fg_chunks) {
            if layer.bounds.x < 0 {
                rotate_left(fg_chunk, (layer.bounds.x.abs() as u32 * 4) as usize)
            } else {
                rotate_right(fg_chunk, (layer.bounds.x as u32 * 4) as usize)
            }

            for (bg, fg) in bg_chunk
                .chunks_exact_mut(4)
                .zip(fg_chunk[8 * 4..].chunks_exact_mut(4))
            {
                if fg[3] != 0 {
                    for i in 0..4 {
                        fg[i] = ((fg[i] as f32 * layer.opacity as f32) / 100.0) as u8
                    }

                    let manipulation = match layer.blending_mode {
                        BlendingMode::Multiply => multiply,
                        BlendingMode::Overlay => overlay,
                        BlendingMode::Luminosity => |bg, fg, _, _| luminosity(bg, fg),
                        BlendingMode::Screen => |bg, fg, _, _| screen(bg, fg),
                        // TODO:
                        //
                        // BlendingMode::PassThrough => todo!(),
                        // BlendingMode::Shade => todo!(),
                        // BlendingMode::LumiShade => todo!(),
                        // BlendingMode::Binary => todo!(),
                        _ => |bg, fg, _, fg_a| normal(bg, fg, fg_a),
                    };

                    for i in 0..4 {
                        bg[i] = manipulation(bg[i], fg[i], bg[3], fg[3])
                    }
                }
            }
        }
    }

    let png = PngImage {
        width: width as u32,
        height: height as u32,
        ..Default::default()
    };

    png.save(&premultiplied_to_straight(&image_bytes), output)?;

    Ok(())
}
