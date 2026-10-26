use crate::{
    disk::Disk,
    page::{Page, PageHeader, PageId, PageKind},
};
use core::fmt;
use std::{
    collections::HashMap,
    io::{self},
    sync::{Arc, PoisonError, RwLock, atomic::AtomicU32},
};

#[derive(Debug)]
pub enum PageError {
    Io(io::Error),
    PageNotFound(PageId),
    LockPoisoned,
    UnexpectedPageKind(PageKind),
}

pub trait PageStore {
    fn get_page(&self, id: PageId) -> Result<Arc<Page>, PageError>;
    fn write_page(&self, id: PageId, page: Page) -> Result<(), PageError>;
    fn free_page(&self, id: PageId) -> Result<(), PageError>;
    fn allocate_page(&self, page: Page) -> Result<PageId, PageError>;
}

impl fmt::Display for PageError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "invalid first item to double")
    }
}

impl From<io::Error> for PageError {
    fn from(err: io::Error) -> Self {
        PageError::Io(err)
    }
}

impl<T> From<PoisonError<T>> for PageError {
    fn from(_: PoisonError<T>) -> Self {
        PageError::LockPoisoned
    }
}
#[derive(Debug)]
pub struct BufferPool {
    disk: Disk,
    cache: RwLock<HashMap<PageId, Arc<Page>>>,
    next_page_id: AtomicU32,
}

impl BufferPool {
    pub fn new(disk: Disk) -> Self {
        Self {
            disk,
            cache: RwLock::new(HashMap::new()),
            next_page_id: AtomicU32::new(0),
        }
    }
}

impl PageStore for BufferPool {
    fn get_page(&self, page_id: PageId) -> Result<Arc<Page>, PageError> {
        if let Some(cached_page) = self.cache.read().unwrap().get(&page_id) {
            return Ok(Arc::clone(cached_page));
        }

        let bytes = self.disk.read(page_id)?;

        let header = PageHeader::decode(&bytes);
        if header.is_deleted {
            return Err(PageError::PageNotFound(page_id));
        }

        let page = Arc::new(Page::from_bytes(bytes));

        self.cache
            .write()
            .unwrap()
            .insert(page_id, Arc::clone(&page));
        Ok(page)
    }

    fn write_page(&self, page_id: PageId, page: Page) -> Result<(), PageError> {
        let bytes = page.to_bytes();
        self.disk.write(page_id, bytes)?;

        self.cache.write().unwrap().insert(page_id, Arc::new(page));
        Ok(())
    }

    fn free_page(&self, page_id: PageId) -> Result<(), PageError> {
        // mark the page dead on disk first, then forget it
        self.disk.write_at_offset(
            page_id,
            PageHeader::IS_DELETED_OFFSET,
            &PageHeader::IS_DELETED_MARK,
        )?;

        self.cache.write().unwrap().remove(&page_id);
        Ok(())
    }

    fn allocate_page(&self, page: Page) -> Result<PageId, PageError> {
        let page_id = self
            .next_page_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let bytes = page.to_bytes();
        self.disk.write(page_id, bytes)?;

        self.cache.write().unwrap().insert(page_id, Arc::new(page));

        Ok(page_id)
    }
}

#[derive(Debug)]
pub struct InMemoryPageStore {
    cache: RwLock<HashMap<PageId, Arc<Page>>>,
    next_page_id: AtomicU32,
}
impl InMemoryPageStore {
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            next_page_id: AtomicU32::new(0),
        }
    }
}

impl Default for InMemoryPageStore {
    fn default() -> Self {
        Self::new()
    }
}
impl PageStore for InMemoryPageStore {
    fn get_page(&self, page_id: PageId) -> Result<Arc<Page>, PageError> {
        match self.cache.read()?.get(&page_id) {
            Some(page) => Ok(Arc::clone(page)),
            None => Err(PageError::PageNotFound(page_id)),
        }
    }

    fn write_page(&self, page_id: PageId, page: Page) -> Result<(), PageError> {
        self.cache.write()?.insert(page_id, Arc::new(page));
        Ok(())
    }

    fn free_page(&self, page_id: PageId) -> Result<(), PageError> {
        self.cache.write()?.remove(&page_id);
        Ok(())
    }

    fn allocate_page(&self, page: Page) -> Result<PageId, PageError> {
        let page_id = self
            .next_page_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        self.cache.write()?.insert(page_id, Arc::new(page));

        Ok(page_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freed_page_is_not_found_after_cache_is_gone() {
        let path = std::env::temp_dir().join(format!(
            "storage-engine-{}-freed-page.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let pool = BufferPool::new(Disk::create(path.to_str().unwrap()).unwrap());
        let page_id = pool.allocate_page(Page::new(PageKind::BtreeLeaf)).unwrap();
        assert!(pool.get_page(page_id).is_ok());

        pool.free_page(page_id).unwrap();

        assert!(matches!(
            pool.get_page(page_id),
            Err(PageError::PageNotFound(id)) if id == page_id
        ));

        let _ = std::fs::remove_file(&path);
    }
}
