//! Task pid implementation.
//!
//! Assign PID to the process here. At the same time, the position of the application KernelStack
//! is determined according to the PID.

use crate::config::{KERNEL_STACK_SIZE, PAGE_SIZE, TRAMPOLINE};
use crate::mm::{MapPermission, VirtAddr, KERNEL_SPACE};
use crate::sync::UPSafeCell;
use alloc::vec::Vec;
use lazy_static::*;

// 类似之前的物理页帧分配器 FrameAllocator ，
// 我们实现一个同样使用简单栈式分配策略的进程标识符分配器 PidAllocator (似乎和书上名字不一样)
// 并将其全局实例化为 PID_ALLOCATOR
pub struct RecycleAllocator {
    current: usize,
    recycled: Vec<usize>,
}

impl RecycleAllocator {
    pub fn new() -> Self {
        RecycleAllocator {
            current: 0,
            recycled: Vec::new(),
        }
    }
    // 分配一个pid
    pub fn alloc(&mut self) -> usize {
        if let Some(id) = self.recycled.pop() {
            id
        } else {
            self.current += 1; // stack计数器+1
            self.current - 1 // 返回当前stack上的值
        }
    }
    // 消除一个pid
    pub fn dealloc(&mut self, id: usize) {
        assert!(id < self.current);
        assert!(
            !self.recycled.iter().any(|i| *i == id),
            "id {} has been deallocated!",
            id
        );
        // 把id放到recycled，以待复用
        self.recycled.push(id);
    }
}

// 初始化的全局allocator
lazy_static! {
    static ref PID_ALLOCATOR: UPSafeCell<RecycleAllocator> =
        unsafe { UPSafeCell::new(RecycleAllocator::new()) };
    static ref KSTACK_ALLOCATOR: UPSafeCell<RecycleAllocator> =
        unsafe { UPSafeCell::new(RecycleAllocator::new()) };
}

/// Abstract structure of PID
/// 同一时间存在的所有进程都有一个自己的进程标识符，它们是互不相同的整数。
/// 这里将其抽象为一个 PidHandle 类型，当它的生命周期结束后，对应的整数会被编译器自动回收
pub struct PidHandle(pub usize);

/// 为 PidHandle 实现 Drop Trait 来允许编译器进行自动的资源回收
impl Drop for PidHandle {
    fn drop(&mut self) {
        //println!("drop pid {}", self.0);
        PID_ALLOCATOR.exclusive_access().dealloc(self.0);
    }
}

/// Allocate a new PID
/// 对外分配pid的接口
pub fn pid_alloc() -> PidHandle {
    PidHandle(PID_ALLOCATOR.exclusive_access().alloc())
}

/// Return (bottom, top) of a kernel stack in kernel space.
/// 没明白TODO
pub fn kernel_stack_position(app_id: usize) -> (usize, usize) {
    let top = TRAMPOLINE - app_id * (KERNEL_STACK_SIZE + PAGE_SIZE);
    let bottom = top - KERNEL_STACK_SIZE;
    (bottom, top)
}

/// Kernel stack for a process(task)
/// 在内核栈 KernelStack 中保存着它所属进程的 PID 
/// 从本章开始，我们将应用编号替换为进程标识符来决定每个进程内核栈在地址空间中的位置。
/// kernel stack是内核空间最靠上的东西
pub struct KernelStack(pub usize);

/// allocate a new kernel stack
pub fn kstack_alloc() -> KernelStack {
    let kstack_id = KSTACK_ALLOCATOR.exclusive_access().alloc();
    let (kstack_bottom, kstack_top) = kernel_stack_position(kstack_id);
    KERNEL_SPACE.exclusive_access().insert_framed_area(
        kstack_bottom.into(),
        kstack_top.into(),
        MapPermission::R | MapPermission::W,
    );
    KernelStack(kstack_id)
}

/// 内核栈 KernelStack 用到了 RAII 的思想，
/// 具体来说，实际保存它的物理页帧的生命周期被绑定到它下面，
/// 当 KernelStack 生命周期结束后，这些物理页帧也将会被编译器自动回收
impl Drop for KernelStack {
    fn drop(&mut self) {
        let (kernel_stack_bottom, _) = kernel_stack_position(self.0);
        let kernel_stack_bottom_va: VirtAddr = kernel_stack_bottom.into();
        KERNEL_SPACE
            .exclusive_access()
            .remove_area_with_start_vpn(kernel_stack_bottom_va.into());
        KSTACK_ALLOCATOR.exclusive_access().dealloc(self.0);
    }
}

impl KernelStack {
    // 为什么这个在rcore代码里没有了
    // 从一个 PidHandle ，也就是一个已分配的进程标识符中对应生成一个内核栈 KernelStack 
    pub fn new(pid_handle: &PidHandle) -> Self {
        let pid = pid_handle.0;
        // 调用了第 4 行声明的 kernel_stack_position 函数来根据进程标识符计算内核栈在内核地址空间中的位置
        let (kernel_stack_bottom, kernel_stack_top) = kernel_stack_position(pid);
        // 把对应pid的空间加到kernel stack
        KERNEL_SPACE.exclusive_access().insert_framed_area(
            kernel_stack_bottom.into(),
            kernel_stack_top.into(),
            MapPermission::R | MapPermission::W,
        );
        // 返回数值包装
        KernelStack(pid_handle.0)
    }
    /// Push a variable of type T into the top of the KernelStack and return its raw pointer
    /// 第 25 行的 push_on_top 方法可以将一个类型为 T 的变量压入内核栈顶并返回其裸指针， - 为啥要这样
    /// 这也是一个泛型函数。它在实现的时候用到了第 32 行的 get_top 方法来获取当前内核栈顶在内核地址空间中的地址。
    #[allow(unused)]
    pub fn push_on_top<T>(&self, value: T) -> *mut T
    where
        T: Sized,
    {
        let kernel_stack_top = self.get_top();
        let ptr_mut = (kernel_stack_top - core::mem::size_of::<T>()) as *mut T;
        unsafe {
            *ptr_mut = value;
        }
        ptr_mut
    }
    /// Get the top of the KernelStack
    pub fn get_top(&self) -> usize {
        let (_, kernel_stack_top) = kernel_stack_position(self.0);
        kernel_stack_top
    }
}
