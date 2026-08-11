use crate::disk::PAGE_SIZE;

pub const HEADER_SIZE: usize = 20;
const SLOT_SIZE: usize = 4;

struct PageHeader {
    lsn: u64,
    page_id: u32,
    slot_count: u16,
    free_start: u16,
    free_end: u16,
    kind: PageKind,
    is_deleted: bool,
}

struct Page {
    buf: Box<[u8; PAGE_SIZE]>,
}

struct Slot {
    offset: u16,
    length: u16,
}

#[repr(u8)]
#[derive(Clone, Copy)]
enum PageKind {
    Data = 0,
    BtreeLeaf = 1,
    BtreeInternal = 2,
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
}

impl PageHeader {
    fn encode(&self, buf: &mut [u8]) {
        let mut packer = BytePacker::new(buf);

        packer.pack(&self.lsn.to_le_bytes());
        packer.pack(&self.page_id.to_le_bytes());
        packer.pack(&self.slot_count.to_le_bytes());
        packer.pack(&self.free_start.to_le_bytes());
        packer.pack(&self.free_end.to_le_bytes());
        packer.pack(&[self.kind as u8]);
        packer.pack(&[self.is_deleted as u8]);
    }
}
