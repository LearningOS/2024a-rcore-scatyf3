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
use crate::mm::{MapPermission, VirtAddr};
use crate::syscall::process::{self, TaskInfo};

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
        next_task.start_time = get_time_ms();
        trace!("next task start time = {}",next_task.start_time);
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
            if inner.tasks[next].start_time==0{
                trace!("next task start time = {}",get_time_ms());
                inner.tasks[next].start_time = get_time_ms();
            }
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
        trace!("current task start time is {}",current_task.start_time);
        trace!("cur time is {}",get_time_ms());
        TaskInfo {
            status: current_task.task_status, 
            syscall_times: current_task.syscall_times,
            time: get_time_ms() - current_task.start_time,
        }
    }
    /// update task info according to current task
    pub fn update_task_info(&self, syscall_id:usize){
        let mut inner = self.inner.exclusive_access();
        let current_idx = inner.current_task;
        // inner.tasks[current_idx].start_time = get_time_ms();
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
