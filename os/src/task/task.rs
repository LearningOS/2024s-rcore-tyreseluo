//! Types related to task management

use alloc::{collections::BTreeMap, vec::Vec};


use crate::syscall::SyscallInfo;

use super::TaskContext;

/// The task control block (TCB) of a task.
#[derive(Clone)]
pub struct TaskControlBlock {
    /// The task status in it's lifecycle
    pub task_status: TaskStatus,
    /// The task context
    pub task_cx: TaskContext,
    /// The task detail information
    pub task_info: TaskInfo
}

/// The status of a task
#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    /// uninitialized
    UnInit,
    /// ready to run
    Ready,
    /// running
    Running,
    /// exited
    Exited,
}

/// The task information
#[derive(Clone,Debug)]
pub struct TaskInfo {
    /// The syscall times of the task, the key is the syscall id, the value is the times
    pub syscall_times: BTreeMap<usize, usize>,
    /// The called syscall list of the task
    pub syscall_list: Vec<SyscallInfo>,
    /// Whether the task is the first dispatched task
    pub is_first_time_dispatched: bool,
    /// The first dispatched time of the task
    pub first_dispatched_time: usize,
}

impl TaskInfo {
    /// Create a default TaskInfo
    pub fn default() -> Self {
        TaskInfo {
            is_first_time_dispatched: true,
            syscall_times: BTreeMap::new(),
            syscall_list: Vec::new(),
            first_dispatched_time: 0,
        }
    }

    /// Set the task as dispatched and record the first dispatched time
    pub fn set_timestamp_is_first_dispatched(&mut self) {
        if self.is_first_time_dispatched {
            self.first_dispatched_time = crate::timer::get_time_us();
            self.is_first_time_dispatched = false;
        }
    }
}
