//! UTF-8 URI transformations (stdlib.md §19.3–§19.4).

const RESERVED: &[u8] = b";,/?:@&=+$#";
const HEX: &[u8] = b"0123456789ABCDEF";

pub(crate) fn encode(input: &[u8], component: bool) -> Vec<u8> {
    let mut output = Vec::new();
    for &byte in input {
        if byte.is_ascii_alphanumeric()
            || b"-_.!~*'()".contains(&byte)
            || (!component && RESERVED.contains(&byte))
        {
            output.push(byte);
        } else {
            output.extend_from_slice(&[b'%', HEX[(byte >> 4) as usize], HEX[(byte & 15) as usize]]);
        }
    }
    output
}

fn escaped(input: &[u8], offset: usize) -> Option<u8> {
    if input.get(offset) != Some(&b'%') {
        return None;
    }
    let hex = |byte: u8| (byte as char).to_digit(16).map(|value| value as u8);
    Some((hex(*input.get(offset + 1)?)? << 4) | hex(*input.get(offset + 2)?)?)
}

pub(crate) fn decode(input: &[u8], component: bool) -> Result<Vec<u8>, usize> {
    let mut output = Vec::new();
    let mut offset = 0;
    while offset < input.len() {
        if input[offset] != b'%' {
            output.push(input[offset]);
            offset += 1;
            continue;
        }
        let start = offset;
        let first = escaped(input, start).ok_or(start)?;
        let width = match first {
            0..=0x7f => 1,
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => return Err(start),
        };
        let mut sequence = [0; 4];
        sequence[0] = first;
        for (index, byte) in sequence.iter_mut().enumerate().take(width).skip(1) {
            *byte = escaped(input, start + 3 * index).ok_or(start)?;
        }
        std::str::from_utf8(&sequence[..width]).map_err(|_| start)?;
        offset += 3 * width;
        if !component && RESERVED.contains(&first) {
            output.extend_from_slice(&input[start..offset]);
        } else {
            output.extend_from_slice(&sequence[..width]);
        }
    }
    Ok(output)
}

pub(crate) fn failure(input: &[u8], component: bool) -> String {
    match decode(input, component) {
        Ok(_) => String::new(),
        Err(offset) => format!(
            "{}: malformed escape at byte {offset}",
            if component {
                "decodeURIComponent"
            } else {
                "decodeURI"
            }
        ),
    }
}

#[cfg(test)]
mod tests;
