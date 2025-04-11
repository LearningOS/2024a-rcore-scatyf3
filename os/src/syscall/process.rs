//! Process management syscalls

use crate::mm::VirtAddr;
use crate::mm::page_table::translated_ptr;
use crate::task::{append_memory_to_cur_task_memspace, current_user_token, unmap_memory_to_cur_task_space};
use crate::{
    config::MAX_SYSCALL_NUM,
    task::{change_program_brk, exit_current_and_run_next, suspend_current_and_run_next, TaskStatus},
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// Task information
/// 这只是get task info的一个中间结果储存结构体
/// 它不该被存到TCB里，在任何意味上
#[derive(Copy, Clone)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    pub status: TaskStatus,
    /// The numbers of syscall called by task
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    pub time: usize,
}

impl TaskInfo {
    pub fn new() -> Self {
        TaskInfo {
            status: TaskStatus::UnInit, // 或者其他默认状态
            syscall_times: [0; MAX_SYSCALL_NUM], // 初始化为全0数组
            time: 0,
        }
    }
}

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    trace!("result is {}",us);
    // 这个syscall的任务是把获得的us写入ts，但传入的ts是虚拟地址而不是物理地址，加入翻译功能...
    let ts = translated_ptr(current_user_token(),ts as usize) as *mut TimeVal;
    unsafe {
        *ts = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,
        };
    }
    0
}


/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ? TODO
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info");
    

    return 0;
}

// YOUR JOB: Implement mmap.
// 申请长度为 len 字节的物理内存（不要求实际物理内存位置，可以随便找一块），将其映射到 start 开始的虚存，内存页属性为 port
// 等等，usize和虚拟地址的关系是啥？
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!("kernel: sys_mmap");
    let virt_addr_start: VirtAddr  = VirtAddr::from(_start);
    let virt_addr_end: VirtAddr  = VirtAddr::from(_start+_len);
    // check
    // check start 没有按页大小对齐
    if virt_addr_start.page_offset() != 0 {
        return -1;
    }
    // check port 第 0 位表示是否可读，第 1 位表示是否可写，第 2 位表示是否可执行。其他位无效且必须为 0
    // 检查port其他位是否有非0
    if _port & !0x7 != 0{
        return -1;
    }
    // 如果读写执行都不可以，这样一段内存没有意义
    if _port & 0x7 == 0 {
        return -1;
    }
    // [start, start + len) 中存在已经被映射的页，丢给os内核部分检查
    append_memory_to_cur_task_memspace(virt_addr_start, virt_addr_end, _port)
}

// YOUR JOB: Implement munmap.
// 取消到 [start, start + len) 虚存的映射。特别地，在 rCore 课程实验中，正确执行的 sys_munmap 仅会对应 唯一且完整 的 mmap 区间，不考虑交叉、截断区间的情况。
// 参数和返回值请参考 mmap
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys_munmap");
    let virt_addr_start: VirtAddr  = VirtAddr::from(_start);
    let virt_addr_end: VirtAddr  = VirtAddr::from(_start+_len);
    if virt_addr_start.page_offset() != 0 {
        return -1;
    }
    unmap_memory_to_cur_task_space(virt_addr_start, virt_addr_end)

}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

