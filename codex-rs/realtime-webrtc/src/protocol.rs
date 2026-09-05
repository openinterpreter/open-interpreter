//! Bounded, same-build lifecycle protocol; this stage carries no audio or credentials.

use std::io;
use std::io::Read;

use serde::Deserialize;
use serde::Serialize;

pub const MAX_FRAME_BYTES: usize = 256;

#[derive(Debug, Deserialize, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Message {
    Hello { protocol: u32, build_commit: String },
    Ready {},
    Close {},
    Closed {},
}

pub fn encode_frame(message: &Message) -> io::Result<Vec<u8>> {
    let payload = serde_json::to_vec(message)?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(io::Error::other("voice frame exceeds limit"));
    }
    let mut frame = (payload.len() as u32).to_be_bytes().to_vec();
    frame.extend(payload);
    Ok(frame)
}

pub fn decode_frame(frame: &[u8]) -> io::Result<Option<Message>> {
    let [a, b, c, d, ..] = frame else {
        return Ok(None);
    };
    let length = u32::from_be_bytes([*a, *b, *c, *d]) as usize;
    if length > MAX_FRAME_BYTES || frame.len() > length + 4 {
        return Err(io::Error::other("invalid voice frame length"));
    }
    frame
        .get(4..length + 4)
        .map(serde_json::from_slice)
        .transpose()
        .map_err(|_| io::Error::other("invalid voice frame"))
}

pub fn read_message(reader: &mut impl Read) -> io::Result<Option<Message>> {
    let mut header = [0; 4];
    match reader.read_exact(&mut header[..1]) {
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        result => result?,
    }
    reader.read_exact(&mut header[1..])?;
    decode_frame(&header)?;
    let length = u32::from_be_bytes(header) as usize;
    let mut frame = header.to_vec();
    frame.resize(length + 4, /*value*/ 0);
    reader.read_exact(&mut frame[4..])?;
    decode_frame(&frame)
}
