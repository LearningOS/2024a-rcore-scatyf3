//! Implementation of [`TrapContext`]
/// 启用分页机制后，trap变复杂，不仅仅是上下文，而且还要切换内存空间
/// 要求应用和内核地址空间在切换地址空间指令附近是平滑的
use riscv::register::sstatus::{self, Sstatus, SPP};

#[repr(C)]
#[derive(Debug)]
/// trap context structure containing sstatus, sepc and registers
/// 在应用地址空间的次高页面而不是内核地址空间中的内核栈中
/// 为了更方便切换上下文，而不破坏任何通用寄存器
/// 为了方便实现，我们在 Trap 上下文中包含更多内容（
/// 和我们关于上下文的定义有些不同，它们在初始化之后便只会被读取而不会被写入 ，并不是每次都需要保存/恢复）
pub struct TrapContext {
    /// General-Purpose Register x0-31
    pub x: [usize; 32],
    /// Supervisor Status Register
    pub sstatus: Sstatus,
    /// Supervisor Exception Program Counter
    pub sepc: usize,

    /// 多出来的字段，aka初始化后只被读取而不会写入
    /// Token of kernel address space
    pub kernel_satp: usize,
    /// Kernel stack pointer of the current application
    pub kernel_sp: usize,
    /// Virtual address of trap handler entry point in kernel
    /// trap handler入口地址
    pub trap_handler: usize,
}

impl TrapContext {
    /// put the sp(stack pointer) into x\[2\] field of TrapContext
    pub fn set_sp(&mut self, sp: usize) {
        self.x[2] = sp;
    }
    /// init the trap context of an application
    pub fn app_init_context(
        entry: usize,
        sp: usize,
        // 新的参数
        kernel_satp: usize,
        kernel_sp: usize,
        trap_handler: usize,
    ) -> Self {
        let mut sstatus = sstatus::read();
        // set CPU privilege to User after trapping back
        sstatus.set_spp(SPP::User);
        let mut cx = Self {
            x: [0; 32],
            sstatus,
            sepc: entry,  // entry point of app
            kernel_satp,  // addr of page table
            kernel_sp,    // kernel stack
            trap_handler, // addr of trap_handler function
        };
        cx.set_sp(sp); // app's user stack pointer
        cx // return initial Trap Context of app
    }
}
