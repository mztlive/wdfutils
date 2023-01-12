// 所有offset常量的格式为：(偏移量, 长度)

use std::io::{Cursor, Read, Seek, SeekFrom};

use anyhow::anyhow;
use image::RgbaImage;
use image::{ImageBuffer, Rgba};

use crate::buffer_utils;

/// 文件标志位偏移量
const FILE_FLAG_OFFSET: (u64, usize) = (0, 2);

/// 文件头长度偏移量
const FILE_HEADER_LEN_OFFSET: (u64, usize) = (2, 2);

// 调色板长度
const PALETTE_SIZE: u16 = 256 * 2;

// 像素类型是透明的（ALPHA）
const PIXEL_TYPE_IS_ALPHA: u8 = 0x00;

// 像素类型是像素的（PIXELS）
const PIXEL_TYPE_IS_PIXELS: u8 = 0x40;

// 像素类型是重复的（REPEAT）
const PIXEL_TYPE_IS_REPEAT: u8 = 0x80;

// 像素类型是跳过的（SKIP）
const PIXEL_TYPE_IS_SKIP: u8 = 0xC0;

const ALPHA_TYPE_IS_PIXEL: u8 = 0x20;

const TYPE_FLAG: u8 = 0xC0;

/// 用于在文件中跳过的字节数， 这是FLAG + HEADER的长度
const SKIP_OFFSET: u16 = 4;

// enum PixelType {
//     Alpha = 0x00,
//     Pixels = 0x40,
//     Repeat = 0x80,
//     Skip = 0xC0,
// }

// enum AlphaType {
//     Pixel = 0x20,
//     Repeat = 0x00,
// }

pub struct ImageHeader {
    /// 方向
    pub direction: u16,

    /// 每个方向有多少帧
    pub frame_count: u16,

    /// 图片的宽度
    pub sprite_width: u16,

    /// 图片的高度
    pub sprite_height: u16,

    /// 图片的x坐标
    pub sprite_x: u16,

    /// 图片的y坐标
    pub sprite_y: u16,
    // delays: u16,
    pub header_len: u16,
}

#[derive(Debug)]
pub struct Frame {
    /// 帧的x坐标
    pub x: u32,

    /// 帧的y坐标
    pub y: u32,

    /// 帧的宽度
    pub width: u32,

    /// 帧的高度
    pub height: u32,

    /// 帧的偏移量
    pub offset: u32,

    /// 每一行像素的偏移量
    pub line_offsets: Vec<u32>,
}

/// 包含了Was文件的所有信息
/// (ImageHeader, Vec<Frame>, Vec<RgbaImage>)
pub type WasInfo = (ImageHeader, Vec<Frame>, Vec<RgbaImage>);

/// 检查文件格式是否正确
fn check_file_format(file: &mut Cursor<Vec<u8>>) -> bool {
    let mut flag: [u8; FILE_FLAG_OFFSET.1] = [0; FILE_FLAG_OFFSET.1];
    file.seek(SeekFrom::Start(FILE_FLAG_OFFSET.0)).unwrap();
    file.read(&mut flag).unwrap();

    // 代表开头两位必须是SP (0x53 0x50) u8[] -> [83, 80]
    flag[0] == 83 && flag[1] == 80
}

/// 读取image的头信息
fn read_imageheader(file: &mut Cursor<Vec<u8>>) -> anyhow::Result<ImageHeader, anyhow::Error> {
    let mut header_len: [u8; FILE_HEADER_LEN_OFFSET.1] = [0; FILE_HEADER_LEN_OFFSET.1];
    file.seek(SeekFrom::Start(FILE_HEADER_LEN_OFFSET.0))?;
    file.read(&mut header_len)?;

    let header_len = u16::from_le_bytes(header_len);

    let mut header: Vec<u8> = vec![0; header_len.into()];
    file.read(&mut header)?;

    let sprite_num = u16::from_le_bytes(header[0..2].try_into()?);
    let frame_count = u16::from_le_bytes(header[2..4].try_into()?);
    let sprite_width = u16::from_le_bytes(header[4..6].try_into()?);
    let sprite_height = u16::from_le_bytes(header[6..8].try_into()?);
    let sprite_x = u16::from_le_bytes(header[8..10].try_into()?);
    let sprite_y = u16::from_le_bytes(header[10..12].try_into()?);

    Ok(ImageHeader {
        direction: sprite_num,
        frame_count,
        sprite_width,
        sprite_height,
        sprite_x,
        sprite_y,
        // delays: 0,
        header_len,
    })
}

/// 读取调色板
fn read_palette(
    file: &mut Cursor<Vec<u8>>,
    image_header: &ImageHeader,
) -> anyhow::Result<Vec<u16>, anyhow::Error> {
    let palette_size = PALETTE_SIZE / 2;
    let mut palette: Vec<u16> = vec![0; palette_size.into()];
    file.seek(SeekFrom::Start(
        (SKIP_OFFSET + image_header.header_len) as u64,
    ))?;
    for i in 0..palette_size {
        palette[i as usize] = buffer_utils::read_u16(file)?;
    }
    Ok(palette)
}

/// 读取每一帧的坐标
fn read_frame_offset(
    file: &mut Cursor<Vec<u8>>,
    image_header: &ImageHeader,
) -> anyhow::Result<Vec<u32>, anyhow::Error> {
    // 读取帧坐标时的偏移量
    let offset = SKIP_OFFSET + image_header.header_len + PALETTE_SIZE;

    // 帧坐标的长度 = 方向数 * 帧数 * 4
    let len = image_header.direction * image_header.frame_count * 4;
    let mut frame_offset: Vec<u8> = vec![0; len as usize];

    file.seek(SeekFrom::Start(offset as u64))?;
    file.read(&mut frame_offset)?;

    // 获取每一帧在文件中的位置偏移
    let frame_offset: Vec<u32> = frame_offset
        .chunks_exact(4)
        .map(|x| u32::from_le_bytes(x.try_into().unwrap()))
        .collect();

    Ok(frame_offset)
}

/// 读取每一帧的数据
fn read_frame(
    file: &mut Cursor<Vec<u8>>,
    frame_offsets: Vec<u32>,
    image_header: &ImageHeader,
) -> anyhow::Result<Vec<Frame>, anyhow::Error> {
    let mut result: Vec<Frame> = vec![];

    for i in 0..frame_offsets.len() {
        let offset: u32 = (frame_offsets[i])
            + (<u16 as Into<u32>>::into(image_header.header_len))
            + SKIP_OFFSET as u32;

        file.seek(SeekFrom::Start(offset as u64))?;
        let mut frame_x: [u8; 4] = [0; 4];
        let mut frame_y: [u8; 4] = [0; 4];
        let mut frame_width: [u8; 4] = [0; 4];
        let mut frame_height: [u8; 4] = [0; 4];
        file.read(&mut frame_x)?;
        file.read(&mut frame_y)?;
        file.read(&mut frame_width)?;
        file.read(&mut frame_height)?;

        let pixel_line_offsets_size = u32::from_le_bytes(frame_height) * 4;
        let mut pixel_line_offsets: Vec<u8> = vec![0; pixel_line_offsets_size as usize];
        file.read(&mut pixel_line_offsets)?;

        let line_offsets: Vec<u32> = pixel_line_offsets
            .chunks_exact(4)
            .map(|x| u32::from_le_bytes(x.try_into().unwrap()))
            .collect();

        result.push(Frame {
            x: u32::from_le_bytes(frame_x),
            y: u32::from_le_bytes(frame_y),
            width: u32::from_le_bytes(frame_width),
            height: u32::from_le_bytes(frame_height),
            line_offsets,
            offset: frame_offsets[i],
        });
    }

    Ok(result)
}

/// 读取每一帧的像素
fn read_pixel(
    frame: &Frame,
    image_header: &ImageHeader,
    file: &mut Cursor<Vec<u8>>,
    palette: &Vec<u16>,
) -> anyhow::Result<Vec<i64>, anyhow::Error> {
    let pixel_size = frame.width * frame.height;
    let mut pixel_data: Vec<i64> = vec![0; pixel_size as usize];

    for y in 0..frame.height {
        let line_offset = frame.line_offsets[y as usize]
            + frame.offset
            + image_header.header_len as u32
            + SKIP_OFFSET as u32;

        file.seek(SeekFrom::Start(line_offset as u64))?;

        let mut x = 0;
        while x < frame.width {
            let mut b = buffer_utils::read_u8(file)?;
            match b & TYPE_FLAG {
                PIXEL_TYPE_IS_ALPHA => {
                    if b & ALPHA_TYPE_IS_PIXEL > 0 {
                        let index = buffer_utils::read_u8(file)?;
                        let color = palette[index as usize];
                        let pixel_index = y * frame.width + x;
                        x = x + 1;
                        let flag = (b & 0x1F) as i64;
                        pixel_data[pixel_index as usize] = color as i64 + (flag << 16);
                    } else if b != 0 {
                        let count = b & 0x1F;
                        b = buffer_utils::read_u8(file)?;
                        let index = buffer_utils::read_u8(file)?;
                        let c = palette[index as usize];
                        for _ in 0..count {
                            let pixel_index = y * frame.width + x;
                            x = x + 1;
                            let flag: i64 = (b & 0x1F) as i64;
                            pixel_data[pixel_index as usize] = (c as i64) + (flag << 16);
                        }
                    } else {
                        if x > frame.width {
                            // 这里表示有问题
                            // console.error("block end error: [" + y + "][" + x + "/" + frameWidth + "]");
                            continue;
                        } else if x == 0 {
                        } else {
                            x = frame.width;
                        }
                    }
                }
                PIXEL_TYPE_IS_PIXELS => {
                    let count = b & 0x3F;
                    for _ in 0..count {
                        let index = buffer_utils::read_u8(file)?;
                        let pixel_index = y * frame.width + x;
                        x = x + 1;
                        pixel_data[pixel_index as usize] =
                            palette[index as usize] as i64 + (0x1f << 16) as i64;
                    }
                }
                PIXEL_TYPE_IS_REPEAT => {
                    let count = b & 0x3F;
                    let index = buffer_utils::read_u8(file)?;
                    let c = palette[index as usize];
                    for _ in 0..count {
                        let pixel_index = y * frame.width + x;
                        x = x + 1;
                        pixel_data[pixel_index as usize] = c as i64 + (0x1f << 16) as i64;
                    }
                }
                PIXEL_TYPE_IS_SKIP => {
                    let count = b & 0x3F;
                    x = x + count as u32;
                }
                _ => {}
            }
        }
    }

    Ok(pixel_data)
}

/// 将像素转换为图片
fn to_image(pixels: Vec<i64>, width: u32, height: u32) -> RgbaImage {
    let mut image = RgbaImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let index = y * width + x;
        let color = pixels[index as usize];
        let r = ((color >> 11) & 0x1F) << 3;
        let g = ((color >> 5) & 0x3F) << 2;
        let b = (color & 0x1F) << 3;
        let a = ((color >> 16) & 0x1F) << 3;
        *pixel = Rgba([r as u8, g as u8, b as u8, a as u8]);
    }

    image
}

/// 读取WAS文件并获取图片
/// 这个方法会读取整个文件，如果文件过大，可能会比较慢
/// 并且，这个方法会将整个文件内的图片全部读出来
/// # Example
/// ```
/// let (image_header, frames, images) = was::get_images("test.was").unwrap();
/// ```
pub fn get_images(file: &mut Cursor<Vec<u8>>) -> anyhow::Result<WasInfo> {
    if !check_file_format(file) {
        return Err(anyhow!("不是WAS文件"));
    }

    let image_header = read_imageheader(file)?;
    let palette = read_palette(file, &image_header)?;
    let frame_offset = read_frame_offset(file, &image_header)?;
    let frams = read_frame(file, frame_offset, &image_header)?;

    let mut image_buffers: Vec<ImageBuffer<Rgba<u8>, Vec<u8>>> = vec![];
    for i in 0..frams.len() {
        let pixel = read_pixel(&frams[i], &image_header, file, &palette)?;
        let image = to_image(pixel, frams[i].width, frams[i].height);

        image_buffers.push(image);
    }

    Ok((image_header, frams, image_buffers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode() {
        let file_path = "C:\\Users\\Tao Mao\\Documents\\Tencent Files\\138019260\\FileRecv\\magic2\\0012-13-0.was";
        let file = std::fs::read(file_path).unwrap();
        let mut file = Cursor::new(file);
        get_images(&mut file);
    }
}
