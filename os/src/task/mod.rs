//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the operating system.
//!
//! Be careful when you see `__switch` ASM function in `switch.S`. Control flow around this function
//! might not be what you expect.

mod context;
mod switch;
#[allow(clippy::module_inception)]
mod task;
use crate::mm::{MapPermission, VirtAddr};
use crate::syscall::process::{self, TaskInfo};

use crate::loader::{get_app_data, get_num_app};
use crate::sync::UPSafeCell;
use crate::timer::get_time_ms;
use crate::trap::TrapContext;
use alloc::vec::Vec;
use lazy_static::*;
use switch::__switch;
pub use task::{TaskControlBlock, TaskStatus};

pub use context::TaskContext;

/// The task manager, where all the tasks are managed.
///
/// Functions implemented on `TaskManager` deals with all task state transitions
/// and task context switching. For convenience, you can find wrappers around it
/// in the module level.
///
/// Most of `TaskManager` are hidden behind the field `inner`, to defer
/// borrowing checks to runtime. You can see examples on how to use `inner` in
/// existing functions on `TaskManager`.
pub struct TaskManager {
    /// total number of tasks
    num_app: usize,
    /// use inner value to get mutable access
    inner: UPSafeCell<TaskManagerInner>,
}

/// The task manager inner in 'UPSafeCell'
struct TaskManagerInner {
    /// task list
    tasks: Vec<TaskControlBlock>,
    /// id of current `Running` task
    current_task: usize,
}

lazy_static! {
    /// a `TaskManager` global instance through lazy_static!
    /// 将所有的应用加载到全局应用管理器中
    pub static ref TASK_MANAGER: TaskManager = {
        println!("init TASK_MANAGER");
        let num_app = get_num_app();
        println!("num_app = {}", num_app);
        let mut tasks: Vec<TaskControlBlock> = Vec::new();
        for i in 0..num_app {
            tasks.push(TaskControlBlock::new(get_app_data(i), i));
        }
        TaskManager {
            num_app,
            inner: unsafe {
                UPSafeCell::new(TaskManagerInner {
                    tasks,
                    current_task: 0,
                })
            },
        }
    };
}

impl TaskManager {
    /// Run the first task in task list.
    ///
    /// Generally, the first task in task list is an idle task (we call it zero process later).
    /// But in ch4, we load apps statically, so the first task is a real app.
    fn run_first_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let next_task = &mut inner.tasks[0];
        next_task.task_status = TaskStatus::Running;
        let next_task_cx_ptr = &next_task.task_cx as *const TaskContext;
        drop(inner);
        let mut _unused = TaskContext::zero_init();
        // before this, we should drop local variables that must be dropped manually
        unsafe {
            __switch(&mut _unused as *mut _, next_task_cx_ptr);
        }
        panic!("unreachable in run_first_task!");
    }

    /// Change the status of current `Running` task into `Ready`.
    fn mark_current_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].task_status = TaskStatus::Ready;
    }

    /// Change the status of current `Running` task into `Exited`.
    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].task_status = TaskStatus::Exited;
    }

    /// Find next task to run and return task id.
    ///
    /// In this case, we only return the first `Ready` task in task list.
    fn find_next_task(&self) -> Option<usize> {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        (current + 1..current + self.num_app + 1)
            .map(|id| id % self.num_app)
            .find(|id| inner.tasks[*id].task_status == TaskStatus::Ready)
    }

    /// Get the current 'Running' task's token.
    fn get_current_token(&self) -> usize {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_user_token()
    }

    /// Get the current 'Running' task's trap contexts.
    fn get_current_trap_cx(&self) -> &'static mut TrapContext {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_trap_cx()
    }

    /// Change the current 'Running' task's program break
    pub fn change_current_program_brk(&self, size: i32) -> Option<usize> {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].change_program_brk(size)
    }

    /// Switch current `Running` task to the task we have found,
    /// or there is no `Ready` task and we can exit with all applications completed
    fn run_next_task(&self) {
        if let Some(next) = self.find_next_task() {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            inner.tasks[next].task_status = TaskStatus::Running;
            inner.current_task = next;
            let current_task_cx_ptr = &mut inner.tasks[current].task_cx as *mut TaskContext;
            let next_task_cx_ptr = &inner.tasks[next].task_cx as *const TaskContext;
            drop(inner);
            // before this, we should drop local variables that must be dropped manually
            unsafe {
                __switch(current_task_cx_ptr, next_task_cx_ptr);
            }
            // go back to user mode
        } else {
            panic!("All applications completed!");
        }
    }
    
    /// get inner control block
    pub fn get_task_info(&self) -> TaskInfo {
        let inner = self.inner.exclusive_access();
        let current_task = &inner.tasks[inner.current_task];
        TaskInfo {
            status: current_task.task_status, 
            syscall_times: current_task.syscall_times,
            time: current_task.start_time - get_time_ms(),
        }
    }
    /// update task info according to current task
    pub fn update_task_info(&self, syscall_id:usize){
        let mut inner = self.inner.exclusive_access();
        let current_idx = inner.current_task;
        inner.tasks[current_idx].syscall_times[syscall_id]+=1;
        trace!("current syscall_times_id = {}",inner.tasks[current_idx].syscall_times[syscall_id]);
        info!("update taskinfo on current task = {} , syscall_id = {}, syscall times = {}",inner.current_task,syscall_id,inner.tasks[current_idx].syscall_times[syscall_id]);
    }
    /// 对当前的任务，增加一段虚拟内存空间
    pub fn append_memory_to_cur_task_memspace(&self,_start: VirtAddr,_end: VirtAddr, _port: usize) -> isize {
        // 使用 UPSafeCell 获取可变引用
        let mut inner = self.inner.exclusive_access();
        let index = inner.current_task;
        trace!("_port={}", _port);
        let mut perm = MapPermission::empty();
        perm.set(MapPermission::R, _port & 0x1 != 0);
        perm.set(MapPermission::W, _port & 0x2 != 0);
        perm.set(MapPermission::X, _port & 0x4 != 0);
        perm.set(MapPermission::U, true);
        trace!("append memory {:?}:{:?} to cur task space",_start,_end);
        // check这段内存是否已经被映射
        if inner.tasks[index].memory_set.check_map_area_overlap(_start, _end) {
            return -1;
        }
        inner.tasks[index].memory_set.insert_framed_area(_start, _end,perm);
        return 0;
    }
    /// 从当前的内存空间里删除一段完整的内存区域
    pub fn unmap_memory_to_cur_task_space(&self,_start: VirtAddr,_end: VirtAddr) -> isize {
        // 使用 UPSafeCell 获取可变引用
        let mut inner = self.inner.exclusive_access();
        let index = inner.current_task;
        trace!("unmap memory {:?}:{:?} to cur task space",_start,_end);
        // 必须全部overlap，才能 unmap
        if inner.tasks[index].memory_set.check_map_area_equal(_start, _end){
            inner.tasks[index].memory_set.remove_map_area(_start);
            return 0;
        }else{
            return -1;
        }
    }
}

/// Run the first task in task list.
pub fn run_first_task() {
    TASK_MANAGER.run_first_task();
}

/// Switch current `Running` task to the task we have found,
/// or there is no `Ready` task and we can exit with all applications completed
fn run_next_task() {
    TASK_MANAGER.run_next_task();
}

/// Change the status of current `Running` task into `Ready`.
fn mark_current_suspended() {
    TASK_MANAGER.mark_current_suspended();
}

/// Change the status of current `Running` task into `Exited`.
fn mark_current_exited() {
    TASK_MANAGER.mark_current_exited();
}

/// Suspend the current 'Running' task and run the next task in task list.
pub fn suspend_current_and_run_next() {
    mark_current_suspended();
    run_next_task();
}

/// Exit the current 'Running' task and run the next task in task list.
pub fn exit_current_and_run_next() {
    mark_current_exited();
    run_next_task();
}

/// Get the current 'Running' task's token.
pub fn current_user_token() -> usize {
    TASK_MANAGER.get_current_token()
}

/// Get the current 'Running' task's trap contexts.
pub fn current_trap_cx() -> &'static mut TrapContext {
    TASK_MANAGER.get_current_trap_cx()
}

/// Change the current 'Running' task's program break
pub fn change_program_brk(size: i32) -> Option<usize> {
    TASK_MANAGER.change_current_program_brk(size)
}

/// 创建并映射一段虚拟内存
pub fn append_memory_to_cur_task_memspace(_start: VirtAddr, _end: VirtAddr, _port: usize) -> isize {
    TASK_MANAGER.append_memory_to_cur_task_memspace(_start,_end,_port)
}

/// 取消一段虚拟内存
pub fn unmap_memory_to_cur_task_space(_start: VirtAddr, _end: VirtAddr) -> isize {
    TASK_MANAGER.unmap_memory_to_cur_task_space(_start,_end)
}

/// 获取人物信息
pub fn get_current_task() -> process::TaskInfo {
    return TASK_MANAGER.get_task_info();
}

/// 更新人物信息
pub fn update_task_info(syscall_id: usize){
    return TASK_MANAGER.update_task_info(syscall_id);
}