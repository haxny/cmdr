//! XDR (RFC 4506), the encoding every ONC RPC message speaks: big-endian
//! four-byte words, and variable-length data padded to a multiple of four.
//!
//! ❗ **A hostile server can't make us allocate.** The [`Decoder`] reads one
//! received record (a `Bytes` the transport already bounded by its record cap)
//! and checks every length against what's left of it before slicing; opaque data
//! comes back as a zero-copy slice of the record. A count of array elements is
//! checked against the smallest an element can be, so a 4 GiB claim in a
//! 100-byte record fails before any capacity is reserved.

use bytes::{Buf, BufMut, Bytes, BytesMut};

/// Why a value wouldn't decode. Every variant is a broken or hostile peer: the
/// call it belongs to fails, the connection under it doesn't.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum XdrError {
    /// The record ended inside a value: `needed` bytes, `left` remaining.
    Truncated {
        /// Bytes the value needed, padding included.
        needed: usize,
        /// Bytes the record had left.
        left: usize,
    },
    /// A length or count past the field's own limit (an `opaque<64>` claiming
    /// 65 bytes).
    TooLong {
        /// What the wire said.
        len: u32,
        /// The most the field allows.
        limit: usize,
    },
    /// A bool or union discriminant that isn't one of its values.
    BadDiscriminant {
        /// The type being decoded, for the log.
        what: &'static str,
        /// What the wire said.
        value: u32,
    },
}

/// Rounds a length up to the next multiple of four, XDR's unit.
fn padded(len: usize) -> usize {
    len.div_ceil(4) * 4
}

/// Reads XDR values off one received record, front to back.
#[derive(Debug)]
pub(crate) struct Decoder {
    buf: Bytes,
}

impl Decoder {
    /// A decoder over one whole record (or the rest of one).
    pub(crate) fn new(buf: Bytes) -> Self {
        Self { buf }
    }

    /// Bytes not yet decoded.
    pub(crate) fn remaining(&self) -> usize {
        self.buf.len()
    }

    fn need(&self, needed: usize) -> Result<(), XdrError> {
        if needed > self.buf.len() {
            return Err(XdrError::Truncated {
                needed,
                left: self.buf.len(),
            });
        }
        Ok(())
    }

    /// An `unsigned int`.
    pub(crate) fn u32(&mut self) -> Result<u32, XdrError> {
        self.need(4)?;
        Ok(self.buf.get_u32())
    }

    /// An `int`.
    pub(crate) fn i32(&mut self) -> Result<i32, XdrError> {
        self.need(4)?;
        Ok(self.buf.get_i32())
    }

    /// An `unsigned hyper`.
    pub(crate) fn u64(&mut self) -> Result<u64, XdrError> {
        self.need(8)?;
        Ok(self.buf.get_u64())
    }

    /// A `bool`: 0 or 1, and anything else is a broken peer.
    pub(crate) fn bool(&mut self) -> Result<bool, XdrError> {
        match self.u32()? {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(XdrError::BadDiscriminant { what: "bool", value }),
        }
    }

    /// Fixed-length opaque data of exactly `N` bytes (a `cookieverf3`, a
    /// `writeverf3`), with no length word. `N` is a multiple of four everywhere
    /// NFS uses this, so there's no padding to skip.
    pub(crate) fn fixed_array<const N: usize>(&mut self) -> Result<[u8; N], XdrError> {
        let pad = padded(N);
        self.need(pad)?;
        let mut out = [0u8; N];
        self.buf.copy_to_slice(&mut out);
        self.buf.advance(pad - N);
        Ok(out)
    }

    /// Variable-length opaque data of at most `limit` bytes, as a slice of the
    /// record. ❗ The length is checked against `limit` and then against what's
    /// left before anything is touched.
    pub(crate) fn opaque(&mut self, limit: usize) -> Result<Bytes, XdrError> {
        let len = self.u32()?;
        let len_usize = len as usize;
        if len_usize > limit {
            return Err(XdrError::TooLong { len, limit });
        }
        let pad = padded(len_usize);
        self.need(pad)?;
        let data = self.buf.split_to(len_usize);
        self.buf.advance(pad - len_usize);
        Ok(data)
    }

    /// The element count of a variable-length array of at most `limit`
    /// elements, each at least `min_element_bytes` long. ❗ Checks that many
    /// elements could fit in what's left, so a `Vec::with_capacity(count)`
    /// after it is bounded by the record.
    pub(crate) fn array_len(&mut self, limit: usize, min_element_bytes: usize) -> Result<usize, XdrError> {
        let count = self.u32()?;
        let count_usize = count as usize;
        if count_usize > limit {
            return Err(XdrError::TooLong { len: count, limit });
        }
        self.need(count_usize.saturating_mul(min_element_bytes))?;
        Ok(count_usize)
    }

    /// An optional value (`T *` in XDR): a presence bool, then the value.
    pub(crate) fn optional<T>(
        &mut self,
        decode: impl FnOnce(&mut Self) -> Result<T, XdrError>,
    ) -> Result<Option<T>, XdrError> {
        if self.bool()? {
            Ok(Some(decode(self)?))
        } else {
            Ok(None)
        }
    }

    /// Everything left, as one slice of the record.
    pub(crate) fn rest(&mut self) -> Bytes {
        self.buf.split_off(0)
    }
}

/// Writes XDR values into one outgoing record, front to back.
pub(crate) struct Encoder {
    buf: BytesMut,
}

impl Encoder {
    /// An empty record.
    pub(crate) fn new() -> Self {
        Self { buf: BytesMut::new() }
    }

    /// An encoder that appends to `buf` (the transport leaves room for the
    /// record mark at its front).
    pub(crate) fn wrap(buf: BytesMut) -> Self {
        Self { buf }
    }

    /// An `unsigned int`.
    pub(crate) fn u32(&mut self, value: u32) {
        self.buf.put_u32(value);
    }

    /// An `int`.
    pub(crate) fn i32(&mut self, value: i32) {
        self.buf.put_i32(value);
    }

    /// An `unsigned hyper`.
    pub(crate) fn u64(&mut self, value: u64) {
        self.buf.put_u64(value);
    }

    /// A `bool`.
    pub(crate) fn bool(&mut self, value: bool) {
        self.u32(u32::from(value));
    }

    /// Fixed-length opaque data, padded, with no length word.
    pub(crate) fn fixed_opaque(&mut self, data: &[u8]) {
        self.buf.put_slice(data);
        self.buf.put_bytes(0, padded(data.len()) - data.len());
    }

    /// Variable-length opaque data (or a `string`): its length, the bytes, and
    /// the padding. The caller keeps `data` under the field's limit, which every
    /// NFS field holds far below `u32::MAX`.
    pub(crate) fn opaque(&mut self, data: &[u8]) {
        self.u32(u32::try_from(data.len()).unwrap_or(u32::MAX));
        self.fixed_opaque(data);
    }

    /// The record so far.
    pub(crate) fn finish(self) -> BytesMut {
        self.buf
    }
}

#[cfg(test)]
#[path = "xdr_test.rs"]
mod xdr_test;
