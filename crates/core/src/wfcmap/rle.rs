use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RleError {
    Malformed { at: usize, text: String },
    ZeroCount { at: usize },
    LengthMismatch { got: usize, expected: usize },
    TileOutOfRange { at: usize },
}

impl fmt::Display for RleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed { at, text } => write!(f, "malformed RLE run at byte {at}: {text:?}"),
            Self::ZeroCount { at } => write!(f, "zero RLE count at byte {at}"),
            Self::LengthMismatch { got, expected } => {
                write!(f, "RLE length {got} does not match expected {expected}")
            }
            Self::TileOutOfRange { at } => {
                write!(f, "RLE tile value out of u16 range at byte {at}")
            }
        }
    }
}

impl Error for RleError {}

/// Encodes row-major tile values as comma-separated decimal runs.
pub fn encode(tiles: &[u16]) -> String {
    let mut runs = Vec::new();
    let mut index = 0;

    while index < tiles.len() {
        let tile = tiles[index];
        let mut count = 1usize;
        while index + count < tiles.len() && tiles[index + count] == tile {
            count += 1;
        }

        let mut remaining = count;
        while remaining > 0 {
            let chunk = remaining.min(u32::MAX as usize);
            if chunk == 1 {
                runs.push(tile.to_string());
            } else {
                runs.push(format!("{tile}x{chunk}"));
            }
            remaining -= chunk;
        }
        index += count;
    }

    runs.join(",")
}

/// Decodes comma-separated decimal tile runs, enforcing the caller's tile count.
pub fn decode(s: &str, expected_len: usize) -> Result<Vec<u16>, RleError> {
    if s.trim().is_empty() {
        return if expected_len == 0 {
            Ok(Vec::new())
        } else {
            Err(RleError::LengthMismatch {
                got: 0,
                expected: expected_len,
            })
        };
    }

    let mut tiles = Vec::with_capacity(expected_len);
    let mut offset = 0;
    for raw_run in s.split(',') {
        let run_start = offset;
        offset += raw_run.len() + 1;
        let run = raw_run.trim();
        if run.is_empty() {
            return Err(RleError::Malformed {
                at: run_start,
                text: raw_run.to_string(),
            });
        }

        let (tile_text, count, count_at) = match run.split_once('x') {
            Some((tile, count_text)) if !count_text.contains('x') => {
                let count_at = run_start + raw_run.find('x').unwrap_or(0) + 1;
                let count = parse_decimal(count_text, count_at)?;
                if count == 0 {
                    return Err(RleError::ZeroCount { at: count_at });
                }
                (tile, count as usize, count_at)
            }
            Some(_) => {
                return Err(RleError::Malformed {
                    at: run_start,
                    text: raw_run.to_string(),
                });
            }
            None => (run, 1, run_start),
        };

        let tile = parse_decimal(tile_text, run_start)?;
        if tile > u16::MAX as u32 {
            return Err(RleError::TileOutOfRange { at: run_start });
        }
        let new_len = tiles
            .len()
            .checked_add(count)
            .ok_or(RleError::LengthMismatch {
                got: usize::MAX,
                expected: expected_len,
            })?;
        if new_len > expected_len {
            return Err(RleError::LengthMismatch {
                got: new_len,
                expected: expected_len,
            });
        }
        let _ = count_at;
        tiles.extend(std::iter::repeat_n(tile as u16, count));
    }

    if tiles.len() != expected_len {
        return Err(RleError::LengthMismatch {
            got: tiles.len(),
            expected: expected_len,
        });
    }
    Ok(tiles)
}

fn parse_decimal(text: &str, at: usize) -> Result<u32, RleError> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(RleError::Malformed {
            at,
            text: text.to_string(),
        });
    }
    text.parse::<u32>().map_err(|_| RleError::Malformed {
        at,
        text: text.to_string(),
    })
}
