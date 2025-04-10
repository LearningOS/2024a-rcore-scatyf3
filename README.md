

项目结构
```
├── os
│   ├── ...
│   └── src
│       ├── ...
│       ├── config.rs(修改：新增一些内存管理的相关配置)
│       ├── linker.ld(修改：将跳板页引入内存布局)
│       ├── loader.rs(修改：仅保留获取应用数量和数据的功能)
│       ├── main.rs(修改)
│       ├── mm(新增：内存管理的 mm 子模块)
│       │   ├── address.rs(物理/虚拟 地址/页号的 Rust 抽象)
│       │   ├── frame_allocator.rs(物理页帧分配器)
│       │   ├── heap_allocator.rs(内核动态内存分配器)
│       │   ├── memory_set.rs(引入地址空间 MemorySet 及逻辑段 MemoryArea 等)
│       │   ├── mod.rs(定义了 mm 模块初始化方法 init)
│       │   └── page_table.rs(多级页表抽象 PageTable 以及其他内容)
│       ├── syscall
│       │   ├── fs.rs(修改：基于地址空间的 sys_write 实现)
│       │   ├── mod.rs
│       │   └── process.rs
│       ├── task
│       │   ├── context.rs(修改：构造一个跳转到不同位置的初始任务上下文)
│       │   ├── mod.rs(修改，详见文档)
│       │   ├── switch.rs
│       │   ├── switch.S
│       │   └── task.rs(修改，详见文档)
│       └── trap
│           ├── context.rs(修改：在 Trap 上下文中加入了更多内容)
│           ├── mod.rs(修改：基于地址空间修改了 Trap 机制，详见文档)
│           └── trap.S(修改：基于地址空间修改了 Trap 上下文保存与恢复汇编代码)
└── user
    ├── build.py(编译时不再使用)
    ├── ...
    └── src
        ├── linker.ld(修改：将所有应用放在各自地址空间中固定的位置)
        └── ...
```

satp寄存器，mmu用的，mode控制开关
```
| 63 - 60 | 59 - 48 | 47 - 0  |
|   MODE  |   ASID  |   PPN   |
```