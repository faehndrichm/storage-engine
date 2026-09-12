use std::{collections::HashMap, sync::Arc};

use crate::{
    disk::Disk,
    page::{Page, PageId},
};

#[derive(Debug)]
pub struct BufferPool {
    disk: Disk,
    cache: HashMap<PageId, Arc<Page>>,
    next_page_id: PageId,
}

impl BufferPool {
    pub fn get_page(&mut self, page_id: PageId) -> Result<Arc<Page>, std::io::Error> {
        if let Some(cached_page) = self.cache.get(&page_id) {
            return Ok(Arc::clone(cached_page));
        }

        let bytes = self.disk.read(page_id)?;
        let page = Arc::new(Page::from_bytes(bytes));
        self.cache.insert(page_id, Arc::clone(&page));
        Ok(page)
    }

    pub fn write_page(&mut self, page_id: PageId, page: Page) -> std::io::Result<()> {
        let bytes = page.to_bytes();
        self.disk.write(page_id, bytes)?;

        self.cache.insert(page_id, Arc::new(page));
        Ok(())
    }

    pub fn allocate_page(&mut self, page: Page) -> std::io::Result<PageId> {
        let page_id = self.next_page_id;
        self.next_page_id += 1;

        let bytes = page.to_bytes();
        self.disk.write(page_id, bytes)?;
        self.cache.insert(page_id, Arc::new(page));

        Ok(page_id)
    }
}
