//! File and filesystem-related syscalls
use crate::mm::translated_byte_buffer;
use crate::sbi::console_getchar;
use crate::task::{current_task, current_user_token, suspend_current_and_run_next};

const FD_STDIN: usize = 0;
const FD_STDOUT: usize = 1;

/// write buf of length `len`  to a file with `fd`
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    match fd {
        FD_STDOUT => {
            let buffers = translated_byte_buffer(current_user_token(), buf, len);
            for buffer in buffers {
                print!("{}", core::str::from_utf8(buffer).unwrap());
            }
            len as isize
        }
        _ => {
            panic!("Unsupported fd in sys_write!");
        }
    }
}

/// 功能：从文件中读取一段内容到缓冲区，为了让user_shell 需要捕获用户输入并进行解析处理
/// 参数：fd 是待读取文件的文件描述符，切片 buffer 则给出缓冲区。
/// 返回值：如果出现了错误则返回 -1，否则返回实际读到的字节数。
/// syscall ID：63
pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    match fd {
        // 仅支持从标准输入 FD_STDIN 即文件描述符 0 读入，且每次只能读入一个字符
        FD_STDIN => {
            assert_eq!(len, 1, "Only support len = 1 in sys_read!");
            let mut c: usize;
            loop {
                // 这是利用 sbi 提供的接口 console_getchar 实现的
                c = console_getchar();
                if c == 0 {
                    // 如果还没有输入，我们就切换到其他进程，等下次切换回来时再看看是否有输入了。 
                    suspend_current_and_run_next();
                    continue;
                } else {
                    // 取到输入后就退出循环
                    break;
                }
            }
            // 并手动查页表将输入字符正确写入到应用地址空间。
            let ch = c as u8;
            let mut buffers = translated_byte_buffer(current_user_token(), buf, len);
            unsafe {
                buffers[0].as_mut_ptr().write_volatile(ch);
            }
            1
        }
        _ => {
            panic!("Unsupported fd in sys_read!");
        }
    }
}

/*
// 我们在用户库中将其进一步封装成每次能够从 标准输入 中获取一个字符的 getchar 函数。

#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate user_lib;

const LF: u8 = 0x0au8;
const CR: u8 = 0x0du8;
const DL: u8 = 0x7fu8;
const BS: u8 = 0x08u8;

use alloc::string::String;
use user_lib::console::getchar;
use user_lib::{exec, flush, fork, waitpid};

#[no_mangle]
pub fn main() -> i32 {
    println!("Rust user shell");
    // 第 23 行声明的字符串 line 则维护着用户当前输入的命令内容，它也在不断发生变化。
    let mut line: String = String::new();
    print!(">> ");
    flush();
    // 可以看到，在以第 25 行开头的主循环中，每次都是调用 getchar 获取一个用户输入的字符， 
    // 并根据它相应进行一些动作。
    loop { 
        let c = getchar();
        match c {
        // 如果用户输入回车键（第 28 行），
            LF | CR => { 
                print!("\n");
                if !line.is_empty() {
                    line.push('\0');
                    // 那么user_shell 会 fork 出一个子进程（第 34 行开始）
                    // 并试图通过 exec 系统调用执行一个应用，应用的名字在字符串 line 中给出。
                    let pid = fork();
                    if pid == 0 {
                        // child process
                        // 如果 exec 的返回值为 -1 ， 说明在应用管理器中找不到对应名字的应用，此时子进程就直接打印错误信息并退出；
                        // 否则子进程将开始执行目标应用。
                        if exec(line.as_str(), &[0 as *const u8]) == -1 {
                            println!("Error when executing!");
                            return -4;
                        }
                        unreachable!();
                    } else {
                        // fork 之后的 user_shell 进程自己的逻辑可以在第 41 行找到。
                        let mut exit_code: i32 = 0;
                        let exit_pid = waitpid(pid as usize, &mut exit_code);
                        assert_eq!(pid, exit_pid);
                        // 它在等待 fork 出来的子进程结束并回收掉它的资源，还会顺带收集子进程的退出状态并打印出来。
                        println!("Shell: Process {} exited with code {}", pid, exit_code);
                    }
                    line.clear();
                }
                print!(">> ");
                flush();
            }
            BS | DL => {
                // 如果用户输入的是退格键
                if !line.is_empty() {
                    // 首先我们需要将屏幕上当前行的最后一个字符用空格替换掉， 这可以通过输入一个特殊的退格字节 BS 来实现。
                    print!("{}", BS as char);
                    print!(" ");
                    print!("{}", BS as char);
                    flush();
                    // 其次，user_shell 进程内维护的 line 也需要弹出最后一个字符。
                    line.pop();
                }
            }
            // 如果用户输入了一个其他字符（第 61 行），就接将它打印在屏幕上，并加入到 line 中。
            _ => {
                print!("{}", c as char);
                flush();
                line.push(c as char);
            }
        }
    }
}




*/