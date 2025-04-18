//!Implementation of [`TaskManager`]
use super::TaskControlBlock;
use crate::sync::UPSafeCell;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;
///A array of `TaskControlBlock` that is thread-safe
pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

/// A simple FIFO scheduler.
/// 在前面的章节中，任务管理器 TaskManager 不仅负责管理所有的任务，还维护着 CPU 当前在执行哪个任务。 
/// 由于这种设计不够灵活，我们需要将任务管理器对于 CPU 的监控职能拆分到处理器管理结构 Processor 中去， 
/// 任务管理器自身仅负责管理所有任务。在这里，任务指的就是进程。
impl TaskManager {
    ///Creat an empty TaskManager
    pub fn new() -> Self {
        Self {
            // 将所有的任务控制块用引用计数 Arc 智能指针包裹后放在一个双端队列 VecDeque 中
            // 用指针避免deque移动的开销
            ready_queue: VecDeque::new(),
        }
    }
    // 从调度算法来看，这里用到的就是最简单的 RR 算法
    /// Add process back to ready queue
    /// 将一个任务加入队尾
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task);
    }
    /// Take a process out of the ready queue
    /// 从队头中取出一个任务来执行
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.ready_queue.pop_front()
    }
}

lazy_static! {
    /// TASK_MANAGER instance through lazy_static!
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

/// 给其他文件的接口
/// Add process to ready queue
pub fn add_task(task: Arc<TaskControlBlock>) {
    //trace!("kernel: TaskManager::add_task");
    TASK_MANAGER.exclusive_access().add(task);
}

/// Take a process out of the ready queue
pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    //trace!("kernel: TaskManager::fetch_task");
    TASK_MANAGER.exclusive_access().fetch()
}
