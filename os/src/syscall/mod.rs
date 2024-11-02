//! Implementation of syscalls
//!
//! The single entry point to all system calls, [`syscall()`], is called
//! whenever userspace wishes to perform a system call using the `ecall`
//! instruction. In this case, the processor raises an 'Environment call from
//! U-mode' exception, which is handled as one of the cases in
//! [`crate::trap::trap_handler`].
//!
//! For clarity, each single syscall is implemented as its own function, named
//! `sys_` then the name of the syscall. You can find functions like this in
//! submodules, and you should also implement syscalls this way.

/// write syscall
const SYSCALL_WRITE: usize = 64;
/// exit syscall
const SYSCALL_EXIT: usize = 93;
/// yield syscall
const SYSCALL_YIELD: usize = 124;
/// gettime syscall
const SYSCALL_GET_TIME: usize = 169;
/// taskinfo syscall
const SYSCALL_TASK_INFO: usize = 410;

mod fs;
pub(crate) mod process;

use fs::*;
use process::*;

use crate::{task::TASK_MANAGER};
/// handle syscall exception with `syscall_id` and other arguments 
pub fn syscall(syscall_id: usize , args: [usize; 3]) -> isize {
    trace!("syscall: id is {} and args = {:?}",syscall_id,args);
    //let last_time = get_time();
    match syscall_id {
        SYSCALL_WRITE => sys_write(args[0], args[1] as *const u8, args[2]),
        SYSCALL_EXIT => sys_exit(args[0] as i32),
        SYSCALL_YIELD => sys_yield(),
        SYSCALL_GET_TIME => sys_get_time(args[0] as *mut TimeVal, args[1]),
        SYSCALL_TASK_INFO => sys_task_info(args[0] as *mut TaskInfo),
        _ => panic!("Unsupported syscall_id: {}", syscall_id),
    };
    // 从TASK_MANAGER 根据索引获得tcb，task info => TaskManagerInner current task
    // 运行时间 time 返回系统调用时刻距离任务第一次被调度时刻的时长，也就是说这个时长可能包含该任务被其他任务抢占后的等待重新调度的时间。

    //TODO 返回ref而不是新建的
    let current_task = TASK_MANAGER.get_current_task();
    trace!("Current task status: {:?}", current_task.task_info.status);
    TASK_MANAGER.update_task_info(syscall_id);
    return 0;
}
