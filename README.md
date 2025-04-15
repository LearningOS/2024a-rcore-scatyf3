# rCore-Camp-Code-2024A

### Code
- [Soure Code of labs for 2024A](https://github.com/LearningOS/rCore-Camp-Code-2024A)
### Documents

- Concise Manual: [rCore-Camp-Guide-2024A](https://LearningOS.github.io/rCore-Camp-Guide-2024A/)

- Detail Book [rCore-Tutorial-Book-v3](https://rcore-os.github.io/rCore-Tutorial-Book-v3/)


### OS API docs
- [ch1](https://learningos.github.io/rCore-Camp-Code-2024A/ch1/os/index.html) [ch2](https://learningos.github.io/rCore-Camp-Code-2024A/ch2/os/index.html) [ch3](https://learningos.github.io/rCore-Camp-Code-2024A/ch3/os/index.html) [ch4](https://learningos.github.io/rCore-Camp-Code-2024A/ch4/os/index.html)
- [ch5](https://learningos.github.io/rCore-Camp-Code-2024A/ch5/os/index.html) [ch6](https://learningos.github.io/rCore-Camp-Code-2024A/ch6/os/index.html) [ch7](https://learningos.github.io/rCore-Camp-Code-2024A/ch7/os/index.html) [ch8](https://learningos.github.io/rCore-Camp-Code-2024A/ch8/os/index.html)

### Related Resources
- [Learning Resource](https://github.com/LearningOS/rust-based-os-comp2022/blob/main/relatedinfo.md)


### Build & Run

Replace `<YourName>` with your github ID, and replace `<Number>` with the chapter ID.

Notice: `<Number>` is chosen from `[1,2,3,4,5,6,7,8]`

```bash
# 
$ git clone git@github.com:LearningOS/2024a-rcore-<YourName>
$ cd 2024a-rcore-<YourName>
$ git clone git@github.com:LearningOS/rCore-Tutorial-Test-2024A user
$ git checkout ch<Number>
$ cd os
$ make run
```

### Grading

Replace `<YourName>` with your github ID, and replace `<Number>` with the chapter ID.

Notice: `<Number>` is chosen from `[3,4,5,6,8]`

```bash
# Replace <YourName> with your github ID 
$ git clone git@github.com:LearningOS/2024a-rcore-<YourName>
$ cd 2024a-rcore-<YourName>
$ rm -rf ci-user
$ git clone git@github.com:LearningOS/rCore-Tutorial-Checker-2024A ci-user
$ git clone git@github.com:LearningOS/rCore-Tutorial-Test-2024A ci-user/user
$ git checkout ch<Number>
$ cd ci-user
$ make test CHAPTER=<Number>
```


```
 1├── os
 2   ├── build.rs(修改：基于应用名的应用构建器)
 3   ├── ...
 4   └── src
 5       ├── ...
 6       ├── loader.rs(修改：基于应用名的应用加载器)
 7       ├── main.rs(修改)
 8       ├── mm(修改：为了支持本章的系统调用对此模块做若干增强)
 9       │   ├── address.rs
10       │   ├── frame_allocator.rs
11       │   ├── heap_allocator.rs
12       │   ├── memory_set.rs
13       │   ├── mod.rs
14       │   └── page_table.rs
15       ├── syscall
16       │   ├── fs.rs(修改：新增 sys_read)
17       │   ├── mod.rs(修改：新的系统调用的分发处理)
18       │   └── process.rs（修改：新增 sys_getpid/fork/exec/waitpid）
19       ├── task
20       │   ├── context.rs
21       │   ├── manager.rs(新增：任务管理器，为上一章任务管理器功能的一部分)
22       │   ├── mod.rs(修改：调整原来的接口实现以支持进程)
23       │   ├── pid.rs(新增：进程标识符和内核栈的 Rust 抽象)
24       │   ├── processor.rs(新增：处理器管理结构 ``Processor`` ，为上一章任务管理器功能的一部分)
25       │   ├── switch.rs
26       │   ├── switch.S
27       │   └── task.rs(修改：支持进程机制的任务控制块)
28       └── trap
29           ├── context.rs
30           ├── mod.rs(修改：对于系统调用的实现进行修改以支持进程系统调用)
31           └── trap.S
```