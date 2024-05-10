//! Implementation of [`PageTableEntry`] and [`PageTable`].

use super::{frame_alloc, FrameTracker, PhysPageNum, StepByOne, VirtAddr, VirtPageNum};
use alloc::vec;
use alloc::vec::Vec;
use bitflags::*;

bitflags! {
    /// page table entry flags
    pub struct PTEFlags: u8 {
        const V = 1 << 0;
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
        const G = 1 << 5;
        const A = 1 << 6;
        const D = 1 << 7;
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
/// page table entry structure
pub struct PageTableEntry {
    /// bits of page table entry
    pub bits: usize,
}

impl PageTableEntry {
    /// Create a new page table entry
    pub fn new(ppn: PhysPageNum, flags: PTEFlags) -> Self {
        PageTableEntry {
            bits: ppn.0 << 10 | flags.bits as usize,
        }
    }
    /// Create an empty page table entry
    pub fn empty() -> Self {
        PageTableEntry { bits: 0 }
    }
    /// Get the physical page number from the page table entry
    /// 获得页表项的物理页号
    pub fn ppn(&self) -> PhysPageNum {
        //self.bits >> 10 去除 10位flags
        // & ((1usize << 44) - 1) 保留低44位
        (self.bits >> 10 & ((1usize << 44) - 1)).into()
    }
    /// Get the flags from the page table entry
    pub fn flags(&self) -> PTEFlags {
        PTEFlags::from_bits(self.bits as u8).unwrap()
    }
    /// The page pointered by page table entry is valid?
    pub fn is_valid(&self) -> bool {
        (self.flags() & PTEFlags::V) != PTEFlags::empty()
    }
    /// The page pointered by page table entry is readable?
    pub fn readable(&self) -> bool {
        (self.flags() & PTEFlags::R) != PTEFlags::empty()
    }
    /// The page pointered by page table entry is writable?
    pub fn writable(&self) -> bool {
        (self.flags() & PTEFlags::W) != PTEFlags::empty()
    }
    /// The page pointered by page table entry is executable?
    pub fn executable(&self) -> bool {
        (self.flags() & PTEFlags::X) != PTEFlags::empty()
    }
}

/// page table structure
pub struct PageTable {
    root_ppn: PhysPageNum, // 根页表的物理页号
    frames: Vec<FrameTracker>, // 保存了页表所有的节点（包括根节点）所在的物理页帧
}

/// Assume that it won't oom when creating/mapping.
impl PageTable {
    /// Create a new page table
    pub fn new() -> Self {
        let frame = frame_alloc().unwrap(); // 分配一个物理页帧
        PageTable {
            root_ppn: frame.ppn, // 根页表的物理页号
            frames: vec![frame], 
        }
    }
    /// Temporarily used to get arguments from user space.
    pub fn from_token(satp: usize) -> Self {
        Self {
            root_ppn: PhysPageNum::from(satp & ((1usize << 44) - 1)), // 获取satp的低44位即为根页表的物理页号
            frames: Vec::new(),
        }
    }
    /// Find PageTableEntry by VirtPageNum, create a frame for a 4KB page table if not exist
    /// 通过虚拟页号vpn查找页表项，如果不存在则创建一个4KB的页表
    fn find_pte_create(&mut self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
        let idxs = vpn.indexes(); // 获得页表项的索引
        let mut ppn = self.root_ppn; // 根页表的物理页号, 是物理页号，页号
        let mut result: Option<&mut PageTableEntry> = None; // 页表项
        for (i, idx) in idxs.iter().enumerate() {
            // 获得页表项
            let pte = &mut ppn.get_pte_array()[*idx];
            // 如果是第三级页表项，直接返回
            if i == 2 {
                result = Some(pte);
                break;
            }
            // 如果页表项无效，则分配一个物理页帧
            if !pte.is_valid() {
                let frame = frame_alloc().unwrap();
                *pte = PageTableEntry::new(frame.ppn, PTEFlags::V);
                self.frames.push(frame);
            }
            // 获得下一级页表的物理页号
            ppn = pte.ppn();
        }
        result
    }
    
    /// Find PageTableEntry by VirtPageNum
    /// 通过虚拟页号vpn查找页表项
    fn find_pte(&self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
        let idxs = vpn.indexes();
        let mut ppn = self.root_ppn;
        let mut result: Option<&mut PageTableEntry> = None;
        for (i, idx) in idxs.iter().enumerate() {
            // 获得页表项
            let pte = &mut ppn.get_pte_array()[*idx]; // 每次都得转化成物理地址去查找页表项
            if i == 2 {
                result = Some(pte);
                break;
            }
            if !pte.is_valid() {
                return None;
            }
            ppn = pte.ppn();
        }
        result
    }
    
    // 动态维护一个虚拟页号到页表项的映射，支持插入/删除键值对
    
    /// 通过虚拟页号vpn映射到物理页号ppn
    /// set the map between virtual page number and physical page number
    #[allow(unused)]
    pub fn map(&mut self, vpn: VirtPageNum, ppn: PhysPageNum, flags: PTEFlags) {
        let pte = self.find_pte_create(vpn).unwrap();
        assert!(!pte.is_valid(), "vpn {:?} is mapped before mapping", vpn);
        *pte = PageTableEntry::new(ppn, flags | PTEFlags::V);
    }
    
    /// 通过虚拟页号vpn删除映射
    /// remove the map between virtual page number and physical page number
    #[allow(unused)]
    pub fn unmap(&mut self, vpn: VirtPageNum) {
        let pte = self.find_pte(vpn).unwrap();
        assert!(pte.is_valid(), "vpn {:?} is invalid before unmapping", vpn);
        *pte = PageTableEntry::empty();
    }
    /// get the page table entry from the virtual page number
    /// 如果能够找到页表项，那么它会将页表项拷贝一份并返回，否则就返回一个 None 。
    /// 这个方法的主要作用是为了在内核中查找页表项，然后将页表项拷贝到内核中，以便内核能够访问到页表项。
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.find_pte(vpn).map(|pte| *pte)
    }
    /// get the token from the page table
    pub fn token(&self) -> usize {
        8usize << 60 | self.root_ppn.0
    }
}

/// Translate&Copy a ptr[u8] array with LENGTH len to a mutable u8 Vec through page table
pub fn translated_byte_buffer(token: usize, ptr: *const u8, len: usize) -> Vec<&'static mut [u8]> {
    let page_table = PageTable::from_token(token); //通过当前stap创建PageTable
    let mut start = ptr as usize; // 起始地址
    let end = start + len; // 结束地址
    let mut v = Vec::new();
    while start < end {
        let start_va = VirtAddr::from(start); // 起始虚拟地址
        let mut vpn = start_va.floor(); // 虚拟页号
        // page_table.translate(vpn) 通过虚拟页号vpn查找页表项 然后返回页表项中的物理页号
        let ppn = page_table.translate(vpn).unwrap().ppn();
        vpn.step(); // 下一个虚拟页号
        let mut end_va: VirtAddr = vpn.into(); // 当前结束虚拟地址
        end_va = end_va.min(VirtAddr::from(end));
        if end_va.page_offset() == 0 {
            v.push(&mut ppn.get_bytes_array()[start_va.page_offset()..]);
        } else {
            v.push(&mut ppn.get_bytes_array()[start_va.page_offset()..end_va.page_offset()]);
        }
        start = end_va.into();
    }
    v
}

/// Get the physical address from the page table
pub fn get_phyical_address(token: usize, ptr: usize) -> usize {
    let page_table = PageTable::from_token(token);


    let va = VirtAddr::from(ptr);
    let offest = va.page_offset();

    let vpn = va.floor();

    let ppn = match page_table.translate(vpn) {
        Some(pte) => pte.ppn(),
        None => panic!("get_phyical_address: can't find pte"),
    };

    let pa = ppn.0 << 12 | offest;
    pa
}