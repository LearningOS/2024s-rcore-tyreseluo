//!Implementation of [`TaskManager`]
use super::TaskControlBlock;
use crate::sync::UPSafeCell;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;

const BIG_STRIDE: usize = 0x10000000;

///A array of `TaskControlBlock` that is thread-safe
pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

/// A simple FIFO scheduler.
impl TaskManager {
    ///Creat an empty TaskManager
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
    /// Add process back to ready queue
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task);
    }
    /// Take a process out of the ready queue
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.ready_queue.pop_front()
    }

    /// Take a process out of the ready queue with stride scheduling
    pub fn fetch_min_stride_task(&mut self) -> Option<Arc<TaskControlBlock>> {
        //default first task is min_stride_task
        let mut min_tcb = self.ready_queue[0].clone();
        let min_stride_task = min_tcb.inner_exclusive_access();
        let mut min_stride = min_stride_task.stride;
        drop(min_stride_task);

        //find min_stride_task
        for tcb in self.ready_queue.iter() {
            let task = tcb.inner_exclusive_access();
            if task.stride < min_stride {
                min_tcb = tcb.clone();
                min_stride = task.stride;
            }
        }

        // remove min_stride_task from ready_queue
        if let Some(index) = self.ready_queue.iter().position(|x| Arc::ptr_eq(x, &min_tcb)) {
            self.ready_queue.remove(index);
        }

        let mut min_stride_task = min_tcb.inner_exclusive_access();
        // update stride
        min_stride_task.stride = min_stride_task.stride + BIG_STRIDE / min_stride_task.priority as usize;

        drop(min_stride_task);
        Some(min_tcb)
    }
}

lazy_static! {
    /// TASK_MANAGER instance through lazy_static!
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

/// Add process to ready queue
pub fn add_task(task: Arc<TaskControlBlock>) {
    //trace!("kernel: TaskManager::add_task");
    TASK_MANAGER.exclusive_access().add(task);
}

/// Take a process out of the ready queue
pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    //trace!("kernel: TaskManager::fetch_task");
    TASK_MANAGER.exclusive_access().fetch()
}

pub fn fetch_min_task() -> Option<Arc<TaskControlBlock>> {
    //trace!("kernel: TaskManager::fetch_task");
    TASK_MANAGER.exclusive_access().fetch_min_stride_task()
}
