use crate::disk::PAGE_SIZE;

pub const HEADER_SIZE: usize = 20;
const SLOT_SIZE: usize = 4;

pub type PageId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PageHeader {
    lsn: u64,
    page_id: PageId,
    slot_count: u16,
    free_start: u16,
    free_end: u16,
    kind: PageKind,
    is_deleted: bool,
}

#[derive(Debug)]
pub struct Page {
    buf: Box<[u8; PAGE_SIZE]>,
}
impl Page {
    pub(crate) fn from_bytes(bytes: [u8; PAGE_SIZE]) -> Self {
        Page {
            buf: Box::new(bytes),
        }
    }

    pub(crate) fn to_bytes(&self) -> &[u8; PAGE_SIZE] {
        &self.buf
    }
}

struct Slot {
    offset: u16,
    length: u16,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageKind {
    Data = 0,
    BtreeLeaf = 1,
    BtreeInternal = 2,
}

#[derive(Debug)]
enum DecodeError {
    InvalidPageKind(u8),
}

impl TryFrom<u8> for PageKind {
    type Error = DecodeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(PageKind::Data),
            1 => Ok(PageKind::BtreeLeaf),
            2 => Ok(PageKind::BtreeInternal),
            other => Err(DecodeError::InvalidPageKind(other)),
        }
    }
}

struct BytePacker<'a> {
    offset: usize,
    buf: &'a mut [u8],
}

impl<'a> BytePacker<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        BytePacker { offset: 0, buf }
    }

    fn pack(&mut self, val: &[u8]) {
        let new_offset = self.offset + val.len();
        self.buf[self.offset..new_offset].copy_from_slice(val);
        self.offset = new_offset;
    }

    fn as_slice(self) -> &'a [u8] {
        self.buf
    }
}

struct ByteUnpacker<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> ByteUnpacker<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn unpack(&mut self, size: usize) -> &'a [u8] {
        let bytes = &self.buf[self.pos..self.pos + size];
        self.pos += size;
        bytes
    }

    fn unpack_u64(&mut self) -> u64 {
        u64::from_le_bytes(self.unpack(8).try_into().unwrap())
    }

    fn unpack_u32(&mut self) -> u32 {
        u32::from_le_bytes(self.unpack(4).try_into().unwrap())
    }

    fn unpack_u16(&mut self) -> u16 {
        u16::from_le_bytes(self.unpack(2).try_into().unwrap())
    }

    fn unpack_u8(&mut self) -> u8 {
        self.unpack(1)[0]
    }
}

impl PageHeader {
    fn encode<'a>(&self, buf: &'a mut [u8; PAGE_SIZE]) {
        let mut packer = BytePacker::new(buf);

        packer.pack(&self.lsn.to_le_bytes());
        packer.pack(&self.page_id.to_le_bytes());
        packer.pack(&self.slot_count.to_le_bytes());
        packer.pack(&self.free_start.to_le_bytes());
        packer.pack(&self.free_end.to_le_bytes());
        packer.pack(&[self.kind as u8]);
        packer.pack(&[self.is_deleted as u8]);
    }

    fn decode(buf: &[u8; PAGE_SIZE]) -> Self {
        let mut unpacker = ByteUnpacker::new(buf);
        PageHeader {
            lsn: unpacker.unpack_u64(),
            page_id: unpacker.unpack_u32(),
            slot_count: unpacker.unpack_u16(),
            free_start: unpacker.unpack_u16(),
            free_end: unpacker.unpack_u16(),
            kind: PageKind::try_from(unpacker.unpack_u8()).unwrap(),
            is_deleted: unpacker.unpack_u8() != 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_decoded_is_equal() {
        let header = PageHeader {
            lsn: 0x0502070403060708,
            page_id: 0x11121314,
            slot_count: 0x2122,
            free_start: 0x3132,
            free_end: 0x4142,
            kind: PageKind::BtreeLeaf,
            is_deleted: true,
        };

        let mut buf = [0u8; PAGE_SIZE];
        header.encode(&mut buf);

        let decoded_header = PageHeader::decode(&buf);

        assert_eq!(header, decoded_header);
    }
}
