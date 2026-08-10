use pretty_hex::PrettyHex;
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::FileExt,
    time::Instant,
};

struct Disk {
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

    pub fn read(&self, page_id: usize, buf: &mut [u8; PAGE_SIZE]) -> std::io::Result<()> {
        self.file.read_exact_at(buf, (page_id * PAGE_SIZE) as u64)
    }
    pub fn write(&self, page_id: usize, buf: &[u8; PAGE_SIZE]) -> std::io::Result<()> {
        self.file.write_all_at(buf, (page_id * PAGE_SIZE) as u64)
    }

    fn sync(&self) -> std::io::Result<()> {
        self.file.sync_data()
    }
}
