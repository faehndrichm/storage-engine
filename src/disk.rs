use std::{
    fs::{File, OpenOptions},
    os::unix::fs::FileExt,
};

use crate::page::{Page, PageId};

#[derive(Debug)]
pub struct Disk {
    file: File,
}

pub const PAGE_SIZE: usize = 4096;

impl Disk {
    pub fn create(path: &str) -> std::io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;
        Ok(Disk { file })
    }

    pub fn read(&self, page_id: PageId) -> std::io::Result<[u8; PAGE_SIZE]> {
        let mut buf = [0u8; PAGE_SIZE];
        self.file
            .read_exact_at(&mut buf, (page_id as usize * PAGE_SIZE) as u64);
        Ok(buf)
    }
    pub fn write(&self, page_id: PageId, buf: &[u8; PAGE_SIZE]) -> std::io::Result<()> {
        self.file
            .write_all_at(buf, (page_id as usize * PAGE_SIZE) as u64)
    }

    fn sync(&self) -> std::io::Result<()> {
        self.file.sync_data()
    }
}
