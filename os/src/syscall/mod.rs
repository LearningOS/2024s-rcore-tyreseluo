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

/// write syscall
const SYSCALL_WRITE: usize = 64;
/// exit syscall
const SYSCALL_EXIT: usize = 93;
/// yield syscall
const SYSCALL_YIELD: usize = 124;
/// gettime syscall
const SYSCALL_GET_TIME: usize = 169;
/// taskinfo syscall
const SYSCALL_TASK_INFO: usize = 410;

mod fs;
mod process;

use fs::*;
use process::*;

use crate::task::add_current_task_syscall_times;

/// handle syscall exception with `syscall_id` and other arguments
pub fn syscall(syscall_id: usize, args: [usize; 3]) -> isize {
    
    // get the syscall name
    //let syscall_name = get_syscall_name(syscall_id);
    
    // create a SyscallInfo
    // let syscall_info = SyscallInfo {
    //     syscall_id,
    //     syscall_name,
    // };
    
    // add the syscall times to the current task
    add_current_task_syscall_times(syscall_id);
    
    // add the syscall_info to the current task
    //add_current_task_syscall_info(syscall_info);
    
    match syscall_id {
        SYSCALL_WRITE => sys_write(args[0], args[1] as *const u8, args[2]),
        SYSCALL_EXIT => sys_exit(args[0] as i32),
        SYSCALL_YIELD => sys_yield(),
        SYSCALL_GET_TIME => sys_get_time(args[0] as *mut TimeVal, args[1]),
        SYSCALL_TASK_INFO => sys_task_info(args[0] as *mut TaskInfo),
        _ => panic!("Unsupported syscall_id: {}", syscall_id),
    }
}

/// Get the name of a syscall by its id
pub fn get_syscall_name(syscall_id: usize) -> &'static str {
    match syscall_id {
        SYSCALL_WRITE => "write",
        SYSCALL_EXIT => "exit",
        SYSCALL_YIELD => "yield",
        SYSCALL_GET_TIME => "get_time",
        SYSCALL_TASK_INFO => "task_info",
        _ => "Unsupported syscall_id",
    }
}

/// Information about a syscall
#[derive(Clone, Debug)]
pub struct SyscallInfo {
    /// The syscall id
    pub syscall_id: usize,
    /// The syscall name
    pub syscall_name: &'static str,
}