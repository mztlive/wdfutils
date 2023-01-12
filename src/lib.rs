use std::{
    fs,
    io::{Cursor, Seek},
    time::Instant,
};

use was::WasInfo;

pub mod buffer_utils;
pub mod was;
pub mod wdf;

/// 加载was文件
pub fn load_was(wdf_file: &str, was_key: &u32) -> anyhow::Result<WasInfo> {
    let start = Instant::now();

    let files = wdf::decode(wdf_file)?;

    if files.contains_key(was_key) == false {
        return Err(anyhow::anyhow!("was文件不存在"));
    }

    let file = &files[was_key];
    let mut buffer = fs::File::open(wdf_file)?;
    buffer.seek(std::io::SeekFrom::Start(file.offset as u64))?;
    let data = buffer_utils::read_bytes(&mut buffer, file.size as usize)?;
    let mut data = Cursor::new(data);
    let images = was::get_images(&mut data)?;
    println!("耗时：{}ms", start.elapsed().as_millis());

    Ok(images)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_geet_was() {
        let file_path: &str = "gj.wdf";

        let was_key: u32 = 1577923263;
        load_was(file_path, &was_key);
    }
}
