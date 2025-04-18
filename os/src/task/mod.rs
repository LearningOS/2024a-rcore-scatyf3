//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the whole operating system.
//!
//! A single global instance of [`Processor`] called `PROCESSOR` monitors running
//! task(s) for each core.
//!
//! A single global instance of `PID_ALLOCATOR` allocates pid for user apps.
//!
//! Be careful when you see `__switch` ASM function in `switch.S`. Control flow around this function
//! might not be what you expect.
mod context;
mod id;
mod manager;
mod processor;
mod switch;
#[allow(clippy::module_inception)]
mod task;

use crate::loader::get_app_data_by_name;
use alloc::sync::Arc;
use lazy_static::*;
pub use manager::{fetch_task, TaskManager};
use switch::__switch;
pub use task::{TaskControlBlock, TaskStatus};

pub use context::TaskContext;
pub use id::{kstack_alloc, pid_alloc, KernelStack, PidHandle};
pub use manager::add_task;
pub use processor::{
    current_task, current_trap_cx, current_user_token, run_tasks, schedule, take_current_task,
    Processor,
};
/// Suspend the current 'Running' task and run the next task in task list.
/// 暂停当前任务，并切换到下一个任务
pub fn suspend_current_and_run_next() {
    // There must be an application running.
    // 获取当前的任务信息
    let task = take_current_task().unwrap();

    // ---- access current TCB exclusively
    // 加锁访问里面的东西，获得当前任务context
    let mut task_inner = task.inner_exclusive_access();
    let task_cx_ptr = &mut task_inner.task_cx as *mut TaskContext;
    // Change status to Ready
    // 修改状态，running -> ready
    task_inner.task_status = TaskStatus::Ready;
    // 结束加锁访问
    drop(task_inner);
    // ---- release current PCB

    // push back to ready queue.
    // 重新加入到就绪队列
    add_task(task);
    // jump to scheduling cycle
    // 丢到调度器，让调度器进行下一个任务
    schedule(task_cx_ptr);
}

/// pid of usertests app in make run TEST=1
pub const IDLE_PID: usize = 0;

/// Exit the current 'Running' task and run the next task in task list.
/// 
pub fn exit_current_and_run_next(exit_code: i32) {
    // take from Processor
    // 将当前进程控制块从处理器监控 PROCESSOR 中取出，而不只是得到一份拷贝，这是为了正确维护进程控制块的引用计数；
    let task = take_current_task().unwrap();

    let pid = task.getpid();
    if pid == IDLE_PID {
        println!(
            "[kernel] Idle process exit with exit_code {} ...",
            exit_code
        );
        panic!("All applications completed!");
    }

    // **** access current TCB exclusively
    let mut inner = task.inner_exclusive_access();
    // Change status to Zombie
    // 将进程控制块中的状态修改为 TaskStatus::Zombie 即僵尸进程
    inner.task_status = TaskStatus::Zombie;
    // Record exit code
    inner.exit_code = exit_code;
    // do not move to its parent but under initproc

    // ++++++ access initproc TCB exclusively
    // 将当前进程的所有子进程挂在初始进程 initproc 下面。
    {
        let mut initproc_inner = INITPROC.inner_exclusive_access();
        for child in inner.children.iter() {
            child.inner_exclusive_access().parent = Some(Arc::downgrade(&INITPROC));
            initproc_inner.children.push(child.clone());
        }
    }
    // ++++++ release parent PCB
    // 第 32 行将当前进程的孩子向量清空。
    inner.children.clear();
    // deallocate user space
    // 对于当前进程占用的资源进行早期回收。 
    // MemorySet::recycle_data_pages 只是将地址空间中的逻辑段列表 areas 清空，
    // 这将导致应用地址空间的所有数据被存放在的物理页帧被回收，而用来存放页表的那些物理页帧此时则不会被回收。
    inner.memory_set.recycle_data_pages();
    drop(inner);
    // **** release current PCB
    // drop task manually to maintain rc correctly
    drop(task);
    // we do not have to save task context
    let mut _unused = TaskContext::zero_init();
    // 调用 schedule 触发调度及任务切换
    schedule(&mut _unused as *mut _);
}

// 内核初始化完毕之后，即会调用 task 子模块提供的 add_initproc 函数来将初始进程 initproc 加入任务管理器，
// 但在这之前，我们需要初始进程的进程控制块 INITPROC ，这基于 lazy_static 在运行时完成。
lazy_static! {
    /// Creation of initial process
    ///
    /// the name "initproc" may be changed to any other app name like "usertests",
    /// but we have user_shell, so we don't need to change it.
    pub static ref INITPROC: Arc<TaskControlBlock> = Arc::new(TaskControlBlock::new(
        // 通过加载器 loader 子模块提供的 get_app_data_by_name 接口查找 initproc 的 ELF 数据来获得
        get_app_data_by_name("ch5b_initproc").unwrap()
    ));
}

///Add init process to the manager
/// 增加初始进程到任务管理器，但为什么？
pub fn add_initproc() {
    add_task(INITPROC.clone());
}


