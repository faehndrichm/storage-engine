use std::fmt::Display;

use crate::disk::PAGE_SIZE;

// header
pub const HEADER_SIZE: usize = 20;
const SLOT_SIZE: usize = 4;

pub type PageId = u32;

// types of values
const COUNT_SIZE: usize = 4;
const PAGE_ID_SIZE: usize = 4;
const KEY_SIZE: usize = 4;

const PAYLOAD_SIZE: usize = PAGE_SIZE - HEADER_SIZE;

// shared for leaf + internal
const KEY_COUNT_OFFSET: usize = 0;

// Leaf (n keys):
//
// [ key_count  ] 4 bytes
// [ right_leaf ] 4 bytes
// [   key 0    ] 4 bytes
// ..............
// [  key n-1   ] 4 bytes

const LEAF_RIGHT_LEAF_OFFSET: usize = KEY_COUNT_OFFSET + COUNT_SIZE; // 4
const LEAF_KEYS_OFFSET: usize = LEAF_RIGHT_LEAF_OFFSET + PAGE_ID_SIZE; // 8
const LEAF_MAX_KEYS: usize = (PAYLOAD_SIZE - LEAF_KEYS_OFFSET) / KEY_SIZE;

// Internal (n keys, n + 1 children):
//
// [ key_count  ] 4 bytes
// [  child 0   ] 4 bytes
// [   key 0    ] 4 bytes
// [  child 1   ] 4 bytes
// ..............
// [  key n-1   ] 4 bytes
// [  child n   ] 4 bytes

const INTERNAL_ENTRIES_OFFSET: usize = KEY_COUNT_OFFSET + COUNT_SIZE; // 4
const INTERNAL_ENTRY_SIZE: usize = PAGE_ID_SIZE + KEY_SIZE; // 8
const INTERNAL_MAX_KEYS: usize =
    (PAYLOAD_SIZE - INTERNAL_ENTRIES_OFFSET - PAGE_ID_SIZE) / INTERNAL_ENTRY_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageHeader {
    lsn: u64,
    page_id: PageId,
    slot_count: u16,
    free_start: u16,
    free_end: u16,
    pub kind: PageKind,
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
    pub fn new(kind: PageKind) -> Self {
        let mut buf = Box::new([0u8; PAGE_SIZE]);
        let header = PageHeader {
            lsn: 0,
            page_id: 0, // stamped by the buffer pool on allocate/write
            slot_count: 0,
            free_start: HEADER_SIZE as u16,
            free_end: PAGE_SIZE as u16,
            kind,
            is_deleted: false,
        };
        header.encode(&mut buf);
        Page { buf }
    }

    pub fn payload(&self) -> &[u8] {
        &self.buf[HEADER_SIZE..]
    }

    pub fn payload_mut(&mut self) -> &mut [u8] {
        &mut self.buf[HEADER_SIZE..]
    }
}

struct Slot {
    offset: u16,
    length: u16,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    Data = 0,
    BtreeLeaf = 1,
    BtreeInternal = 2,
}

impl Display for PageKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PageKind::Data => write!(f, "Data"),
            PageKind::BtreeLeaf => write!(f, "BtreeLeaf"),
            PageKind::BtreeInternal => write!(f, "BtreeInternal"),
        }
    }
}

#[derive(Debug)]
pub enum DecodeError {
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
    pub fn encode<'a>(&self, buf: &'a mut [u8; PAGE_SIZE]) {
        let mut packer = BytePacker::new(buf);

        packer.pack(&self.lsn.to_le_bytes());
        packer.pack(&self.page_id.to_le_bytes());
        packer.pack(&self.slot_count.to_le_bytes());
        packer.pack(&self.free_start.to_le_bytes());
        packer.pack(&self.free_end.to_le_bytes());
        packer.pack(&[self.kind as u8]);
        packer.pack(&[self.is_deleted as u8]);
    }

    pub fn decode(buf: &[u8; PAGE_SIZE]) -> Self {
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
impl AsRef<Page> for Page {
    fn as_ref(&self) -> &Page {
        self
    }
}
impl AsMut<Page> for Page {
    fn as_mut(&mut self) -> &mut Page {
        self
    }
}
/// Sentinel for non existing page
pub const NO_PAGE: PageId = PageId::MAX;

fn read_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
}
fn write_u32(buf: &mut [u8], off: usize, val: u32) {
    buf[off..off + 4].copy_from_slice(&val.to_le_bytes());
}

pub struct LeafPage<P> {
    page: P,
}

pub struct InternalPage<P> {
    page: P,
}

impl<P> LeafPage<P> {
    fn key_offset(i: usize) -> usize {
        LEAF_KEYS_OFFSET + i * KEY_SIZE
    }
}
impl<P: AsRef<Page>> LeafPage<P> {
    pub fn new(page: P) -> Self {
        Self { page }
    }
    pub fn key_count(&self) -> usize {
        read_u32(self.page.as_ref().payload(), KEY_COUNT_OFFSET) as usize
    }
    pub fn right_leaf(&self) -> Option<PageId> {
        let value = read_u32(self.page.as_ref().payload(), LEAF_RIGHT_LEAF_OFFSET);
        if value == NO_PAGE { None } else { Some(value) }
    }
    pub fn key(&self, i: usize) -> u32 {
        read_u32(self.page.as_ref().payload(), Self::key_offset(i))
    }
}
impl<P: AsRef<Page> + AsMut<Page>> LeafPage<P> {
    pub fn set_key_count(&mut self, key_count: usize) {
        assert!(
            key_count <= LEAF_MAX_KEYS,
            "leaf overflow: {key_count} keys"
        );
        write_u32(
            self.page.as_mut().payload_mut(),
            KEY_COUNT_OFFSET,
            key_count as u32,
        );
    }
    pub fn set_right_leaf(&mut self, right_leaf: Option<PageId>) {
        let value = right_leaf.unwrap_or(NO_PAGE);
        write_u32(
            self.page.as_mut().payload_mut(),
            LEAF_RIGHT_LEAF_OFFSET,
            value,
        );
    }
    pub fn set_key(&mut self, i: usize, key: u32) {
        write_u32(self.page.as_mut().payload_mut(), Self::key_offset(i), key);
    }
}
impl<P> InternalPage<P> {
    fn child_offset(i: usize) -> usize {
        INTERNAL_ENTRIES_OFFSET + i * INTERNAL_ENTRY_SIZE
    }
    fn key_offset(i: usize) -> usize {
        Self::child_offset(i) + PAGE_ID_SIZE
    }
}
impl<P: AsRef<Page>> InternalPage<P> {
    pub fn new(page: P) -> Self {
        Self { page }
    }
    pub fn key_count(&self) -> usize {
        read_u32(self.page.as_ref().payload(), KEY_COUNT_OFFSET) as usize
    }
    pub fn key(&self, i: usize) -> u32 {
        read_u32(self.page.as_ref().payload(), Self::key_offset(i))
    }
    pub fn child_node(&self, i: usize) -> PageId {
        read_u32(self.page.as_ref().payload(), Self::child_offset(i))
    }
}
impl<P: AsRef<Page> + AsMut<Page>> InternalPage<P> {
    pub fn set_key_count(&mut self, key_count: usize) {
        assert!(
            key_count <= INTERNAL_MAX_KEYS,
            "internal node overflow: {key_count} keys"
        );
        write_u32(
            self.page.as_mut().payload_mut(),
            KEY_COUNT_OFFSET,
            key_count as u32,
        );
    }
    pub fn set_key(&mut self, i: usize, key: u32) {
        write_u32(self.page.as_mut().payload_mut(), Self::key_offset(i), key);
    }
    pub fn set_child_node(&mut self, i: usize, child_id: PageId) {
        write_u32(
            self.page.as_mut().payload_mut(),
            Self::child_offset(i),
            child_id,
        );
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
