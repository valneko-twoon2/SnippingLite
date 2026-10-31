//! Reading and writing the bytes of a `.rec` file.

#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    Text(String),
    Number(i64),
}

#[derive(Debug, PartialEq)]
pub enum Error {
    BadMagic,
    Truncated,
    BadKind(u8),
    BadText,
    BadNumber,
    TooMany,
    TooLong,
    Trailing,
}

pub const MAX_RECORDS: usize = 1000;
pub const MAX_TEXT: usize = 4096;

pub fn decode(bytes: &[u8]) -> Result<Vec<Record>, Error> {
    if bytes.len() < 6 || &bytes[..4] != b"REC1" {
        return Err(Error::BadMagic);
    }
    let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    if count > MAX_RECORDS {
        return Err(Error::TooMany);
    }
    let mut at = 6;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if at + 3 > bytes.len() {
            return Err(Error::Truncated);
        }
        let kind = bytes[at];
        let len = u16::from_le_bytes([bytes[at + 1], bytes[at + 2]]) as usize;
        at += 3;
        let payload = &bytes[at..at + len];
        at += len;
        match kind {
            1 => {
                if len > MAX_TEXT {
                    return Err(Error::TooLong);
                }
                let text = String::from_utf8(payload.to_vec()).map_err(|_| Error::BadText)?;
                out.push(Record::Text(text));
            }
            2 => {
                if len != 8 {
                    return Err(Error::BadNumber);
                }
                out.push(Record::Number(i64::from_le_bytes(payload.try_into().unwrap())));
            }
            k => return Err(Error::BadKind(k)),
        }
    }
    Ok(out)
}

pub fn encode(records: &[Record]) -> Vec<u8> {
    let mut out = b"REC1".to_vec();
    out.extend((records.len() as u16).to_le_bytes());
    for r in records {
        match r {
            Record::Text(s) => {
                out.push(1);
                out.extend((s.len() as u16).to_le_bytes());
                out.extend(s.as_bytes());
            }
            Record::Number(n) => {
                out.push(2);
                out.extend(8u16.to_le_bytes());
                out.extend(n.to_le_bytes());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_come_back_as_written() {
        let records = vec![Record::Text("hello".into()), Record::Number(-7)];
        assert_eq!(decode(&encode(&records)).unwrap(), records);
    }

    #[test]
    fn a_file_without_the_magic_is_refused() {
        assert_eq!(decode(b"NOPE\x00\x00"), Err(Error::BadMagic));
    }
}
