use std::{
    fs,
    io::Read,
    io::{Seek, SeekFrom},
};

use crate::buffer_utils;

pub enum FileType {
    // was | tcp
    WAS(u8),
    JPG(u8),
    BMP(u8),
    TGA(u8),
    WAV(u8),
    MP3(u8),
    LUA(u8),
    Unknown(u8),
}

pub struct FileInfo {
    pub uid: u32,
    pub offset: u32,
    pub size: u32,
    pub space: u32,
    pub file_type: FileType,
}

type WdfFileMap = std::collections::HashMap<u32, FileInfo>;

pub fn decode(file_path: &str) -> anyhow::Result<WdfFileMap> {
    let mut file = fs::File::open(file_path)?;

    let _ = buffer_utils::read_u32(&mut file)?; // 这个值是文件头里面的flag, 忽略返回相当于前进4个字节
    let file_num = buffer_utils::read_u32(&mut file)?;
    let offset = buffer_utils::read_u32(&mut file)?;

    file.seek(SeekFrom::Start(offset as u64))?;
    let mut file_list = get_filelist(file_num, &mut file)?;
    get_filetype(&mut file_list, file)?;
    Ok(file_list)
}

/// 获取文件类型
fn get_filetype(
    file_list: &mut std::collections::HashMap<u32, FileInfo>,
    mut file: fs::File,
) -> Result<(), anyhow::Error> {
    Ok(for (_, info) in file_list.iter_mut() {
        file.seek(SeekFrom::Start(info.offset as u64))?;
        let hdw = buffer_utils::read_u16(&mut file)?;
        // 向后移动4个字节
        file.seek(SeekFrom::Current(4))?;
        let sst = buffer_utils::read_u32(&mut file)?;
        // 向前移动两个字节
        file.seek(SeekFrom::Current(-2))?;
        let nst = buffer_utils::read_u32(&mut file)?;
        file.seek(SeekFrom::Start((info.offset + info.size - 6) as u64))?;

        let dss = buffer_utils::read_u32(&mut file)?;
        file.seek(SeekFrom::Start((info.offset + info.size - 3) as u64))?;

        let dsg = buffer_utils::read_bytes(&mut file, 3)?;
        file.seek(SeekFrom::Start((info.offset + 4) as u64))?;

        let sss = buffer_utils::read_u16(&mut file)?;

        transfer_filetype(hdw, info, sst, dss, nst, dsg, sss);
    })
}

/// 获取文件类型
fn transfer_filetype(
    hdw: u16,
    info: &mut FileInfo,
    sst: u32,
    dss: u32,
    nst: u32,
    dsg: Vec<u8>,
    sss: u16,
) {
    if hdw == 0x5053 {
        info.file_type = FileType::WAS(1);
    } else if hdw == 0x4d42 {
        info.file_type = FileType::BMP(6);
    } else if sst == 0x49464A10 {
        info.file_type = FileType::JPG(3);
    } else if dss == 0x454C4946 {
        info.file_type = FileType::TGA(4);
    } else if hdw == 0x4952 && nst == 0x45564157 {
        info.file_type = FileType::WAV(5);
    } else if hdw == 0x00FF {
        info.file_type = FileType::MP3(2);
    } else if dsg[0] == 0x11 && dsg[1] == 0x00 && dsg[2] == 0x00 && sss == 0x1000 || sss == 0x0f00 {
        info.file_type = FileType::LUA(7);
    } else {
        info.file_type = FileType::Unknown(0);
    }
}

/// 获取文件列表
fn get_filelist(
    file_num: u32,
    file: &mut fs::File,
) -> Result<std::collections::HashMap<u32, FileInfo>, anyhow::Error> {
    let mut file_list: WdfFileMap = WdfFileMap::new();
    for _ in 0..file_num {
        let mut info_data: [u8; 16] = [0; 16];
        file.read(&mut info_data)?;

        let info = FileInfo {
            uid: u32::from_le_bytes(info_data[0..4].try_into()?),
            offset: u32::from_le_bytes(info_data[4..8].try_into()?),
            size: u32::from_le_bytes(info_data[8..12].try_into()?),
            space: u32::from_le_bytes(info_data[12..16].try_into()?),
            file_type: FileType::Unknown(0),
        };

        file_list.insert(info.uid, info);
    }
    Ok(file_list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode() {
        let file_path: &str = "gj.wdf";
        let file_list = decode(file_path).unwrap();
        assert_eq!(file_list.len() > 0, true);
    }
}
