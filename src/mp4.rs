use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

use tempfile::Builder;

use crate::error::{IoResultExt, MarkerFixerError, Result};

const BOX_HEADER_LEN: u64 = 8;
const BOX_EXTENDED_SIZE_LEN: u64 = 8;
const BOX_UUID_LEN: u64 = 16;
const ADOBE_XMP_UUID: [u8; 16] = [
    0xBE, 0x7A, 0xCF, 0xCB, 0x97, 0xA9, 0x42, 0xE8, 0x9C, 0x71, 0x99, 0x94, 0x91, 0xE3, 0xAF,
    0xAC,
];

#[derive(Debug, Clone)]
struct RootBox {
    start: u64,
    end: u64,
    box_type: [u8; 4],
    header_len: u64,
    uuid: Option<[u8; 16]>,
}

pub fn read_xmp_payload(path: &Path) -> Result<Option<Vec<u8>>> {
    let mut file = File::open(path).at_path(path)?;
    let file_len = file.metadata().at_path(path)?.len();
    let boxes = parse_root_boxes(&mut file, file_len)?;

    for root_box in boxes {
        if root_box.box_type == *b"uuid" && root_box.uuid == Some(ADOBE_XMP_UUID) {
            let payload_start = root_box.start + root_box.header_len;
            let payload_len = root_box.end.saturating_sub(payload_start);
            let mut payload = vec![0_u8; payload_len as usize];
            file.seek(SeekFrom::Start(payload_start)).at_path(path)?;
            file.read_exact(&mut payload).at_path(path)?;
            return Ok(Some(payload));
        }
    }

    Ok(None)
}

pub fn write_xmp_payload(input_path: &Path, output_path: &Path, xmp_payload: &[u8]) -> Result<()> {
    let mut source = File::open(input_path).at_path(input_path)?;
    let file_len = source.metadata().at_path(input_path)?.len();
    let boxes = parse_root_boxes(&mut source, file_len)?;

    let parent_dir = output_path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp_file = Builder::new()
        .prefix("marker-fixer-")
        .suffix(".tmp")
        .tempfile_in(parent_dir)
        .at_path(parent_dir)?;

    {
        let temp_path = temp_file.path().to_path_buf();
        let temp_inner = temp_file.as_file_mut();
        let mut writer = BufWriter::new(temp_inner);

        let mut reader = BufReader::new(File::open(input_path).at_path(input_path)?);

        if let Some(existing_box) = boxes
            .iter()
            .find(|candidate| candidate.box_type == *b"uuid" && candidate.uuid == Some(ADOBE_XMP_UUID))
        {
            copy_range(&mut reader, &mut writer, 0, existing_box.start, input_path)?;
            write_xmp_box(&mut writer, xmp_payload)?;
            copy_range(
                &mut reader,
                &mut writer,
                existing_box.end,
                file_len.saturating_sub(existing_box.end),
                input_path,
            )?;
        } else {
            copy_range(&mut reader, &mut writer, 0, file_len, input_path)?;
            write_xmp_box(&mut writer, xmp_payload)?;
        }

        writer.flush().at_path(&temp_path)?;
    }

    temp_file.persist(output_path).map_err(|err| MarkerFixerError::Io {
        path: output_path.to_path_buf(),
        source: err.error,
    })?;

    Ok(())
}

fn parse_root_boxes(file: &mut File, file_len: u64) -> Result<Vec<RootBox>> {
    let mut boxes = Vec::new();
    let mut offset = 0_u64;

    while offset + BOX_HEADER_LEN <= file_len {
        file.seek(SeekFrom::Start(offset)).map_err(|source| MarkerFixerError::Io {
            path: Path::new("<mp4>").to_path_buf(),
            source,
        })?;

        let mut header = [0_u8; 8];
        file.read_exact(&mut header)
            .map_err(|source| MarkerFixerError::Io { path: Path::new("<mp4>").to_path_buf(), source })?;

        let size32 = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as u64;
        let box_type = [header[4], header[5], header[6], header[7]];

        let (box_size, mut header_len) = if size32 == 1 {
            let mut ext_size_buf = [0_u8; 8];
            file.read_exact(&mut ext_size_buf).map_err(|source| MarkerFixerError::Io {
                path: Path::new("<mp4>").to_path_buf(),
                source,
            })?;
            (u64::from_be_bytes(ext_size_buf), BOX_HEADER_LEN + BOX_EXTENDED_SIZE_LEN)
        } else if size32 == 0 {
            (file_len.saturating_sub(offset), BOX_HEADER_LEN)
        } else {
            (size32, BOX_HEADER_LEN)
        };

        if box_size < header_len {
            return Err(MarkerFixerError::InvalidMp4(format!(
                "box at offset {offset} has invalid size {box_size}"
            )));
        }

        if offset + box_size > file_len {
            return Err(MarkerFixerError::InvalidMp4(format!(
                "box at offset {offset} exceeds file length"
            )));
        }

        let uuid = if box_type == *b"uuid" {
            if box_size < header_len + BOX_UUID_LEN {
                return Err(MarkerFixerError::InvalidMp4(format!(
                    "uuid box at offset {offset} is too short"
                )));
            }
            let mut uuid = [0_u8; 16];
            file.read_exact(&mut uuid).map_err(|source| MarkerFixerError::Io {
                path: Path::new("<mp4>").to_path_buf(),
                source,
            })?;
            header_len += BOX_UUID_LEN;
            Some(uuid)
        } else {
            None
        };

        boxes.push(RootBox {
            start: offset,
            end: offset + box_size,
            box_type,
            header_len,
            uuid,
        });

        offset += box_size;
    }

    Ok(boxes)
}

fn write_xmp_box(writer: &mut impl Write, xmp_payload: &[u8]) -> Result<()> {
    let total_size = BOX_HEADER_LEN + BOX_UUID_LEN + xmp_payload.len() as u64;
    if total_size > u32::MAX as u64 {
        return Err(MarkerFixerError::InvalidMp4(
            "xmp payload too large for 32-bit box size".to_string(),
        ));
    }

    writer
        .write_all(&(total_size as u32).to_be_bytes())
        .map_err(|source| MarkerFixerError::Io {
            path: Path::new("<output>").to_path_buf(),
            source,
        })?;
    writer.write_all(b"uuid").map_err(|source| MarkerFixerError::Io {
        path: Path::new("<output>").to_path_buf(),
        source,
    })?;
    writer.write_all(&ADOBE_XMP_UUID).map_err(|source| MarkerFixerError::Io {
        path: Path::new("<output>").to_path_buf(),
        source,
    })?;
    writer
        .write_all(xmp_payload)
        .map_err(|source| MarkerFixerError::Io {
            path: Path::new("<output>").to_path_buf(),
            source,
        })?;
    Ok(())
}

fn copy_range(reader: &mut BufReader<File>, writer: &mut BufWriter<&mut File>, start: u64, len: u64, src_path: &Path) -> Result<()> {
    reader.seek(SeekFrom::Start(start)).at_path(src_path)?;
    let mut take = reader.take(len);
    std::io::copy(&mut take, writer).map_err(|source| MarkerFixerError::Io {
        path: src_path.to_path_buf(),
        source,
    })?;
    Ok(())
}
