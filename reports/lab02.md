## 实验任务

在这个实验里，起始源代码为我们实现了:
- 虚拟内存和物理内存的地址和转换 `os/src/mm/address.rs`
- 全部的SV39的多级页表机制 `os/src/mm/page_table.rs`
- 大部分虚拟内存空间&分段的机制 `os/src/mm/memory_set.rs`

我们需要完成
- 重写 `sys_get_time`和`sys_task_info`
- 完成`sys_mmap`和`sys_munmap`

## 批注

在`89898eef59add4c9b73ccb16fb4dc059ce114764`这个commit里，我根据rcore实验书批注了代码，初步发现sys_mmap需要调用`os/src/task/mod.rs`下提供的接口，以修改`TaskControlBlock`下的`memory_set`成员

## 内存映射

沿着这个思路，我在`ff3fc82c7af2b0831be933eaaafca9d97d4b3658`完成了`sys_mmap`和`sys_munmap`的初稿，大致思路是
- 在`os/src/syscall/process.rs`中，完成要求的检查，包括地址有效性和权限位有效性
- 仿照其他rcore的设计，在`os/src/task/mod.rs`里实现对外的接口和`TaskManager`里的内部方法
    - 对mmap，使用已有的`insert_framed_area`方法
    - 在`os/src/mm/memory_set.rs`里，增加`remove_map_area`方法，删除某个内存段

在`96794b7fb4abe331361e1b1b4f42c514180d0a5f`中，我发现我还需要进行一步额外的对虚拟内存段的检查
- 对mmap，检查传入的虚拟内存段有没有和已分配的内存段重合，增加`check_map_area_overlap`和内部调用的`is_overlapping`函数
- 对munmap，检查入的虚拟内存段是否和已分配的内存段完全重合，增加`check_map_area_equal`和内部调用的`is_equal`

这样，我完成了要求功能

## 重写两个方法

### 地址翻译

这个问题的关键是需要把虚拟指针变成物理指针，则需要实现一个翻译地址的函数，我们仿写翻译页号的`translate`和翻译一段内存区的`translated_byte_buffer`，得到`translated_ptr`


```rust
pub fn translated_ptr(token: usize,ptr:usize) -> usize {
    let page_table = PageTable::from_token(token);
    let start = ptr;
    let start_va = VirtAddr::from(start);
    let vpn = start_va.floor();
    let ppn = page_table.translate(vpn).unwrap().ppn();
    return usize::from(ppn) << 12 | start_va.page_offset();
}
```

在`7b10745fd17b65773e1d3da77eaa989ec3b06945`，则直接朴素调用此方法，得到正确的sys_get_time

```rust
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

```



### 增加开始时间计时

而`sys_task_info`比较微妙，不仅仅要增加虚拟地址翻译

```rust
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info");
    let task = get_current_task();
    trace!("task time is {}",task.time); // 距离任务第一次被调度时刻的时长
    trace!("task status is {:?}",task.status);
    trace!("task syscall times is {:?}",task.syscall_times);
    let _ti = translated_ptr(current_user_token(), _ti as usize) as *mut TaskInfo;
    unsafe {
        *_ti = TaskInfo {
            status: task.status,
            syscall_times: task.syscall_times,
            time: task.time,
        };
    }

    return 0;
}
```

还要仿照ch3，增加task起始时间记录机制和syscall计数机制，见`57e41d97f63d369b97c8aeada9343fe3e38b41c5`。特别需要指出的是，2024fall的代码里，ch4给出的`get_task_info`的时间记录减法是反的，会导致overflow

```rust
pub fn get_task_info(&self) -> TaskInfo {
    let inner = self.inner.exclusive_access();
    let current_task = &inner.tasks[inner.current_task];
    trace!("current task start time is {}",current_task.start_time);
    trace!("cur time is {}",get_time_ms());
    TaskInfo {
        status: current_task.task_status, 
        syscall_times: current_task.syscall_times,
        //原来的错误
        //time: current_task.start_time - get_time_ms(),
        time: get_time_ms() - current_task.start_time,
    }
}
```

到这里，代码可以通过所有测试用例。





