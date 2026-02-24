use bytes::BytesMut;
use tokio_util::codec::{Decoder, Encoder};

use crate::messages::RawMessage;

/// Newline-delimited JSON-RPC codec per ZIP 301.
/// Each message is a JSON object terminated by `\n`.
#[derive(Debug, Default)]
pub struct StratumCodec;

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Line too long (>{0} bytes)")]
    LineTooLong(usize),
}

const MAX_LINE_LENGTH: usize = 64 * 1024;

impl Decoder for StratumCodec {
    type Item = RawMessage;
    type Error = CodecError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(pos) = src.iter().position(|&b| b == b'\n') {
            let line = src.split_to(pos + 1);
            let line = &line[..line.len() - 1]; // strip \n
            if line.is_empty() {
                return Ok(None);
            }
            let msg: RawMessage = serde_json::from_slice(line)?;
            Ok(Some(msg))
        } else if src.len() > MAX_LINE_LENGTH {
            Err(CodecError::LineTooLong(MAX_LINE_LENGTH))
        } else {
            Ok(None)
        }
    }
}

impl Encoder<String> for StratumCodec {
    type Error = CodecError;

    fn encode(&mut self, item: String, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.extend_from_slice(item.as_bytes());
        dst.extend_from_slice(b"\n");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_single_message() {
        let mut codec = StratumCodec;
        let mut buf = BytesMut::from(
            r#"{"id":1,"method":"mining.subscribe","params":["Agent/1.0"]}"#,
        );
        buf.extend_from_slice(b"\n");

        let msg = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(msg.method.as_deref(), Some("mining.subscribe"));
    }

    #[test]
    fn decode_partial_then_complete() {
        let mut codec = StratumCodec;
        let mut buf = BytesMut::from(r#"{"id":1,"method":"min"#);

        assert!(codec.decode(&mut buf).unwrap().is_none());

        buf.extend_from_slice(br#"ing.subscribe","params":[]}"#);
        buf.extend_from_slice(b"\n");

        let msg = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(msg.method.as_deref(), Some("mining.subscribe"));
    }

    #[test]
    fn encode_adds_newline() {
        let mut codec = StratumCodec;
        let mut buf = BytesMut::new();
        codec.encode(r#"{"id":null,"method":"mining.notify","params":[]}"#.to_string(), &mut buf).unwrap();
        assert!(buf.ends_with(b"\n"));
    }
}
