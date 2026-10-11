
use std::{ collections::BTreeMap, fmt::Write };

use color_quant::NeuQuant;

use image::{ ImageFormat, imageops::{ FilterType, resize }, load_from_memory_with_format };

use crate::runtime::terminal::TerminalCapabilities;

const LOGO: &[u8] = include_bytes!("../../Shelly.png");
const WELCOME: &str = include_str!("../../welcome_true.txt");
const COLUMNS: u32 = 35;
const ROWS: u32 = 18;
const GAP: u32 = 3;

/// Resize actual source pixels to a terminal-cell bounding box before encoding.
/// Cell dimensions are cached by startup detection; 8x16 is the fallback.
pub fn banner(capabilities: &TerminalCapabilities) -> Option<String>
{
    let (cell_width, cell_height) = capabilities.cell_size.unwrap_or((8, 16));
    let (screen_columns, screen_rows) = capabilities.screen_size.unwrap_or((80, 24));
    let columns = COLUMNS.min(u32::from(screen_columns) / 2).max(1);
    let rows = ROWS.min(u32::from(screen_rows).saturating_sub(2)).max(1);
    let available_width = columns * u32::from(cell_width);
    let available_height = rows * u32::from(cell_height);
    let mut source = load_from_memory_with_format(LOGO, ImageFormat::Png).ok()?.to_rgba8();
    let scale = (f64::from(available_width) / f64::from(source.width()))
        .min(f64::from(available_height) / f64::from(source.height()));
    let width = ((f64::from(source.width()) * scale).round() as u32)
        .clamp(1, available_width);
    let height = ((f64::from(source.height()) * scale).round() as u32)
        .clamp(1, available_height);

    // imageops::resize expects premultiplied alpha. Undo it after filtering so
    // colors at the edge do not acquire a black matte on a light terminal.
    for pixel in source.pixels_mut()
    {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel.0[..3]
        { *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8; }
    }
    let mut image = resize(&source, width, height, FilterType::Lanczos3);
    let mut visible = Vec::new();
    for pixel in image.pixels_mut()
    {
        // Sixel has binary transparency. Discard faint filtered edge pixels;
        // do not encode transparent source pixels as opaque black.
        if pixel[3] < 128 { continue; }
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel.0[..3]
        { *channel = ((u16::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8; }
        pixel[3] = 255;
        visible.extend_from_slice(&pixel.0);
    }
    if visible.is_empty() { return None; }
    let palette = NeuQuant::new(10, 256, &visible);
    let indices = image.pixels().map(|pixel|
        {
            if pixel[3] < 128 { 256 }
            else { palette.index_of(&pixel.0) as u16 }
        }).collect::<Vec<_>>();

    let mut sixel = format!("\x1bP0;1;0q\"1;1;{width};{height}");
    for (index, rgb) in palette.color_map_rgb().as_chunks::<3>().0.iter().enumerate()
    {
        let percent = |value| (u32::from(value) * 100 + 127) / 255;
        let _ = write!(sixel, "#{index};2;{};{};{}",
                       percent(rgb[0]), percent(rgb[1]), percent(rgb[2]));
    }
    for top in (0..height).step_by(6)
    {
        let mut planes = BTreeMap::<u16, Vec<u8>>::new();
        for y in top..(top + 6).min(height)
        {
            for x in 0..width
            {
                let index = indices[(y * width + x) as usize];
                if index == 256 { continue; }
                let plane = planes.entry(index).or_insert_with(|| vec![0; width as usize]);
                plane[x as usize] |= 1 << (y - top);
            }
        }
        for (position, (index, mut values)) in planes.into_iter().enumerate()
        {
            if position != 0 { sixel.push('$'); }
            while values.last() == Some(&0) { values.pop(); }
            let _ = write!(sixel, "#{index}");
            append_runs(&mut sixel, &values);
        }
        if top + 6 < height { sixel.push('-'); }
    }
    sixel.push_str("\x1b\\");

    let text = WELCOME.trim().lines().map(|line| line.trim_end().replace("\\x1b", "\x1b"))
        .collect::<Vec<_>>();
    let image_rows = height.div_ceil(u32::from(cell_height)) as usize;
    let total_rows = image_rows.max(text.len());
    let mut output = "\r\n".repeat(total_rows);
    let _ = write!(output, "\x1b[{total_rows}A\r\x1b7");
    let image_offset = (total_rows - image_rows) / 2;
    if image_offset != 0 { let _ = write!(output, "\x1b[{image_offset}B"); }
    output.push_str(&sixel);
    output.push_str("\x1b8");
    let text_offset = (total_rows - text.len()) / 2;
    let text_column = width.div_ceil(u32::from(cell_width)) + GAP;
    for row in 0..total_rows
    {
        if    let Some(index) = row.checked_sub(text_offset)
           && let Some(line) = text.get(index).filter(|line| !line.is_empty())
        { let _ = write!(output, "\x1b[{text_column}C{line}"); }
        output.push_str("\r\n");
    }
    Some(output)
}

fn append_runs(output: &mut String, values: &[u8])
{
    let mut start = 0;
    while start < values.len()
    {
        let mut end = start + 1;
        while end < values.len() && values[end] == values[start] { end += 1; }
        let count = end - start;
        let character = char::from(63 + values[start]);
        let repeat = format!("!{count}{character}");
        if repeat.len() < count { output.push_str(&repeat); }
        else { output.extend(std::iter::repeat_n(character, count)); }
        start = end;
    }
}
