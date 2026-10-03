// SPDX-License-Identifier: MIT
use crate::frame::{self, Frame, Kind, Reason};
use crate::frame::{
    DEFAULT_MAX_FRAME_SIZE, DEFAULT_SETTINGS_HEADER_TABLE_SIZE, MAX_MAX_FRAME_SIZE,
};
use crate::proto::Error;
use crate::tracing;

use crate::hpack;

use crate::poll_stream::Stream;

use bytes::{Buf, BytesMut};

use std::io;

use crate::io::{AsyncRead, ReadBuf};
use std::pin::Pin;
use std::task::{Context, Poll};

// 16 MB "sane default" taken from golang http2
const DEFAULT_SETTINGS_MAX_HEADER_LIST_SIZE: usize = 16 << 20;

// The first capacity of the buffer of the bytes read.
const INITIAL_READ_CAPACITY: usize = 8 * 1024;

#[derive(Debug)]
pub struct FramedRead<T> {
    inner: T,

    // The bytes read and not yet framed.
    buf: BytesMut,

    // The largest payload a frame may announce, SETTINGS_MAX_FRAME_SIZE.
    max_frame_size: usize,

    // The length of the frame whose header was read, its header included.
    frame_len: Option<usize>,

    // The state of the reads, as a length delimited reader keeps it: the IO
    // answered the end, the buffer may hold a frame, an error was returned.
    eof: bool,
    readable: bool,
    errored: bool,

    // hpack decoder state
    hpack: hpack::Decoder,

    max_header_list_size: usize,

    max_continuation_frames: usize,

    partial: Option<Partial>,
}

/// Partially loaded headers frame
#[derive(Debug)]
struct Partial {
    /// Empty frame
    frame: Continuable,

    /// Partial header payload
    buf: BytesMut,

    continuation_frames_count: usize,
}

#[derive(Debug)]
enum Continuable {
    Headers(frame::Headers),
    PushPromise(frame::PushPromise),
}

impl<T> FramedRead<T> {
    pub fn new(inner: T) -> FramedRead<T> {
        let max_header_list_size = DEFAULT_SETTINGS_MAX_HEADER_LIST_SIZE;
        let max_frame_size = DEFAULT_MAX_FRAME_SIZE as usize;
        let max_continuation_frames =
            calc_max_continuation_frames(max_header_list_size, max_frame_size);
        FramedRead {
            inner,
            buf: BytesMut::with_capacity(INITIAL_READ_CAPACITY),
            max_frame_size,
            frame_len: None,
            eof: false,
            readable: false,
            errored: false,
            hpack: hpack::Decoder::new(DEFAULT_SETTINGS_HEADER_TABLE_SIZE),
            max_header_list_size,
            max_continuation_frames,
            partial: None,
        }
    }

    pub fn get_ref(&self) -> &T {
        &self.inner
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Returns the current max frame size setting
    #[inline]
    pub fn max_frame_size(&self) -> usize {
        self.max_frame_size
    }

    /// Updates the max frame size setting.
    ///
    /// Must be within 16,384 and 16,777,215.
    #[inline]
    pub fn set_max_frame_size(&mut self, val: usize) {
        assert!(DEFAULT_MAX_FRAME_SIZE as usize <= val && val <= MAX_MAX_FRAME_SIZE as usize);
        self.max_frame_size = val;
        // Update max CONTINUATION frames too, since its based on this
        self.max_continuation_frames = calc_max_continuation_frames(self.max_header_list_size, val);
    }

    /// Update the max header list size setting.
    #[inline]
    pub fn set_max_header_list_size(&mut self, val: usize) {
        self.max_header_list_size = val;
        // Update max CONTINUATION frames too, since its based on this
        self.max_continuation_frames = calc_max_continuation_frames(val, self.max_frame_size());
    }

    /// Update the header table size setting.
    #[inline]
    pub fn set_header_table_size(&mut self, val: usize) {
        self.hpack.queue_size_update(val);
    }
}

fn calc_max_continuation_frames(header_max: usize, frame_max: usize) -> usize {
    // At least this many frames needed to use max header list size
    let min_frames_for_list = (header_max / frame_max).max(1);
    // Some padding for imperfectly packed frames
    // 25% without floats
    let padding = min_frames_for_list >> 2;
    min_frames_for_list.saturating_add(padding).max(5)
}

/// Decodes a frame.
///
/// This method is intentionally de-generified and outlined because it is very large.
fn decode_frame(
    hpack: &mut hpack::Decoder,
    max_header_list_size: usize,
    max_continuation_frames: usize,
    partial_inout: &mut Option<Partial>,
    mut bytes: BytesMut,
) -> Result<Option<Frame>, Error> {
    let span = tracing::trace_span!("FramedRead::decode_frame", offset = bytes.len());
    let _e = span.enter();

    tracing::trace!("decoding frame from {}B", bytes.len());

    // Parse the head
    let head = frame::Head::parse(&bytes);

    if partial_inout.is_some() && head.kind() != Kind::Continuation {
        proto_err!(conn: "expected CONTINUATION, got {:?}", head.kind());
        return Err(Error::library_go_away(Reason::PROTOCOL_ERROR));
    }

    let kind = head.kind();

    tracing::trace!(frame.kind = ?kind);

    macro_rules! header_block {
        ($frame:ident, $head:ident, $bytes:ident) => ({
            // Drop the frame header
            $bytes.advance(frame::HEADER_LEN);

            // Parse the header frame w/o parsing the payload
            let (mut frame, mut payload) = match frame::$frame::load($head, $bytes) {
                Ok(res) => res,
                Err(frame::Error::InvalidDependencyId) => {
                    proto_err!(stream: "invalid HEADERS dependency ID");
                    // A stream cannot depend on itself. An endpoint MUST
                    // treat this as a stream error (Section 5.4.2) of type
                    // `PROTOCOL_ERROR`.
                    return Err(Error::library_reset($head.stream_id(), Reason::PROTOCOL_ERROR));
                },
                Err(_e) => {
                    proto_err!(conn: "failed to load frame; err={:?}", e);
                    return Err(Error::library_go_away(Reason::PROTOCOL_ERROR));
                }
            };

            let is_end_headers = frame.is_end_headers();

            // Load the HPACK encoded headers
            match frame.load_hpack(&mut payload, max_header_list_size, hpack) {
                Ok(_) => {},
                Err(frame::Error::Hpack(hpack::DecoderError::NeedMore(_))) if !is_end_headers => {},
                Err(frame::Error::MalformedMessage) => {
                    let id = $head.stream_id();
                    proto_err!(stream: "malformed header block; stream={:?}", id);
                    return Err(Error::library_reset(id, Reason::PROTOCOL_ERROR));
                },
                Err(frame::Error::HeaderListWayTooLarge) => {
                    proto_err!(conn: "decoded header list size over abuse limit");
                    return Err(Error::library_go_away_data(
                        Reason::ENHANCE_YOUR_CALM,
                        "header_list_way_too_large",
                    ));
                },
                Err(_e) => {
                    proto_err!(conn: "failed HPACK decoding; err={:?}", e);
                    return Err(Error::library_go_away(Reason::PROTOCOL_ERROR));
                }
            }

            if is_end_headers {
                frame.into()
            } else {
                tracing::trace!("loaded partial header block");
                // Defer returning the frame
                *partial_inout = Some(Partial {
                    frame: Continuable::$frame(frame),
                    buf: payload,
                    continuation_frames_count: 0,
                });

                return Ok(None);
            }
        });
    }

    let frame = match kind {
        Kind::Settings => {
            let res = frame::Settings::load(head, &bytes[frame::HEADER_LEN..]);

            res.map_err(|_e| {
                proto_err!(conn: "failed to load SETTINGS frame; err={:?}", e);
                Error::library_go_away(Reason::PROTOCOL_ERROR)
            })?
            .into()
        }
        Kind::Ping => {
            let res = frame::Ping::load(head, &bytes[frame::HEADER_LEN..]);

            res.map_err(|_e| {
                proto_err!(conn: "failed to load PING frame; err={:?}", e);
                Error::library_go_away(Reason::PROTOCOL_ERROR)
            })?
            .into()
        }
        Kind::WindowUpdate => {
            let res = frame::WindowUpdate::load(head, &bytes[frame::HEADER_LEN..]);

            res.map_err(|_e| {
                proto_err!(conn: "failed to load WINDOW_UPDATE frame; err={:?}", e);
                Error::library_go_away(Reason::PROTOCOL_ERROR)
            })?
            .into()
        }
        Kind::Data => {
            bytes.advance(frame::HEADER_LEN);
            let res = frame::Data::load(head, bytes.freeze());

            // TODO: Should this always be connection level? Probably not...
            res.map_err(|_e| {
                proto_err!(conn: "failed to load DATA frame; err={:?}", e);
                Error::library_go_away(Reason::PROTOCOL_ERROR)
            })?
            .into()
        }
        Kind::Headers => header_block!(Headers, head, bytes),
        Kind::Reset => {
            let res = frame::Reset::load(head, &bytes[frame::HEADER_LEN..]);
            res.map_err(|_e| {
                proto_err!(conn: "failed to load RESET frame; err={:?}", e);
                Error::library_go_away(Reason::PROTOCOL_ERROR)
            })?
            .into()
        }
        Kind::GoAway => {
            let res = frame::GoAway::load(&bytes[frame::HEADER_LEN..]);
            res.map_err(|_e| {
                proto_err!(conn: "failed to load GO_AWAY frame; err={:?}", e);
                Error::library_go_away(Reason::PROTOCOL_ERROR)
            })?
            .into()
        }
        Kind::PushPromise => header_block!(PushPromise, head, bytes),
        Kind::Priority => {
            if head.stream_id() == 0 {
                // Invalid stream identifier
                proto_err!(conn: "invalid stream ID 0");
                return Err(Error::library_go_away(Reason::PROTOCOL_ERROR));
            }

            match frame::Priority::load(head, &bytes[frame::HEADER_LEN..]) {
                Ok(frame) => frame.into(),
                Err(frame::Error::InvalidDependencyId) => {
                    // A stream cannot depend on itself. An endpoint MUST
                    // treat this as a stream error (Section 5.4.2) of type
                    // `PROTOCOL_ERROR`.
                    let id = head.stream_id();
                    proto_err!(stream: "PRIORITY invalid dependency ID; stream={:?}", id);
                    return Err(Error::library_reset(id, Reason::PROTOCOL_ERROR));
                }
                Err(_e) => {
                    proto_err!(conn: "failed to load PRIORITY frame; err={:?};", e);
                    return Err(Error::library_go_away(Reason::PROTOCOL_ERROR));
                }
            }
        }
        Kind::Continuation => {
            let is_end_headers = (head.flag() & 0x4) == 0x4;

            let mut partial = match partial_inout.take() {
                Some(partial) => partial,
                None => {
                    proto_err!(conn: "received unexpected CONTINUATION frame");
                    return Err(Error::library_go_away(Reason::PROTOCOL_ERROR));
                }
            };

            // The stream identifiers must match
            if partial.frame.stream_id() != head.stream_id() {
                proto_err!(conn: "CONTINUATION frame stream ID does not match previous frame stream ID");
                return Err(Error::library_go_away(Reason::PROTOCOL_ERROR));
            }

            // Check for CONTINUATION flood
            if is_end_headers {
                partial.continuation_frames_count = 0;
            } else {
                let cnt = partial.continuation_frames_count + 1;
                if cnt > max_continuation_frames {
                    tracing::debug!("too_many_continuations, max = {}", max_continuation_frames);
                    return Err(Error::library_go_away_data(
                        Reason::ENHANCE_YOUR_CALM,
                        "too_many_continuations",
                    ));
                } else {
                    partial.continuation_frames_count = cnt;
                }
            }

            // Extend the buf
            if partial.buf.is_empty() {
                partial.buf = bytes.split_off(frame::HEADER_LEN);
            } else {
                if partial.frame.is_over_size() {
                    // If there was left over bytes previously, they may be
                    // needed to continue decoding, even though we will
                    // be ignoring this frame. This is done to keep the HPACK
                    // decoder state up-to-date.
                    //
                    // Still, we need to be careful, because if a malicious
                    // attacker were to try to send a gigantic string, such
                    // that it fits over multiple header blocks, we could
                    // grow memory uncontrollably again, and that'd be a shame.
                    //
                    // Instead, we use a simple heuristic to determine if
                    // we should continue to ignore decoding, or to tell
                    // the attacker to go away.
                    if partial.buf.len() + bytes.len() > max_header_list_size {
                        proto_err!(conn: "CONTINUATION frame header block size over ignorable limit");
                        return Err(Error::library_go_away(Reason::COMPRESSION_ERROR));
                    }
                }
                partial.buf.extend_from_slice(&bytes[frame::HEADER_LEN..]);
            }

            match partial
                .frame
                .load_hpack(&mut partial.buf, max_header_list_size, hpack)
            {
                Ok(_) => {}
                Err(frame::Error::Hpack(hpack::DecoderError::NeedMore(_))) if !is_end_headers => {}
                Err(frame::Error::MalformedMessage) => {
                    let id = head.stream_id();
                    proto_err!(stream: "malformed CONTINUATION frame; stream={:?}", id);
                    return Err(Error::library_reset(id, Reason::PROTOCOL_ERROR));
                }
                Err(frame::Error::HeaderListWayTooLarge) => {
                    proto_err!(conn: "decoded CONTINUATION header list size over abuse limit");
                    return Err(Error::library_go_away_data(
                        Reason::ENHANCE_YOUR_CALM,
                        "header_list_way_too_large",
                    ));
                }
                Err(_e) => {
                    proto_err!(conn: "failed HPACK decoding; err={:?}", e);
                    return Err(Error::library_go_away(Reason::PROTOCOL_ERROR));
                }
            }

            if is_end_headers {
                partial.frame.into()
            } else {
                *partial_inout = Some(partial);
                return Ok(None);
            }
        }
        Kind::Unknown => {
            // Unknown frames are ignored
            return Ok(None);
        }
    };

    Ok(Some(frame))
}

impl<T> Stream for FramedRead<T>
where
    T: AsyncRead + Unpin,
{
    type Item = Result<Frame, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let span = tracing::trace_span!("FramedRead::poll_next");
        let _e = span.enter();
        loop {
            tracing::trace!("poll");
            let bytes = match ready!(self.poll_frame(cx)) {
                Some(Ok(bytes)) => bytes,
                Some(Err(e)) => return Poll::Ready(Some(Err(e))),
                None => return Poll::Ready(None),
            };

            tracing::trace!(read.bytes = bytes.len());
            let Self {
                ref mut hpack,
                max_header_list_size,
                ref mut partial,
                max_continuation_frames,
                ..
            } = *self;
            if let Some(frame) = decode_frame(
                hpack,
                max_header_list_size,
                max_continuation_frames,
                partial,
                bytes,
            )? {
                tracing::debug!(?frame, "received");
                return Poll::Ready(Some(Ok(frame)));
            }
        }
    }
}

impl<T> FramedRead<T>
where
    T: AsyncRead + Unpin,
{
    /// Returns the bytes of the next frame, its header of 9 bytes and the
    /// payload whose length the first 3 bytes of the header give.
    ///
    /// It keeps the states of a length delimited reader: after an error it
    /// returns `None` once, and an end of the IO inside a frame is an error.
    fn poll_frame(&mut self, cx: &mut Context<'_>) -> Poll<Option<Result<BytesMut, Error>>> {
        loop {
            if self.errored {
                self.errored = false;
                self.readable = false;
                return Poll::Ready(None);
            }

            if self.readable {
                match self.split_frame() {
                    Ok(Some(frame)) => return Poll::Ready(Some(Ok(frame))),
                    Ok(None) if self.eof && !self.buf.is_empty() => {
                        self.errored = true;
                        let err = io::Error::new(io::ErrorKind::Other, "bytes remaining on stream");
                        return Poll::Ready(Some(Err(err.into())));
                    }
                    Ok(None) if self.eof => {
                        self.readable = false;
                        return Poll::Ready(None);
                    }
                    Ok(None) => self.readable = false,
                    Err(err) => {
                        self.errored = true;
                        return Poll::Ready(Some(Err(err)));
                    }
                }
            }

            // Room for at least one byte, so a read of zero bytes is the end.
            self.buf.reserve(1);
            let read = match self.poll_read_buf(cx) {
                Poll::Ready(Ok(read)) => read,
                Poll::Ready(Err(err)) => {
                    self.errored = true;
                    return Poll::Ready(Some(Err(err.into())));
                }
                Poll::Pending => return Poll::Pending,
            };
            if read == 0 {
                if self.eof {
                    return Poll::Ready(None);
                }
                self.eof = true;
            } else {
                self.eof = false;
            }
            self.readable = true;
        }
    }

    /// Splits the next whole frame off the buffer.
    ///
    /// The length of a frame whose header was read is kept, so a smaller
    /// maximum frame size set while its payload is read takes effect with the
    /// next frame.
    fn split_frame(&mut self) -> Result<Option<BytesMut>, Error> {
        let frame_len = match self.frame_len {
            Some(frame_len) => frame_len,
            None => {
                if self.buf.len() < 3 {
                    return Ok(None);
                }
                let payload_len = (usize::from(self.buf[0]) << 16)
                    | (usize::from(self.buf[1]) << 8)
                    | usize::from(self.buf[2]);
                if payload_len > self.max_frame_size {
                    return Err(Error::library_go_away(Reason::FRAME_SIZE_ERROR));
                }
                let frame_len = payload_len + frame::HEADER_LEN;
                self.frame_len = Some(frame_len);
                frame_len
            }
        };
        if self.buf.len() < frame_len {
            self.buf.reserve(frame_len - self.buf.len());
            return Ok(None);
        }
        self.frame_len = None;
        Ok(Some(self.buf.split_to(frame_len)))
    }

    /// Reads from the IO into the free capacity of the buffer.
    ///
    /// The free capacity is filled with zeros first, so the read writes into
    /// initialized memory.
    fn poll_read_buf(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        let filled = self.buf.len();
        let capacity = self.buf.capacity();
        self.buf.resize(capacity, 0);
        let mut read_buf = ReadBuf::new(&mut self.buf[filled..]);
        let polled = Pin::new(&mut self.inner).poll_read(cx, &mut read_buf);
        let read = read_buf.filled().len();
        self.buf.truncate(filled + read);
        ready!(polled)?;
        Poll::Ready(Ok(read))
    }
}

// ===== impl Continuable =====

impl Continuable {
    fn stream_id(&self) -> frame::StreamId {
        match *self {
            Continuable::Headers(ref h) => h.stream_id(),
            Continuable::PushPromise(ref p) => p.stream_id(),
        }
    }

    fn is_over_size(&self) -> bool {
        match *self {
            Continuable::Headers(ref h) => h.is_over_size(),
            Continuable::PushPromise(ref p) => p.is_over_size(),
        }
    }

    fn load_hpack(
        &mut self,
        src: &mut BytesMut,
        max_header_list_size: usize,
        decoder: &mut hpack::Decoder,
    ) -> Result<(), frame::Error> {
        match *self {
            Continuable::Headers(ref mut h) => h.load_hpack(src, max_header_list_size, decoder),
            Continuable::PushPromise(ref mut p) => p.load_hpack(src, max_header_list_size, decoder),
        }
    }
}

impl<T> From<Continuable> for Frame<T> {
    fn from(cont: Continuable) -> Self {
        match cont {
            Continuable::Headers(mut headers) => {
                headers.set_end_headers();
                headers.into()
            }
            Continuable::PushPromise(mut push) => {
                push.set_end_headers();
                push.into()
            }
        }
    }
}
