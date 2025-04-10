//! Implementation of [`FrameAllocator`] which
//! controls all the frames in the operating system.

use super::{PhysAddr, PhysPageNum};
use crate::config::MEMORY_END;
use crate::sync::UPSafeCell;
use alloc::vec::Vec;
use core::fmt::{self, Debug, Formatter};
use lazy_static::*;

/// tracker for physical page frame allocation and deallocation
/// 就是对外接口获得的包装一层的物理页号
pub struct FrameTracker {
    /// physical page number
    pub ppn: PhysPageNum,
}

// init FrameTracker
// 我们将分配来的物理页帧的物理页号作为参数传给 FrameTracker 的 new 方法来创建一个 FrameTracker 实例
// 由于这个物理页帧之前可能被分配过并用做其他用途，我们在这里直接将这个物理页帧上的所有字节清零。
// 这一过程并不 那么显然，我们后面再详细介绍。
impl FrameTracker {
    /// Create a new FrameTracker
    pub fn new(ppn: PhysPageNum) -> Self {
        // page cleaning
        let bytes_array = ppn.get_bytes_array();
        // 对这一物理地址的全部内容擦除
        for i in bytes_array {
            *i = 0;
        }
        Self { ppn }
    }
}

impl Debug for FrameTracker {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("FrameTracker:PPN={:#x}", self.ppn.0))
    }
}

// 当一个 FrameTracker 生命周期结束被编译器回收的时候，我们需要将它控制的物理页帧回收掉 FRAME_ALLOCATOR 中
impl Drop for FrameTracker {
    fn drop(&mut self) {
        frame_dealloc(self.ppn);
    }
}

/// 物理页帧管理器
/// aka管理一块块物理页，分配的和没分配的
trait FrameAllocator {
    fn new() -> Self;
    fn alloc(&mut self) -> Option<PhysPageNum>;
    fn dealloc(&mut self, ppn: PhysPageNum);
}
/// an implementation for frame allocator
/// 我们声明一个 FrameAllocator Trait 来描述一个物理页帧管理器需要提供哪些功能
pub struct StackFrameAllocator {
    // current-end记录了从未被分配过的物理页号区间
    // 也就是 [current, end) 这个区间的物理页号都没有被分配过
    current: usize,
    end: usize,
    //以后入先出的方式保存了被回收的物理页号
    // 也就是 [0, current) 这个区间的物理页号都被分配过，这里储存的是被回收的页号
    recycled: Vec<usize>,
}

/// 最简单的栈式物理页帧管理策略 StackFrameAllocator
impl StackFrameAllocator {
    // 而在它真正被使用起来之前，需要调用 init 方法将自身的 [current,end) 初始化为可用物理页号区间
    pub fn init(&mut self, l: PhysPageNum, r: PhysPageNum) {
        self.current = l.0; // l.0 表示访问 l 这个 PhysPageNum 实例的第一个字段（即 usize 值）
        self.end = r.0;
        // trace!("last {} Physical Frames.", self.end - self.current);
    }
}
impl FrameAllocator for StackFrameAllocator {
    // 只需将区间两端均设为 0， 然后创建一个新的向量；
    fn new() -> Self {
        Self {
            current: 0,
            end: 0,
            recycled: Vec::new(),
        }
    }
    // 核心机制，分配和回收
    fn alloc(&mut self) -> Option<PhysPageNum> {
        // 首先会检查栈 recycled 内有没有之前回收的物理页号，如果有的话直接弹出栈顶并返回
        // 也就是说，优先用之前被分配过，然后后来被回收的页号
        if let Some(ppn) = self.recycled.pop() {
            Some(ppn.into())
        // 检查是否到达分配上线，如果是，分配失败
        } else if self.current == self.end {
            None
        // 从没有被分配过的内存区分配
        } else {
            self.current += 1;
            Some((self.current - 1).into())
        }
    }
    fn dealloc(&mut self, ppn: PhysPageNum) {
        let ppn = ppn.0;
        // validity check
        // 检查要求回收的页号是否valid，和是否在current这个上限之外
        if ppn >= self.current || self.recycled.iter().any(|&v| v == ppn) {
            panic!("Frame ppn={:#x} has not been allocated!", ppn);
        }
        // recycle
        // 就是存到recycled的stack上👀
        self.recycled.push(ppn);
    }
}

type FrameAllocatorImpl = StackFrameAllocator;

lazy_static! {
    /// frame allocator instance through lazy_static!
    pub static ref FRAME_ALLOCATOR: UPSafeCell<FrameAllocatorImpl> =
        unsafe { UPSafeCell::new(FrameAllocatorImpl::new()) };
}
/// initiate the frame allocator using `ekernel` and `MEMORY_END`
pub fn init_frame_allocator() {
    extern "C" {
        fn ekernel();
    }
    FRAME_ALLOCATOR.exclusive_access().init(
        PhysAddr::from(ekernel as usize).ceil(),
        PhysAddr::from(MEMORY_END).floor(),
    );
}


/// 公开给其他模块的内存分配接口，以上都是内部实现
/// 最后做一个小结：从其他模块的视角看来，物理页帧分配的接口是调用 frame_alloc 函数得到一个 FrameTracker （如果物理内存还有剩余），
/// 它就代表了一个物理页帧，当它的生命周期结束之后它所控制的物理页帧将被自动回收。


/// Allocate a physical page frame in FrameTracker style
/// 返回类型不是物理页号，而经过一层包装
pub fn frame_alloc() -> Option<FrameTracker> {
    // 这里FRAME_ALLOCATOR也是全局变量，估计在内核初始化的时候就已经初始化
    FRAME_ALLOCATOR
        .exclusive_access()// 类似加锁，一次只能有一个家伙修改
        .alloc()
        .map(FrameTracker::new)
}

/// Deallocate a physical page frame with a given ppn
pub fn frame_dealloc(ppn: PhysPageNum) {
    FRAME_ALLOCATOR.exclusive_access().dealloc(ppn);
}

#[allow(unused)]
/// a simple test for frame allocator
pub fn frame_allocator_test() {
    let mut v: Vec<FrameTracker> = Vec::new();
    for i in 0..5 {
        let frame = frame_alloc().unwrap();
        println!("{:?}", frame);
        v.push(frame);
    }
    v.clear();
    for i in 0..5 {
        let frame = frame_alloc().unwrap();
        println!("{:?}", frame);
        v.push(frame);
    }
    drop(v);
    println!("frame_allocator_test passed!");
}
