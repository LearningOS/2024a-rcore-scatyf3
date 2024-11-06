//! Implementation of [`MapArea`] and [`MemorySet`].

use super::{frame_alloc, FrameTracker};
use super::{PTEFlags, PageTable, PageTableEntry};
use super::{PhysAddr, PhysPageNum, VirtAddr, VirtPageNum};
use super::{StepByOne, VPNRange};
use crate::config::{
    KERNEL_STACK_SIZE, MEMORY_END, PAGE_SIZE, TRAMPOLINE, TRAP_CONTEXT_BASE, USER_STACK_SIZE,
};
use crate::sync::UPSafeCell;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::arch::asm;
use lazy_static::*;
use riscv::register::satp;

extern "C" {
    fn stext();
    fn etext();
    fn srodata();
    fn erodata();
    fn sdata();
    fn edata();
    fn sbss_with_stack();
    fn ebss();
    fn ekernel();
    fn strampoline();
}

lazy_static! {
    /// The kernel's initial memory mapping(kernel address space)
    pub static ref KERNEL_SPACE: Arc<UPSafeCell<MemorySet>> =
        Arc::new(unsafe { UPSafeCell::new(MemorySet::new_kernel()) });
}
/// address space
/// 地址空间是一系列有关联的逻辑段，这种关联一般是指这些逻辑段属于一个运行的程序
pub struct MemorySet {
    // 该地址空间的多级页表 page_table
    // PageTable 下挂着所有多级页表的[节点所在的物理页帧]，即页表数据
    page_table: PageTable,
    // 逻辑段 MapArea 的向量 areas
    // 每个 MapArea下则挂着对应[逻辑段中的数据所在的物理页帧]，即地址里用户程序相关数据
    areas: Vec<MapArea>,
}

impl MemorySet {
    /// Create a new empty `MemorySet`.
    /// 新建一个空的地址空间
    pub fn new_bare() -> Self {
        Self {
            page_table: PageTable::new(),
            areas: Vec::new(),
        }
    }
    /// Get the page table token
    pub fn token(&self) -> usize {
        self.page_table.token()
    }
    /// Assume that no conflicts.
    /// 在当前地址空间插入一个 Framed 方式映射到 物理内存的逻辑段
    pub fn insert_framed_area(
        &mut self,
        start_va: VirtAddr,
        end_va: VirtAddr,
        permission: MapPermission,
    ) {
        // 注意该方法的调用者要保证[同一地址空间内的任意两个逻辑段不能存在交集]
        // 调用push
        self.push(
            MapArea::new(start_va, end_va, MapType::Framed, permission),
            None,
        );
    }
    /// push 方法可以在当前地址空间插入一个新的逻辑段 map_area 
    /// 如果它是以 Framed 方式映射到 物理内存，
    /// 还可以可选地在那些被映射到的物理页帧上写入一些初始化数据 data
    fn push(&mut self, mut map_area: MapArea, data: Option<&[u8]>) {
        // 将当前对象的page_table传给map_area
        // 回忆，一个MemorySet对应多个MapArea
        map_area.map(&mut self.page_table);
        // 还可以可选地在那些被映射到的物理页帧上写入一些初始化数据 data
        if let Some(data) = data {
            map_area.copy_data(&mut self.page_table, data);
        }
        // 这行代码将 map_area 添加到当前对象的 areas 列表中
        // RAII生命周期管理相关
        self.areas.push(map_area);
    }
    /// Mention that trampoline is not collected by areas.
    fn map_trampoline(&mut self) {
        self.page_table.map(
            VirtAddr::from(TRAMPOLINE).into(),
            PhysAddr::from(strampoline as usize).into(),
            PTEFlags::R | PTEFlags::X,
        );
    }
    /// Without kernel stacks.
    /// 生成内核的地址空间
    pub fn new_kernel() -> Self {
        let mut memory_set = Self::new_bare();
        // map trampoline
        // 第一步，在内核空间的最高层生成跳板
        memory_set.map_trampoline();
        // map kernel sections
        // 打印kernel各段尺寸
        info!(".text [{:#x}, {:#x})", stext as usize, etext as usize);
        info!(".rodata [{:#x}, {:#x})", srodata as usize, erodata as usize);
        info!(".data [{:#x}, {:#x})", sdata as usize, edata as usize);
        info!(
            ".bss [{:#x}, {:#x})",
            sbss_with_stack as usize, ebss as usize
        );
        // text段，stext~etext
        info!("mapping .text section");
        memory_set.push(
            MapArea::new(
                (stext as usize).into(),
                (etext as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::X,
            ),
            None,
        );
        // .rodata段，srodata~erodata
        info!("mapping .rodata section");
        memory_set.push(
            MapArea::new(
                (srodata as usize).into(),
                (erodata as usize).into(),
                MapType::Identical,
                MapPermission::R,
            ),
            None,
        );
        // data段，sdata~edata
        info!("mapping .data section");
        memory_set.push(
            MapArea::new(
                (sdata as usize).into(),
                (edata as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        // .bss段，sbss_with_stack~ebss
        info!("mapping .bss section");
        memory_set.push(
            MapArea::new(
                (sbss_with_stack as usize).into(),
                (ebss as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );

        // 各个kernel stack？
        info!("mapping physical memory");
        memory_set.push(
            MapArea::new(
                (ekernel as usize).into(),
                MEMORY_END.into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        memory_set
    }
    /// Include sections in elf and trampoline and TrapContext and user stack,
    /// also returns user_sp_base and entry point.
    /// 可以通过应用的 ELF 格式可执行文件 解析出各数据段并对应生成应用的地址空间
    pub fn from_elf(elf_data: &[u8]) -> (Self, usize, usize) {
        let mut memory_set = Self::new_bare();
        // map trampoline
        memory_set.map_trampoline();
        // map program headers of elf, with U flag
        // 使用外部 crate xmas_elf 来解析传入的应用 ELF 数据并可以轻松取出各个部分
        let elf = xmas_elf::ElfFile::new(elf_data).unwrap();
        let elf_header = elf.header;
        // 我们取出 ELF 的魔数来判断 它是不是一个合法的 ELF 
        let magic = elf_header.pt1.magic;
        assert_eq!(magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf!");
        // 直接得到 program header 的数目
        let ph_count = elf_header.pt2.ph_count();
        let mut max_end_vpn = VirtPageNum(0);
        // 遍历所有的 program header 并将合适的区域加入到应用地址空间中
        for i in 0..ph_count {
            // 按index获得当前的program hearder
            let ph = elf.program_header(i).unwrap();
            // 确定类型是load
            if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
                // 通过 ph.virtual_addr() 和 ph.mem_size() 来计算这一区域在应用地址空间中的位置
                let start_va: VirtAddr = (ph.virtual_addr() as usize).into();
                let end_va: VirtAddr = ((ph.virtual_addr() + ph.mem_size()) as usize).into();
                let mut map_perm = MapPermission::U;
                // 通过 ph.flags() 来确认这一区域访问方式的 限制并将其转换为 MapPermission 类型
                let ph_flags = ph.flags();
                if ph_flags.is_read() {
                    map_perm |= MapPermission::R;
                }
                if ph_flags.is_write() {
                    map_perm |= MapPermission::W;
                }
                if ph_flags.is_execute() {
                    map_perm |= MapPermission::X;
                }
                // 通过获得的信息，创建逻辑段
                let map_area = MapArea::new(start_va, end_va, MapType::Framed, map_perm);
                max_end_vpn = map_area.vpn_range.get_end();
                //  push 到应用地址空间
                memory_set.push(
                    map_area,
                    Some(&elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize]),
                );
            }
        }
        // map user stack with U flags
        // 处理用户栈
        let max_end_va: VirtAddr = max_end_vpn.into();
        let mut user_stack_bottom: usize = max_end_va.into();
        // guard page
        // 防止保护页面
        user_stack_bottom += PAGE_SIZE;
        let user_stack_top = user_stack_bottom + USER_STACK_SIZE;
        memory_set.push(
            MapArea::new(
                user_stack_bottom.into(),
                user_stack_top.into(),
                MapType::Framed,
                MapPermission::R | MapPermission::W | MapPermission::U,
            ),
            None,
        );
        // used in sbrk
        // 放置用户栈
        memory_set.push(
            MapArea::new(
                user_stack_top.into(),
                user_stack_top.into(),
                MapType::Framed,
                MapPermission::R | MapPermission::W | MapPermission::U,
            ),
            None,
        );
        // map TrapContext
        // 在应用地址空间中映射次高页面来存放 Trap 上下文
        memory_set.push(
            MapArea::new(
                TRAP_CONTEXT_BASE.into(),
                TRAMPOLINE.into(),
                MapType::Framed,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        // 不仅返回应用地址空间 memory_set ，也同时返回用户栈虚拟地址 user_stack_top 以及从解析 ELF 得到的该应用入口点地址，
        // 它们将被我们用来创建应用的任务控制块
        (
            memory_set,
            user_stack_top,
            elf.header.pt2.entry_point() as usize,
        )
    }
    /// Change page table by writing satp CSR Register.
    pub fn activate(&self) {
        let satp = self.page_table.token();
        unsafe {
            satp::write(satp);
            asm!("sfence.vma");
        }
    }
    /// Translate a virtual page number to a page table entry
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.page_table.translate(vpn)
    }
    /// shrink the area to new_end
    #[allow(unused)]
    pub fn shrink_to(&mut self, start: VirtAddr, new_end: VirtAddr) -> bool {
        if let Some(area) = self
            .areas
            .iter_mut()
            .find(|area| area.vpn_range.get_start() == start.floor())
        {
            area.shrink_to(&mut self.page_table, new_end.ceil());
            true
        } else {
            false
        }
    }

    /// append the area to new_end
    #[allow(unused)]
    pub fn append_to(&mut self, start: VirtAddr, new_end: VirtAddr) -> bool {
        if let Some(area) = self
            .areas
            .iter_mut()
            .find(|area| area.vpn_range.get_start() == start.floor())
        {
            area.append_to(&mut self.page_table, new_end.ceil());
            true
        } else {
            false
        }
    }
}
/// map area structure, controls a contiguous piece of virtual memory
/// 逻辑段，一段实际可用的地址连续的虚拟地址空间
pub struct MapArea {
    // 一段虚拟页号的连续区间,是iter
    vpn_range: VPNRange,
    // 如果采用frame
    // data_frames 是一个保存了该逻辑段内的每个虚拟页面 和它被映射到的物理页帧 FrameTracker 的一个键值对容器 BTreeMap 中
    // 这些物理页帧被用来存放【实际内存数据而不是作为多级页表中的中间节点】
    // TODO: 这里意思是保存物理页号吗，有点没看懂
    // 这也用到了 RAII 的思想，将这些物理页帧的生命周期绑定到它所在的逻辑段 MapArea 下，当逻辑段被回收之后这些之前分配的物理页帧也会自动地同时被回收。
    data_frames: BTreeMap<VirtPageNum, FrameTracker>,
    // MapType 描述该逻辑段内的所有虚拟页面映射到物理页帧的同一种方式
    // 它是一个枚举类型，在内核当前的实现中支持两种方式
    map_type: MapType,
    map_perm: MapPermission,
}

impl MapArea {
    /// 新建一个逻辑段结构体
    pub fn new(
        start_va: VirtAddr,
        end_va: VirtAddr,
        map_type: MapType,
        map_perm: MapPermission,
    ) -> Self {
        // 传入的起始/终止虚拟地址会分别被下取整/上取整为虚拟页号并传入 迭代器 vpn_range 中
        let start_vpn: VirtPageNum = start_va.floor();
        let end_vpn: VirtPageNum = end_va.ceil();
        Self {
            vpn_range: VPNRange::new(start_vpn, end_vpn),
            data_frames: BTreeMap::new(),
            map_type,
            map_perm,
        }
    }
    /// 对单个虚拟页面映射/解映射
    pub fn map_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        let ppn: PhysPageNum;
        // 物理页号则取决于当前逻辑段映射到物理内存的方式
        match self.map_type {
            // 以恒等映射 Identical 方式映射的时候，物理页号就等于虚拟页号
            MapType::Identical => {
                ppn = PhysPageNum(vpn.0);
            }
            // 以 Framed 方式映射的时候
            MapType::Framed => {
                // 需要分配一个物理页帧让当前的虚拟页面可以映射过去
                let frame = frame_alloc().unwrap();
                // 此时页表项中的物理页号自然就是 这个被分配的物理页帧的物理页号
                ppn = frame.ppn;
                // 还需要将这个物理页帧挂在逻辑段的 data_frames 字段下
                self.data_frames.insert(vpn, frame);
            }
        }
        // 页表项的标志位来源于当前逻辑段的类型为 MapPermission 的统一配置，只需将其转换为 PTEFlags
        let pte_flags = PTEFlags::from_bits(self.map_perm.bits).unwrap();
        // 调用多级页表 PageTable 的 map 接口来插入键值对
        page_table.map(vpn, ppn, pte_flags);
    }
    #[allow(unused)]
    pub fn unmap_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        // 当以 Framed 映射的时候，不要忘记同时将虚拟页面被映射到的物理页帧 FrameTracker 从 data_frames 中移除，
        // 这样这个物理页帧才能立即被回收以备后续分配。
        if self.map_type == MapType::Framed {
            self.data_frames.remove(&vpn);
        }
        // 调用 PageTable 的 unmap 接口删除以传入的虚拟页号为键的 键值对即可
        page_table.unmap(vpn);
    }
    /// 当前逻辑段到物理内存的映射从传入的该逻辑段所属的地址空间的 多级页表中加入
    pub fn map(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.map_one(page_table, vpn);
        }
    }
    #[allow(unused)]
    /// 当前逻辑段到物理内存的映射从传入的该逻辑段所属的地址空间的 多级页表中删除
    pub fn unmap(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.unmap_one(page_table, vpn);
        }
    }
    #[allow(unused)]
    pub fn shrink_to(&mut self, page_table: &mut PageTable, new_end: VirtPageNum) {
        for vpn in VPNRange::new(new_end, self.vpn_range.get_end()) {
            self.unmap_one(page_table, vpn)
        }
        self.vpn_range = VPNRange::new(self.vpn_range.get_start(), new_end);
    }
    #[allow(unused)]
    pub fn append_to(&mut self, page_table: &mut PageTable, new_end: VirtPageNum) {
        for vpn in VPNRange::new(self.vpn_range.get_end(), new_end) {
            self.map_one(page_table, vpn)
        }
        self.vpn_range = VPNRange::new(self.vpn_range.get_start(), new_end);
    }
    /// data: start-aligned but maybe with shorter length
    /// assume that all frames were cleared before
    /// 将切片 data 中的数据拷贝到当前逻辑段实际被内核放置在的各物理页帧上
    /// 调用它的时候需要满足：切片 data 中的数据大小不超过当前逻辑段的 总大小，
    /// 且切片中的数据会被对齐到逻辑段的开头，然后逐页拷贝到实际的物理页帧。
    pub fn copy_data(&mut self, page_table: &mut PageTable, data: &[u8]) {
        assert_eq!(self.map_type, MapType::Framed);
        // 当前的起点
        let mut start: usize = 0;
        // vpn的起点
        let mut current_vpn = self.vpn_range.get_start();
        // 数据长度
        let len = data.len();
        // 遍历每一个需要拷贝数据的虚拟页面
        loop {
            // 从data获得一页大小的数据
            let src = &data[start..len.min(start + PAGE_SIZE)];
            // dst，从vpn获得一个页的数据
            let dst = &mut page_table
                .translate(current_vpn)
                .unwrap()
                .ppn()
                .get_bytes_array()[..src.len()];
            // copy
            dst.copy_from_slice(src);
            // start递增
            start += PAGE_SIZE;
            if start >= len {
                break;
            }
            // current_vpn递增
            current_vpn.step();
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
/// map type for memory set: identical or framed
pub enum MapType {
    // 恒等映射，用于在启用多级页表之后仍能够访问一个特定的物理地址指向的物理内存
    Identical,
    // 表示对于每个虚拟页面都需要映射到一个新分配的物理页帧
    Framed,
}

bitflags! {
    /// map permission corresponding to that in pte: `R W X U`
    /// MapPermission 表示控制该逻辑段的访问方式
    /// 它是页表项标志位 PTEFlags 的一个子集，仅保留 U/R/W/X 四个标志位
    pub struct MapPermission: u8 {
        ///Readable
        const R = 1 << 1;
        ///Writable
        const W = 1 << 2;
        ///Excutable
        const X = 1 << 3;
        ///Accessible in U mode
        const U = 1 << 4;
    }
}

/// Return (bottom, top) of a kernel stack in kernel space.
pub fn kernel_stack_position(app_id: usize) -> (usize, usize) {
    let top = TRAMPOLINE - app_id * (KERNEL_STACK_SIZE + PAGE_SIZE);
    let bottom = top - KERNEL_STACK_SIZE;
    (bottom, top)
}

/// remap test in kernel space
#[allow(unused)]
pub fn remap_test() {
    let mut kernel_space = KERNEL_SPACE.exclusive_access();
    let mid_text: VirtAddr = ((stext as usize + etext as usize) / 2).into();
    let mid_rodata: VirtAddr = ((srodata as usize + erodata as usize) / 2).into();
    let mid_data: VirtAddr = ((sdata as usize + edata as usize) / 2).into();
    assert!(!kernel_space
        .page_table
        .translate(mid_text.floor())
        .unwrap()
        .writable(),);
    assert!(!kernel_space
        .page_table
        .translate(mid_rodata.floor())
        .unwrap()
        .writable(),);
    assert!(!kernel_space
        .page_table
        .translate(mid_data.floor())
        .unwrap()
        .executable(),);
    println!("remap_test passed!");
}
