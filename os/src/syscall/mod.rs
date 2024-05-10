//! Implementation of syscalls
//!
//! The single entry point to all system calls, [`syscall()`], is called
//! whenever userspace wishes to perform a system call using the `ecall`
//! instruction. In this case, the processor raises an 'Environment call from
//! U-mode' exception, which is handled as one of the cases in
//! [`crate::trap::trap_handler`].
//!
//! For clarity, each single syscall is implemented as its own function, named
//! `sys_` then the name of the syscall. You can find functions like this in
//! submodules, and you should also implement syscalls this way.
const SYSCALL_WRITE: usize = 64;
/// exit syscall
const SYSCALL_EXIT: usize = 93;
/// yield syscall
const SYSCALL_YIELD: usize = 124;
/// gettime syscall
const SYSCALL_GET_TIME: usize = 169;
/// sbrk syscall
const SYSCALL_SBRK: usize = 214;
/// munmap syscall
const SYSCALL_MUNMAP: usize = 215;
/// mmap syscall
const SYSCALL_MMAP: usize = 222;
/// taskinfo syscall
const SYSCALL_TASK_INFO: usize = 410;

mod fs;
pub mod process;

use fs::*;
use process::*;

use crate::task::{add_current_task_syscall_info, add_current_task_syscall_times};

/// Syscall information
#[derive(Clone, Debug)]
pub struct SyscallInfo {
    /// Syscall id
    pub syscall_id: usize,
    /// Syscall name
    pub syscall_name: &'static str,
}

impl SyscallInfo {
    /// Create a new SyscallInfo
    pub fn new(syscall_id: usize) -> Self {
        Self {
            syscall_id,
            syscall_name: Self::get_syscall_info(syscall_id).unwrap_or("unknown syscall"),
        }
    }

    /// Get syscall info by syscall id
    pub fn get_syscall_info(syscall_id: usize) -> Option<&'static str> {
        match syscall_id {
            SYSCALL_WRITE => Some("write"),
            SYSCALL_EXIT => Some("exit"),
            SYSCALL_YIELD => Some("yield"),
            SYSCALL_GET_TIME => Some("get_time"),
            SYSCALL_SBRK => Some("sbrk"),
            SYSCALL_MUNMAP => Some("munmap"),
            SYSCALL_MMAP => Some("mmap"),
            SYSCALL_TASK_INFO => Some("task_info"),
            _ => None,
        }
    }
    
}


/// handle syscall exception with `syscall_id` and other arguments
pub fn syscall(syscall_id: usize, args: [usize; 3]) -> isize {

    // add current task syscall times
    add_current_task_syscall_times(syscall_id);

    // add current task syscall info
    let syscall_info = SyscallInfo::new(syscall_id);
    add_current_task_syscall_info(syscall_info);    

    match syscall_id {
        SYSCALL_WRITE => sys_write(args[0], args[1] as *const u8, args[2]),
        SYSCALL_EXIT => sys_exit(args[0] as i32),
        SYSCALL_YIELD => sys_yield(),
        SYSCALL_GET_TIME => sys_get_time(args[0] as *mut TimeVal, args[1]),
        SYSCALL_TASK_INFO => sys_task_info(args[0] as *mut TaskInfo),
        SYSCALL_MMAP => sys_mmap(args[0], args[1], args[2]),
        SYSCALL_MUNMAP => sys_munmap(args[0], args[1]),
        SYSCALL_SBRK => sys_sbrk(args[0] as i32),
        _ => panic!("Unsupported syscall_id: {}", syscall_id),
    }
}
