//! A canonical binary codec: a magic prefix, big-endian fixed-width integers, and
//! u32-length-prefixed byte strings. Every value has exactly one encoding.

use crate::error::{CoreError, Result};
use crate::hash::Hash;

pub struct Writer {
    buf: Vec<u8>,
}

pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl Writer {
    pub fn new(magic: &[u8]) -> Self {
        Self {
            buf: magic.to_vec(),
        }
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn i64(&mut self, v: i64) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    /// Writes a collection length or byte length as a u32.
    ///
    /// # Panics
    /// If `n` exceeds `u32::MAX`; callers bound sizes long before that.
    pub fn count(&mut self, n: usize) {
        self.u32(u32::try_from(n).expect("length exceeds u32::MAX"));
    }

    pub fn bytes(&mut self, v: &[u8]) {
        self.count(v.len());
        self.buf.extend_from_slice(v);
    }

    pub fn str(&mut self, v: &str) {
        self.bytes(v.as_bytes());
    }

    pub fn hash(&mut self, v: &Hash) {
        self.buf.extend_from_slice(v.as_bytes());
    }

    pub fn opt_str(&mut self, v: Option<&str>) {
        match v {
            Some(s) => {
                self.u8(1);
                self.str(s);
            }
            None => self.u8(0),
        }
    }

    pub fn opt_hash(&mut self, v: Option<&Hash>) {
        match v {
            Some(h) => {
                self.u8(1);
                self.hash(h);
            }
            None => self.u8(0),
        }
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8], magic: &[u8]) -> Result<Self> {
        if !buf.starts_with(magic) {
            return Err(CoreError::Decode("bad magic"));
        }
        Ok(Self {
            buf,
            pos: magic.len(),
        })
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(CoreError::Decode("length overflow"))?;
        let slice = self
            .buf
            .get(self.pos..end)
            .ok_or(CoreError::Decode("unexpected end of input"))?;
        self.pos = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.take(N)?
            .try_into()
            .map_err(|_| CoreError::Decode("unexpected end of input"))
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    pub fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.array()?))
    }

    pub fn count(&mut self) -> Result<usize> {
        Ok(self.u32()? as usize)
    }

    pub fn bytes(&mut self) -> Result<&'a [u8]> {
        let n = self.count()?;
        self.take(n)
    }

    pub fn string(&mut self) -> Result<String> {
        String::from_utf8(self.bytes()?.to_vec()).map_err(|_| CoreError::Decode("invalid utf-8"))
    }

    pub fn hash(&mut self) -> Result<Hash> {
        Ok(Hash::from_bytes(self.array()?))
    }

    pub fn opt_string(&mut self) -> Result<Option<String>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.string()?)),
            _ => Err(CoreError::Decode("bad option tag")),
        }
    }

    pub fn opt_hash(&mut self) -> Result<Option<Hash>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.hash()?)),
            _ => Err(CoreError::Decode("bad option tag")),
        }
    }

    /// Fails unless every byte was consumed, so each value has one encoding.
    pub fn finish(self) -> Result<()> {
        if self.pos == self.buf.len() {
            Ok(())
        } else {
            Err(CoreError::Decode("trailing bytes"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAGIC: &[u8] = b"TEST\x01";

    #[test]
    fn round_trips_every_value_type() {
        let h = Hash::of(b"x");
        let mut w = Writer::new(MAGIC);
        w.u8(7);
        w.u32(70_000);
        w.u64(u64::MAX);
        w.i64(-5);
        w.count(3);
        w.bytes(b"raw");
        w.str("文字");
        w.hash(&h);
        w.opt_str(Some("s"));
        w.opt_str(None);
        w.opt_hash(Some(&h));
        w.opt_hash(None);
        let bytes = w.finish();

        let mut r = Reader::new(&bytes, MAGIC).unwrap();
        assert_eq!(r.u8().unwrap(), 7);
        assert_eq!(r.u32().unwrap(), 70_000);
        assert_eq!(r.u64().unwrap(), u64::MAX);
        assert_eq!(r.i64().unwrap(), -5);
        assert_eq!(r.count().unwrap(), 3);
        assert_eq!(r.bytes().unwrap(), b"raw");
        assert_eq!(r.string().unwrap(), "文字");
        assert_eq!(r.hash().unwrap(), h);
        assert_eq!(r.opt_string().unwrap().as_deref(), Some("s"));
        assert_eq!(r.opt_string().unwrap(), None);
        assert_eq!(r.opt_hash().unwrap(), Some(h));
        assert_eq!(r.opt_hash().unwrap(), None);
        r.finish().unwrap();
    }

    #[test]
    fn rejects_bad_magic_truncation_and_trailing_bytes() {
        assert_eq!(
            Reader::new(b"NOPE", MAGIC).err(),
            Some(CoreError::Decode("bad magic"))
        );

        let mut w = Writer::new(MAGIC);
        w.str("hello");
        let bytes = w.finish();
        let truncated = &bytes[..bytes.len() - 1];
        let mut r = Reader::new(truncated, MAGIC).unwrap();
        assert_eq!(
            r.string(),
            Err(CoreError::Decode("unexpected end of input"))
        );

        let mut extra = bytes.clone();
        extra.push(0);
        let mut r = Reader::new(&extra, MAGIC).unwrap();
        r.string().unwrap();
        assert_eq!(r.finish(), Err(CoreError::Decode("trailing bytes")));
    }

    #[test]
    fn rejects_invalid_option_tag_and_utf8() {
        let mut w = Writer::new(MAGIC);
        w.u8(9);
        let bytes = w.finish();
        assert_eq!(
            Reader::new(&bytes, MAGIC).unwrap().opt_string(),
            Err(CoreError::Decode("bad option tag"))
        );

        let mut w = Writer::new(MAGIC);
        w.bytes(&[0xff]);
        let bytes = w.finish();
        assert_eq!(
            Reader::new(&bytes, MAGIC).unwrap().string(),
            Err(CoreError::Decode("invalid utf-8"))
        );
    }
}
