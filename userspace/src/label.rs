//! ANSI/IBM standard tape label writing.
//!
//! Writes labels directly to the tape file (.vtltape) using the VTL block format.
//! Filemarks are ephemeral in VTL (not persisted), but label data blocks are
//! written with the correct VTL_BLOCK_HEADER so the kernel READ path handles them.

use crate::{log_message, VtlError};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

const VTL_BLOCK_MAGIC: u32 = 0x564C5442;
const VTL_BLOCK_HEADER_SIZE: usize = 16;
const VTL_COMP_NONE: u8 = 0;

const LABEL_LEN: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LabelFormat {
    Ansi,
    Ibm,
}

impl LabelFormat {
    pub fn name(&self) -> &str {
        match self {
            LabelFormat::Ansi => "ANSI",
            LabelFormat::Ibm => "IBM",
        }
    }
}

/// Build a VTL block header (16 bytes, big-endian).
fn vtl_block_header(data: &[u8]) -> [u8; VTL_BLOCK_HEADER_SIZE] {
    let mut hdr = [0u8; VTL_BLOCK_HEADER_SIZE];
    let len = data.len() as u32;
    hdr[0..4].copy_from_slice(&VTL_BLOCK_MAGIC.to_be_bytes());
    hdr[4..8].copy_from_slice(&len.to_be_bytes());
    hdr[8..12].copy_from_slice(&len.to_be_bytes());
    hdr[12] = VTL_COMP_NONE;
    hdr
}

/// Space-pad a string to the given length.
fn pad_right(s: &str, len: usize) -> String {
    let mut out = s.to_string();
    out.truncate(len);
    while out.len() < len {
        out.push(' ');
    }
    out
}

/// Build an ANSI/IBM VOL1 label (80 bytes).
/// VOL1: volume header label.
fn build_vol1(volser: &str, owner: &str) -> [u8; LABEL_LEN] {
    let mut buf = [b' '; LABEL_LEN];
    buf[0..4].copy_from_slice(b"VOL1");
    buf[4] = b'1'; // label number
    let vs = pad_right(volser, 6);
    buf[5..11].copy_from_slice(vs.as_bytes());
    buf[11] = b'1'; // accessibility
    // bytes 12-36: reserved (already spaces)
    let ow = pad_right(owner, 14);
    buf[37..51].copy_from_slice(ow.as_bytes());
    // bytes 51-79: reserved (already spaces)
    buf
}

/// Build an ANSI/IBM HDR1 label (80 bytes).
/// HDR1: first file header label.
fn build_hdr1(file_id: &str) -> [u8; LABEL_LEN] {
    let mut buf = [b' '; LABEL_LEN];
    buf[0..4].copy_from_slice(b"HDR1");
    buf[4] = b'1'; // label number
    let fid = pad_right(file_id, 17);
    buf[5..22].copy_from_slice(fid.as_bytes());
    // file set id (6): spaces
    // file section number (4): "0001"
    buf[28..32].copy_from_slice(b"0001");
    // file sequence number (4): "0001"
    buf[32..36].copy_from_slice(b"0001");
    // generation number (4): "0001"
    buf[36..40].copy_from_slice(b"0001");
    // generation version (2): "00"
    buf[40..42].copy_from_slice(b"00");
    // creation date (6): spaces (today's date in YYDDD format could be used)
    // expiration date (6): spaces
    // accessibility (1): ' ' (space)
    // block count (6): "000000"
    buf[55..61].copy_from_slice(b"000000");
    // system code (13): spaces
    // reserved (6): spaces
    buf
}

/// Build an ANSI/IBM HDR2 label (80 bytes).
/// HDR2: second file header label.
fn build_hdr2(block_size: u32, density_code: u8) -> [u8; LABEL_LEN] {
    let mut buf = [b' '; LABEL_LEN];
    buf[0..4].copy_from_slice(b"HDR2");
    buf[4] = b'1'; // label number
    buf[5] = b'D'; // record format (D=variable, F=fixed, U=undefined)
    // block length (5): decimal, clamp to 5-digit max
    let bs = if block_size > 99999 {
        "99999".to_string()
    } else {
        format!("{:05}", block_size)
    };
    buf[6..11].copy_from_slice(bs.as_bytes());
    // record length (5): same as block for variable
    buf[11..16].copy_from_slice(bs.as_bytes());
    // bytes 16-31: reserved
    // bytes 32-34: tape density (0x40 = 64 decimal)
    // tape density (3-digit decimal, e.g. 0x40=64 → "064")
    let ds = format!("{:03}", density_code);
    buf[32..35].copy_from_slice(ds.as_bytes());
    // bytes 37-42: reserved/offset
    // bytes 43-79: reserved
    buf
}

/// Build a complete ANSI/IBM label set: VOL1 + HDR1 + HDR2.
/// Returns a Vec of blocks (each 80 bytes).
fn build_label_set(format: LabelFormat, volser: &str, owner: &str, block_size: u32, density_code: u8) -> Vec<[u8; LABEL_LEN]> {
    let vol1 = build_vol1(volser, owner);
    let file_id = match format {
        LabelFormat::Ansi => "VTLADM.TAPE",
        LabelFormat::Ibm => "VTLADM.TAPE",
    };
    let hdr1 = build_hdr1(file_id);
    let hdr2 = build_hdr2(block_size, density_code);
    vec![vol1, hdr1, hdr2]
}

/// Write a single label block (with VTL header) to the file.
fn write_label_block(file: &mut impl Write, block: &[u8; LABEL_LEN]) -> Result<(), VtlError> {
    let hdr = vtl_block_header(block);
    file.write_all(&hdr)
        .map_err(|e| VtlError::IoError(e))?;
    file.write_all(block)
        .map_err(|e| VtlError::IoError(e))?;
    Ok(())
}

/// Write ANSI/IBM standard tape labels to a tape file.
///
/// The label set consists of:
///   VOL1 (80 bytes)
///   HDR1 (80 bytes)
///   HDR2 (80 bytes)
///
/// Labels are written with VTL_BLOCK_HEADER (algorithm=NONE, i.e., passthrough)
/// so that subsequent READ commands return the raw label data without decompression.
///
/// The file is truncated and the labels are written at position 0 (BOT).
pub fn write_tape_labels(
    image_path: &str,
    format: LabelFormat,
    volser: &str,
    owner: &str,
    block_size: u32,
    density_code: u8,
) -> Result<(), VtlError> {
    let path = Path::new(image_path);
    let mut file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| VtlError::IoError(std::io::Error::new(
            e.kind(),
            format!("无法打开磁带镜像 {}: {}", image_path, e),
        )))?;

    // Truncate: clear existing data, then write labels at offset 0.
    // Block devices (zvol) don't support truncate — just overwrite from offset 0.
    let _ = file.set_len(0);

    let blocks = build_label_set(format, volser, owner, block_size, density_code);
    for (i, block) in blocks.iter().enumerate() {
        write_label_block(&mut file, block)?;
        log_message(&format!(
            "已写入 {} 标签块 {} ({:.4})",
            format.name(),
            match i {
                0 => "VOL1",
                1 => "HDR1",
                2 => "HDR2",
                _ => "???",
            },
            std::str::from_utf8(&block[0..4]).unwrap_or("????"),
        ));
    }

    file.sync_all().map_err(VtlError::from)?;
    log_message(&format!(
        "已为磁带写入 {} 标准标签 (卷号: {})",
        format.name(),
        volser
    ));
    Ok(())
}
