use crate::error::VtlError;
use crate::robot_sync::MAILSLOT_OFFSET;

const MAX_SLOTS: i32 = 1000;
const MAX_DRIVES: i32 = 100;

pub(crate) fn parse_size(size_str: &str) -> Result<u64, VtlError> {
    let upper_str = size_str.to_uppercase();
    let size_str = upper_str.trim();
    let mut num_str = String::new();
    let mut unit = String::new();

    for c in size_str.chars() {
        if c.is_ascii_digit() || c == '.' {
            num_str.push(c);
        } else {
            unit.push(c);
        }
    }

    let num: f64 = num_str
        .parse()
        .map_err(|_| VtlError::InvalidSize(format!("Invalid number: {}", num_str)))?;

    let multiplier = match unit.as_str() {
        "" | "B" => 1u64,
        "K" | "KB" => 1024u64,
        "M" | "MB" => 1024u64 * 1024,
        "G" | "GB" => 1024u64 * 1024 * 1024,
        "T" | "TB" => 1024u64 * 1024 * 1024 * 1024,
        _ => return Err(VtlError::InvalidSize(format!("Unknown unit: {}", unit))),
    };

    if !num.is_finite() || num < 0.0 {
        return Err(VtlError::InvalidSize(format!(
            "Invalid number: {}",
            num_str
        )));
    }
    let bytes = num * multiplier as f64;
    if !bytes.is_finite() || bytes > u64::MAX as f64 {
        return Err(VtlError::InvalidSize(format!(
            "Size too large: {}",
            size_str
        )));
    }
    Ok(bytes as u64)
}

pub(crate) fn parse_slot(slot_str: &str) -> Option<i32> {
    let slot_str = slot_str.trim().to_lowercase();
    let value = if slot_str.starts_with("slot") {
        slot_str.trim_start_matches("slot").parse().ok()?
    } else if slot_str.starts_with("mail") {
        let num = slot_str.trim_start_matches("mail").parse::<i32>().ok()?;
        return Some(MAILSLOT_OFFSET + num);
    } else {
        slot_str.parse().ok()?
    };

    if value < 0 || value > MAX_SLOTS {
        return None;
    }
    Some(value)
}

pub(crate) fn parse_drive(drive_str: &str) -> Option<i32> {
    let drive_str = drive_str.trim().to_lowercase();
    let value = if drive_str.starts_with("drive") {
        drive_str.trim_start_matches("drive").parse().ok()?
    } else {
        drive_str.parse().ok()?
    };

    if value < 0 || value > MAX_DRIVES {
        return None;
    }
    Some(value)
}

pub(crate) fn generate_barcode() -> String {
    format!("VTL{:06X}", rand::random::<u32>())
}

pub(crate) fn validate_tape_name(name: &str) -> Result<(), VtlError> {
    if name.is_empty() {
        return Err(VtlError::InvalidTapeName(
            "Tape name cannot be empty".to_string(),
        ));
    }
    if name == "." || name == ".." {
        return Err(VtlError::InvalidTapeName(
            "Tape name cannot be '.' or '..'".to_string(),
        ));
    }
    if name.len() > 255 {
        return Err(VtlError::InvalidTapeName(
            "Tape name too long (max 255 characters)".to_string(),
        ));
    }
    let invalid_chars = ['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\0'];
    for c in name.chars() {
        if invalid_chars.contains(&c) || c.is_control() {
            return Err(VtlError::InvalidTapeName(format!(
                "Invalid character in tape name: '{}'",
                c
            )));
        }
    }
    Ok(())
}

pub(crate) fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{}K", bytes / 1024)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{}M", bytes / (1024 * 1024))
    } else if bytes < 1024 * 1024 * 1024 * 1024 {
        format!("{}G", bytes / (1024 * 1024 * 1024))
    } else {
        format!("{}T", bytes / (1024 * 1024 * 1024 * 1024))
    }
}
