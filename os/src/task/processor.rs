//!Implementation of [`Processor`] and Intersection of control flow
//!
//! Here, the continuous operation of user apps in CPU is maintained,
//! the current running state of CPU is recorded,
//! and the replacement and transfer of control flow of different applications are executed.

use super::__switch;
use super::{fetch_task, TaskStatus};
use super::{TaskContext, TaskControlBlock};
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use alloc::sync::Arc;
use lazy_static::*;

/// Processor management structure
/// 处理器管理结构 Processor 负责维护从任务管理器 TaskManager 分离出去的那部分 CPU 状态
pub struct Processor {
    ///The task currently executing on the current processor
    /// 处理器上正在执行的任务
    current: Option<Arc<TaskControlBlock>>,

    ///The basic control flow of each core, helping to select and switch process
    /// 表示当前处理器上的 idle 控制流的任务上下文的地址 ?
    /// context: 每个 Processor 都有一个 idle 控制流，
    /// 它们运行在每个核各自的启动栈上，功能是尝试从任务管理器中选出一个任务来在当前核上执行。
    idle_task_cx: TaskContext,
}

impl Processor {
    ///Create an empty Processor
    pub fn new() -> Self {
        Self {
            current: None,
            idle_task_cx: TaskContext::zero_init(),
        }
    }

    ///Get mutable reference to `idle_task_cx`
    fn get_idle_task_cx_ptr(&mut self) -> *mut TaskContext {
        &mut self.idle_task_cx as *mut _
    }

    ///Get current task in moving semanteme
    /// 取出正在执行的任务，意味着current 字段也变为 None(为啥)
    pub fn take_current(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.current.take()
    }

    ///Get current task in cloning semanteme
    /// 返回当前执行的任务的一份拷贝
    pub fn current(&self) -> Option<Arc<TaskControlBlock>> {
        self.current.as_ref().map(Arc::clone)
    }
}

lazy_static! {
    pub static ref PROCESSOR: UPSafeCell<Processor> = unsafe { UPSafeCell::new(Processor::new()) };
}

///The main part of process execution and scheduling
///Loop `fetch_task` to get the process that needs to run, and switch the process through `__switch`
/// 在内核初始化完毕之后，核通过调用 run_tasks 函数来进入 idle 控制流
pub fn run_tasks() {
    loop {
        let mut processor = PROCESSOR.exclusive_access();
        // 它循环调用 fetch_task 直到顺利从任务管理器中取出一个任务
        if let Some(task) = fetch_task() {
            let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
            // access coming task TCB exclusively
            let mut task_inner = task.inner_exclusive_access();
            let next_task_cx_ptr = &task_inner.task_cx as *const TaskContext;
            task_inner.task_status = TaskStatus::Running;
            // release coming task_inner manually
            drop(task_inner);
            // release coming task TCB manually
            processor.current = Some(task);
            // release processor manually
            drop(processor);
            // 然后获得 __switch 两个参数进行任务切换
            unsafe {
                __switch(idle_task_cx_ptr, next_task_cx_ptr);
            }
        } else {
            warn!("no tasks available in run_tasks");
        }
    }
}

/// Get current task through take, leaving a None in its place
pub fn take_current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().take_current()
}

/// Get a copy of the current task
pub fn current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().current()
}

/// Get the current user token(addr of page table)
///  基于 current_task 实现，提供当前正在执行的任务的更多信息
pub fn current_user_token() -> usize {
    let task = current_task().unwrap();
    task.get_user_token()
}

///Get the mutable reference to trap context of current task
///  基于 current_task 实现，提供当前正在执行的任务的更多信息
pub fn current_trap_cx() -> &'static mut TrapContext {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .get_trap_cx()
}

///Return to idle control flow for new scheduling
/// 当一个应用交出 CPU 使用权时，进入内核后它会调用 schedule 函数来切换到 idle 控制流并开启新一轮的任务调度。
/// 回忆一些ostep内容，这是非常古老的os调度方法，它假设每个进程都是善意的，开发者要手动交还控制权限...
pub fn schedule(switched_task_cx_ptr: *mut TaskContext) {
    let mut processor = PROCESSOR.exclusive_access();
    let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
    drop(processor);
    // 从当前切换到 idle 控制流
    unsafe {
        __switch(switched_task_cx_ptr, idle_task_cx_ptr);
    }
}
