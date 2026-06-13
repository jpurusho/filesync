use bytes::{Buf, BufMut, BytesMut};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Decoder, Encoder, Framed};

use crate::Error;

const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024; // 16 MiB
const HEADER_SIZE: usize = 5; // 4 bytes length + 1 byte message type

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    RpcRequest = 0x01,
    RpcResponse = 0x02,
    FileData = 0x03,
    Error = 0x04,
    Shutdown = 0xFF,
}

impl MessageType {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(Self::RpcRequest),
            0x02 => Some(Self::RpcResponse),
            0x03 => Some(Self::FileData),
            0x04 => Some(Self::Error),
            0xFF => Some(Self::Shutdown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub msg_type: MessageType,
    pub payload: Vec<u8>,
}

pub struct SyncCodec;

impl Decoder for SyncCodec {
    type Item = Frame;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < HEADER_SIZE {
            return Ok(None);
        }

        let len = u32::from_be_bytes([src[0], src[1], src[2], src[3]]) as usize;
        if len > MAX_FRAME_SIZE {
            return Err(Error::Transport(format!(
                "frame too large: {len} bytes (max {MAX_FRAME_SIZE})"
            )));
        }

        let total = HEADER_SIZE + len;
        if src.len() < total {
            src.reserve(total - src.len());
            return Ok(None);
        }

        src.advance(4); // consume length
        let msg_type_byte = src[0];
        src.advance(1); // consume type

        let msg_type = MessageType::from_u8(msg_type_byte).ok_or_else(|| {
            Error::Transport(format!("unknown message type: 0x{msg_type_byte:02x}"))
        })?;

        let payload = src.split_to(len).to_vec();

        Ok(Some(Frame { msg_type, payload }))
    }
}

impl Encoder<Frame> for SyncCodec {
    type Error = Error;

    fn encode(&mut self, item: Frame, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let len = item.payload.len();
        if len > MAX_FRAME_SIZE {
            return Err(Error::Transport(format!(
                "payload too large: {len} bytes (max {MAX_FRAME_SIZE})"
            )));
        }

        dst.reserve(HEADER_SIZE + len);
        dst.put_u32(len as u32);
        dst.put_u8(item.msg_type as u8);
        dst.put_slice(&item.payload);
        Ok(())
    }
}

pub type FramedStream<S> = Framed<S, SyncCodec>;

pub fn framed<S: AsyncRead + AsyncWrite>(stream: S) -> FramedStream<S> {
    Framed::new(stream, SyncCodec)
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use tokio_util::codec::{Decoder, Encoder};

    use super::*;

    #[test]
    fn roundtrip_frame() {
        let mut codec = SyncCodec;
        let frame = Frame {
            msg_type: MessageType::RpcRequest,
            payload: b"hello".to_vec(),
        };

        let mut buf = BytesMut::new();
        codec.encode(frame.clone(), &mut buf).unwrap();

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded.msg_type, MessageType::RpcRequest);
        assert_eq!(decoded.payload, b"hello");
    }

    #[test]
    fn partial_read() {
        let mut codec = SyncCodec;
        let frame = Frame {
            msg_type: MessageType::RpcResponse,
            payload: vec![1, 2, 3, 4, 5],
        };

        let mut full = BytesMut::new();
        codec.encode(frame, &mut full).unwrap();

        // Feed only partial data
        let mut partial = full.split_to(3);
        assert!(codec.decode(&mut partial).unwrap().is_none());

        // Feed the rest
        partial.unsplit(full);
        let decoded = codec.decode(&mut partial).unwrap().unwrap();
        assert_eq!(decoded.msg_type, MessageType::RpcResponse);
        assert_eq!(decoded.payload, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn rejects_oversized_frame() {
        let mut codec = SyncCodec;
        let mut buf = BytesMut::new();
        // Write a length that exceeds MAX_FRAME_SIZE
        buf.put_u32((MAX_FRAME_SIZE + 1) as u32);
        buf.put_u8(0x01);
        buf.put_bytes(0, 10);

        let result = codec.decode(&mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_unknown_message_type() {
        let mut codec = SyncCodec;
        let mut buf = BytesMut::new();
        buf.put_u32(3); // payload length
        buf.put_u8(0xAB); // unknown type
        buf.put_slice(&[1, 2, 3]);

        let result = codec.decode(&mut buf);
        assert!(result.is_err());
    }
}
