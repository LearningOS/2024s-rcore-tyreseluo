//! Process management syscalls
use crate::{
    config::MAX_SYSCALL_NUM, mm::get_phyical_address, task::{
        change_program_brk, current_task_mmap, current_task_munmap, current_task_status, current_task_syscall_times, current_user_token, exit_current_and_run_next, first_dispatched_time, suspend_current_and_run_next, TaskStatus
    }, timer::{get_time_ms, get_time_us} 
};

#[repr(C)]
#[derive(Debug)]
/// Time value
pub struct TimeVal {
    /// Seconds since Unix epoch
    pub sec: usize,
    /// Microseconds
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    // 获取当前地址空间的页表
    let token = current_user_token();
    let physical_address = get_phyical_address(token, ts as usize);
    let time = get_time_us();

    unsafe {
        *(physical_address as *mut TimeVal) = TimeVal {
            sec: time / 1_000_000,
            usec: time % 1_000_000,
        };
    }
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info");
    let token = current_user_token();
    let physical_address = get_phyical_address(token, ti as usize);
    let ptr = physical_address as *mut TaskInfo;
    unsafe {
        (*ptr).status = current_task_status();
        (*ptr).syscall_times = current_task_syscall_times();
        (*ptr).time = get_time_ms() - first_dispatched_time();
    }
    0
}

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    trace!("kernel: sys_mmap");
    current_task_mmap(start, len, port)
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap");
    current_task_munmap(start, len)
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
