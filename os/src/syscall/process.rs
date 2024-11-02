//! Process management syscalls
use crate::{
    config::MAX_SYSCALL_NUM,
    task::{exit_current_and_run_next, suspend_current_and_run_next, TaskStatus},
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// Task information
#[derive(Copy, Clone)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    pub status: TaskStatus,
    /// The numbers of syscall called by task
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    pub time: usize,
}

/// Default init
impl Default for TaskInfo {
    fn default() -> Self {
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

/// get time with second and microsecond
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    unsafe {
        *ts = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,
        };
    }
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    let task_info :TaskInfo = unsafe {*_ti};
    trace!("[sys_task_info] current task's status is {:?}",task_info.status);
    trace!("[sys_task_info] current task's time is {:?}",task_info.time);
    trace!("[sys_task_info] current task's syscall_times is {:?}",task_info.syscall_times);
    // TODO:error check
    return 0;
}

pub fn update_task_info(syscall_id: usize , _ti: *mut TaskInfo){
    let ti_ref: &mut TaskInfo = unsafe { &mut *_ti };
    ti_ref.syscall_times[syscall_id]+=1;
}