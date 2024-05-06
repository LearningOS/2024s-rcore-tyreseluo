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

use crate::config::{MAX_APP_NUM, MAX_SYSCALL_NUM};
use crate::loader::{get_num_app, init_app_cx};
use crate::sync::UPSafeCell;
use alloc::collections::BTreeMap;
use lazy_static::*;
use switch::__switch;
pub use task::{TaskControlBlock, TaskStatus, TaskInfo};

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

/// Inner of Task Manager
pub struct TaskManagerInner {
    /// task list
    tasks: [TaskControlBlock; MAX_APP_NUM],
    /// id of current `Running` task
    current_task: usize,
}

lazy_static! {
    /// Global variable: TASK_MANAGER
    pub static ref TASK_MANAGER: TaskManager = {
        let num_app = get_num_app();
        let mut tasks: [TaskControlBlock; MAX_APP_NUM]= core::array::from_fn(|_| {
            TaskControlBlock {
                task_cx: TaskContext::zero_init(),
                task_status: TaskStatus::UnInit,
                task_info: TaskInfo::default(),
            }
        });
        for (i, task) in tasks.iter_mut().enumerate() {
            task.task_cx = TaskContext::goto_restore(init_app_cx(i));
            task.task_status = TaskStatus::Ready;
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
    /// But in ch3, we load apps statically, so the first task is a real app.
    fn run_first_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let task0 = &mut inner.tasks[0];
        task0.task_status = TaskStatus::Running;
        task0.task_info.set_timestamp_is_first_dispatched();
        let next_task_cx_ptr = &task0.task_cx as *const TaskContext;
        drop(inner);
        let mut _unused = TaskContext::zero_init();
        // before this, we should drop local variables that must be dropped manually
        unsafe {
            __switch(&mut _unused as *mut TaskContext, next_task_cx_ptr);
        }
        panic!("unreachable in run_first_task!");
    }

    /// Change the status of current `Running` task into `Ready`.
    fn mark_current_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status = TaskStatus::Ready;
    }

    /// Change the status of current `Running` task into `Exited`.
    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status = TaskStatus::Exited;
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

    /// Get the task status of the current task
    fn get_current_task_status(&self) -> TaskStatus {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status
    }

    /// Get the task syscall times of the current task
    /// the key is the syscall id, the value is the times
    fn get_current_task_syscall_times(&self) -> BTreeMap<usize, usize> {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_info.syscall_times.clone()
    }

    /// Get the task syscall list of the current task
    // fn get_current_task_syscall_list(&self) -> Vec<SyscallInfo> {
    //     let inner = self.inner.exclusive_access();
    //     let current = inner.current_task;
    //     inner.tasks[current].task_info.syscall_list.clone()
    // }

    /// Add syscall call times to current task;
    fn add_current_task_syscall_times(&self, syscall_id: usize) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        let task_info = &mut inner.tasks[current].task_info;
        *task_info.syscall_times.entry(syscall_id).or_insert(0) += 1;
    }

    /// Add syscall info to current task;
    // fn add_current_task_syscall_info(&self, syscall_info: SyscallInfo) {
    //     let mut inner = self.inner.exclusive_access();
    //     let current = inner.current_task;
    //     let task_info = &mut inner.tasks[current].task_info;
    //     task_info.syscall_list.push(syscall_info);
    // }

    /// Get the first dispatched time of the current task
    fn get_current_task_first_dispatched_time(&self) -> usize {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_info.first_dispatched_time
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

/// Get the task status of the current task
pub fn current_task_status() -> TaskStatus {
    TASK_MANAGER.get_current_task_status()
}

/// Get the task syscall times of the current task
pub fn current_task_syscall_times() -> [u32; MAX_SYSCALL_NUM] {
    let syscall_times_map = TASK_MANAGER.get_current_task_syscall_times();
    let mut syscall_times = [0; MAX_SYSCALL_NUM];

    for (syscall_id, times) in syscall_times_map {
        syscall_times[syscall_id] = times as u32;
    }
    syscall_times
}
/// Add syscall times to current task;
pub fn add_current_task_syscall_times(syscall_id: usize) {
    TASK_MANAGER.add_current_task_syscall_times(syscall_id);
}
/// Get the task syscall list of the current task
// pub fn current_task_syscall_list() -> Vec<SyscallInfo> {
//     TASK_MANAGER.get_current_task_syscall_list()
// }
/// Add syscall info to current task;
// pub fn add_current_task_syscall_info(syscall_info: SyscallInfo) {
//     TASK_MANAGER.add_current_task_syscall_info(syscall_info);
// }

/// Get the first dispatched time of the current task
pub fn current_task_first_dispatched_time() -> usize {
    TASK_MANAGER.get_current_task_first_dispatched_time()
}