//! Implementation of [`PageTableEntry`] and [`PageTable`].

use super::{frame_alloc, FrameTracker, PhysPageNum, StepByOne, VirtAddr, VirtPageNum};
use alloc::vec;
use alloc::vec::Vec;
use bitflags::*;

// 页表控制位
// 根据SV39的定义，页表项的高 43 位 aka [53:10]是物理页号，低 8 位，aka[7:0]是页表项的控制位。
bitflags! { //  Rust 中常用来比特标志位的 crate
    /// page table entry flags
    pub struct PTEFlags: u8 {
        const V = 1 << 0; // valid
        const R = 1 << 1; // R/W/X = 读/写/取指
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4; // 在用户态是否允许访问
        const G = 1 << 5; 
        const A = 1 << 6; // 记录自从页表项上的这一位被清零之后，页表项的对应虚拟页面是否被访问过
        const D = 1 << 7; // 记录自从页表项上的这一位被清零之后，页表项的对应虚拟页表是否被修改过
    }
}

// 让编译器自动为 PageTableEntry 实现 Copy/Clone Trait，
// 来让这个类型以值语义赋值/传参的时候 不会发生所有权转移，而是拷贝一份新的副本
#[derive(Copy, Clone)] 
#[repr(C)]
/// page table entry structure
pub struct PageTableEntry {
    /// bits of page table entry
    pub bits: usize,
}

impl PageTableEntry {
    /// Create a new page table entry
    /// 从一个物理页号 PhysPageNum 和一个页表项标志位 PTEFlags 生成一个页表项 PageTableEntry 实例
    /// 把两个数值连缀在一起罢了
    pub fn new(ppn: PhysPageNum, flags: PTEFlags) -> Self {
        PageTableEntry {
            bits: ppn.0 << 10 | flags.bits as usize,
        }
    }
    /// Create an empty page table entry
    /// 生成一个全0，aka不合法的PageTableEntry
    pub fn empty() -> Self {
        PageTableEntry { bits: 0 }
    }
    /// Get the physical page number from the page table entry
    /// 获得页表号，aka放弃后面10个bit
    pub fn ppn(&self) -> PhysPageNum {
        (self.bits >> 10 & ((1usize << 44) - 1)).into()
    }
    /// Get the flags from the page table entry
    /// 获取后面的flags
    pub fn flags(&self) -> PTEFlags {
        PTEFlags::from_bits(self.bits as u8).unwrap()
    }
    /// The page pointered by page table entry is valid?
    /// 通过v位判断是否合法
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
    root_ppn: PhysPageNum,
    frames: Vec<FrameTracker>,
}

/// Assume that it won't oom when creating/mapping.
/// 这里是一个多级页表实现
/// 每个应用的地址空间都对应一个不同的多级页表，这也就意味这不同页表的起始地址（即页表根节点的地址）是不一样的。
/// 或者说每个应用都有一个独一的页表，唉唉
impl PageTable {
    /// Create a new page table
    pub fn new() -> Self {
        let frame = frame_alloc().unwrap();
        PageTable {
            root_ppn: frame.ppn, //只需要保存根节点，作为页表唯一的区分标志（本进程页表和其他页表）
            // 向量 frames 以 FrameTracker 的形式保存了页表所有的节点（包括根节点）所在的物理页帧
            frames: vec![frame], //生命周期绑定
        }
    }
    /// Temporarily used to get arguments from user space.
    /// 临时创建一个专用来手动查页表的 PageTable
    /// satp是mmu里的要素，是多级页表根节点的物理页号
    /// 它的 frames 字段为空，也即不实际控制任何资源
    pub fn from_token(satp: usize) -> Self {
        Self {
            // 仅有一个从传入的 satp token 中得到的多级页表根节点的物理页号
            root_ppn: PhysPageNum::from(satp & ((1usize << 44) - 1)),
            // frame 为空，意味着它不控制任何资源
            frames: Vec::new(),
        }
    }
    /// Find PageTableEntry by VirtPageNum, create a frame for a 4KB page table if not exist
    /// 在多级页表找到一个虚拟页号对应的页表项的可变引用方便后续的读写。
    /// 回想，页表向是虚拟地址到物理地址的映射， 还有一堆杂七杂八信息
    /// 如果在 遍历的过程中发现有节点尚未创建则会新建一个节点。
    fn find_pte_create(&mut self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
        // 虚拟页号 -> 三级节点
        let idxs = vpn.indexes();
        // 当前节点的物理页号,最开始指向多级页表的根节点
        let mut ppn = self.root_ppn;
        let mut result: Option<&mut PageTableEntry> = None;
        // 在多级页表里逐级深入
        for (i, idx) in idxs.iter().enumerate() {
            // 每次循环通过 get_pte_array 将 取出当前节点的页表项数组，并根据当前级页索引找到对应的页表项
            // 当前的page table entry
            let pte = &mut ppn.get_pte_array()[*idx];
            // 循环到结尾，break，返回
            if i == 2 {
                result = Some(pte);
                break;
            }
            // 走不下去的话，aka没有找到valid的页表项
            if !pte.is_valid() {
                // 就新建一个节点，更新作为下级节点指针的页表项
                let frame = frame_alloc().unwrap();
                // 更新作为下级节点指针的页表项
                *pte = PageTableEntry::new(frame.ppn, PTEFlags::V);
                // 并将新分配的物理页帧移动到 向量 frames 中方便后续的自动回收。
                self.frames.push(frame);
            }
            // 更新ppn
            ppn = pte.ppn();
        }
        result
    }
    /// Find PageTableEntry by VirtPageNum
    /// 不经过mmu而是手动查页表
    /// TODO:需要看和上面那个函数的区别？只是没找到的话不创建，和mmu的关系是？
    fn find_pte(&self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
        let idxs = vpn.indexes();
        let mut ppn = self.root_ppn;
        let mut result: Option<&mut PageTableEntry> = None;
        for (i, idx) in idxs.iter().enumerate() {
            let pte = &mut ppn.get_pte_array()[*idx];
            if i == 2 {
                result = Some(pte);
                break;
            }
            //和之前的 find_pte_create 不同之处在于不会试图分配物理页帧
            if !pte.is_valid() {
                return None;
            }
            ppn = pte.ppn();
        }
        result
    }
    /// 为了执行页表搜索，操作系统维护虚拟页号到页表项的映射
    /// set the map between virtual page number and physical page number
    #[allow(unused)]
    pub fn map(&mut self, vpn: VirtPageNum, ppn: PhysPageNum, flags: PTEFlags) {
        // 根据虚拟页号找到页表项，或者说创造？
        let pte = self.find_pte_create(vpn).unwrap();
        assert!(!pte.is_valid(), "vpn {:?} is mapped before mapping", vpn);
        // 根据参数修改页表项
        *pte = PageTableEntry::new(ppn, flags | PTEFlags::V);
    }
    /// remove the map between virtual page number and physical page number
    /// 删除键值对
    #[allow(unused)]
    pub fn unmap(&mut self, vpn: VirtPageNum) {
        // 根据虚拟页号找到页表项
        let pte = self.find_pte(vpn).unwrap();
        assert!(pte.is_valid(), "vpn {:?} is invalid before unmapping", vpn);
        // 清空页表项
        *pte = PageTableEntry::empty();
    }
    /// get the page table entry from the virtual page number
    /// 调用 find_pte 来实现，如果能够找到页表项，那么它会将页表项拷贝一份并返回，否则就 返回一个 None
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.find_pte(vpn).map(|pte| *pte)
    }
    /// get the token from the page table
    /// PageTable::token 会按照 satp CSR 格式要求 构造一个无符号 64 位无符号整数，
    /// 使得其 分页模式为 SV39 ，且将当前多级页表的根节点所在的物理页号填充进去。
    /// 在 activate 中，我们将这个值写入当前 CPU 的 satp CSR ，
    /// 从这一刻开始 SV39 分页模式就被启用了，而且 MMU 会使用内核地址空间的多级页表进行地址转换。
    pub fn token(&self) -> usize {
        8usize << 60 | self.root_ppn.0
    }
}

/// Translate&Copy a ptr[u8] array with LENGTH len to a mutable u8 Vec through page table
/// 同样由于内核和应用地址空间的隔离， sys_write 不再能够直接访问位于应用空间中的数据，
/// 而需要手动查页表才能知道那些 数据被放置在哪些物理页帧上并进行访问。
/// 为此，页表模块 page_table 提供了将应用地址空间中一个缓冲区转化为在内核空间中能够直接访问的形式的辅助函数
/// 参数中的 token 是某个应用地址空间的 token，aka一个用于配置 satp（Supervisor Address Translation and Protection）控制寄存器的值,
/// 包含了分页模式和当前页表根节点的物理地址等信息
/// ptr 和 len 则分别表示该地址空间中的一段缓冲区的起始地址 和长度
pub fn translated_byte_buffer(token: usize, ptr: *const u8, len: usize) -> Vec<&'static mut [u8]> {
    let page_table = PageTable::from_token(token);
    let mut start = ptr as usize;
    let end = start + len;
    let mut v = Vec::new();
    while start < end {
        let start_va = VirtAddr::from(start);
        let mut vpn = start_va.floor();
        let ppn = page_table.translate(vpn).unwrap().ppn();
        vpn.step();
        let mut end_va: VirtAddr = vpn.into();
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
