//! 局域内存暂存区 (Scratch Arena Allocator)
//!
//! 借鉴自 MSLS70 (Microsoft Line Services 7.0) 与 MSPTS70 的内存管理架构：
//! 排版（分行、测距、分节分栏）是高频密集的纯几何计算。
//! 若每行、每个排版单元都走全局堆分配（Heap Alloc），会引发剧烈内存碎片与性能损耗。
//! Arena 负责在单次排版过程中批量分配，排版完成后一次性 Reset/Drop，达到零堆分配开销。

use std::cell::RefCell;

pub struct Arena {
    buffer: RefCell<Vec<u8>>,
    offset: RefCell<usize>,
}

impl Arena {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: RefCell::new(Vec::with_capacity(capacity)),
            offset: RefCell::new(0),
        }
    }

    pub fn new() -> Self {
        Self::with_capacity(64 * 1024)
    }

    pub fn alloc_bytes(&self, size: usize) -> usize {
        let mut off = self.offset.borrow_mut();
        let cur = *off;
        let mut buf = self.buffer.borrow_mut();
        if cur + size > buf.len() {
            buf.resize(cur + size, 0);
        }
        *off += size;
        cur
    }

    pub fn reset(&self) {
        *self.offset.borrow_mut() = 0;
    }

    pub fn allocated_bytes(&self) -> usize {
        *self.offset.borrow()
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}
