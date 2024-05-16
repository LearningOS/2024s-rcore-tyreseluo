//! Types related to task management & Functions for completely changing TCB
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use super::TaskContext;
use super::{kstack_alloc, pid_alloc, KernelStack, PidHandle};
use crate::config::{MAX_SYSCALL_NUM, PAGE_SIZE, TRAP_CONTEXT_BASE};
use crate::mm::{MapPermission, MemorySet, PhysPageNum, VirtAddr, KERNEL_SPACE};
use crate::sync::UPSafeCell;
use crate::syscall::SyscallInfo;
use crate::trap::{trap_handler, TrapContext};
use alloc::sync::{Arc, Weak};
use core::cell::RefMut;

/// Task control block structure
///
/// Directly save the contents that will not change during running
pub struct TaskControlBlock {
    // Immutable
    /// Process identifier
    pub pid: PidHandle,

    /// Kernel stack corresponding to PID
    pub kernel_stack: KernelStack,

    /// Mutable
    inner: UPSafeCell<TaskControlBlockInner>,
}

impl TaskControlBlock {
    /// Get the mutable reference of the inner TCB
    pub fn inner_exclusive_access(&self) -> RefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }
    /// Get the address of app's page table
    pub fn get_user_token(&self) -> usize {
        let inner = self.inner_exclusive_access();
        inner.memory_set.token()
    }
}

pub struct TaskControlBlockInner {
    /// The physical page number of the frame where the trap context is placed
    pub trap_cx_ppn: PhysPageNum,

    /// Application data can only appear in areas
    /// where the application address space is lower than base_size
    pub base_size: usize,

    /// Save task context
    pub task_cx: TaskContext,

    /// Maintain the execution status of the current process
    pub task_status: TaskStatus,

    /// scheduling priority
    pub priority: isize,

    /// current stride
    pub stride: usize,

    /// Task information
    pub task_info: TaskInfo,

    /// Application address space
    pub memory_set: MemorySet,

    /// Parent process of the current process.
    /// Weak will not affect the reference count of the parent
    pub parent: Option<Weak<TaskControlBlock>>,

    /// A vector containing TCBs of all child processes of the current process
    pub children: Vec<Arc<TaskControlBlock>>,

    /// It is set when active exit or execution error occurs
    pub exit_code: i32,

    /// Heap bottom
    pub heap_bottom: usize,

    /// Program break
    pub program_brk: usize,
}

impl TaskControlBlockInner {
    /// get the trap context
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        self.trap_cx_ppn.get_mut()
    }
    /// get the user token
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }
    fn get_status(&self) -> TaskStatus {
        self.task_status
    }
    pub fn is_zombie(&self) -> bool {
        self.get_status() == TaskStatus::Zombie
    }
}

/// TCBImp is the implementation of TaskControlBlock
impl TaskControlBlock {
    /// Create a new process
    ///
    /// At present, it is only used for the creation of initproc
    pub fn new(elf_data: &[u8]) -> Self {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();
        // alloc a pid and a kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kernel_stack = kstack_alloc();
        let kernel_stack_top = kernel_stack.get_top();
        // push a task context which goes to trap_return to the top of kernel stack
        let task_control_block = Self {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: user_sp,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    priority: 16,
                    stride: 0,
                    memory_set,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                    heap_bottom: user_sp,
                    program_brk: user_sp,
                    task_info: TaskInfo::default(),
                })
            },
        };
        // prepare TrapContext in user space
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            kernel_stack_top,
            trap_handler as usize,
        );
        task_control_block
    }

    /// Load a new elf to replace the original application address space and start execution
    pub fn exec(&self, elf_data: &[u8]) {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();

        // **** access current TCB exclusively
        let mut inner = self.inner_exclusive_access();
        // substitute memory_set
        inner.memory_set = memory_set;
        // update trap_cx ppn
        inner.trap_cx_ppn = trap_cx_ppn;
        // initialize base_size
        inner.base_size = user_sp;
        // initialize trap_cx
        let trap_cx = inner.get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            self.kernel_stack.get_top(),
            trap_handler as usize,
        );
        // **** release inner automatically
    }

    /// parent process fork the child process
    pub fn fork(self: &Arc<Self>) -> Arc<Self> {
        // ---- access parent PCB exclusively
        let mut parent_inner = self.inner_exclusive_access();
        // copy user space(include trap context)
        let memory_set = MemorySet::from_existed_user(&parent_inner.memory_set);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();
        // alloc a pid and a kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kernel_stack = kstack_alloc();
        let kernel_stack_top = kernel_stack.get_top();
        let task_control_block = Arc::new(TaskControlBlock {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: parent_inner.base_size,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    priority: 16,
                    stride: 0,
                    memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    heap_bottom: parent_inner.heap_bottom,
                    program_brk: parent_inner.program_brk,
                    task_info: TaskInfo::default(),
                })
            },
        });
        // add child
        parent_inner.children.push(task_control_block.clone());
        // modify kernel_sp in trap_cx
        // **** access child PCB exclusively
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        trap_cx.kernel_sp = kernel_stack_top;
        // return
        task_control_block
        // **** release child PCB
        // ---- release parent PCB
    }

    /// spawn a new process
    pub fn spawn(&self, elf_data: &[u8]) -> Arc<Self> {
        let new_task = Arc::new(TaskControlBlock::new(elf_data));
        self.inner_exclusive_access().children.push(new_task.clone());
        new_task
    }

    /// get pid of process
    pub fn getpid(&self) -> usize {
        self.pid.0
    }

    /// change the location of the program break. return None if failed.
    pub fn change_program_brk(&self, size: i32) -> Option<usize> {
        let mut inner = self.inner_exclusive_access();
        let heap_bottom = inner.heap_bottom;
        let old_break = inner.program_brk;
        let new_brk = inner.program_brk as isize + size as isize;
        if new_brk < heap_bottom as isize {
            return None;
        }
        let result = if size < 0 {
            inner
                .memory_set
                .shrink_to(VirtAddr(heap_bottom), VirtAddr(new_brk as usize))
        } else {
            inner
                .memory_set
                .append_to(VirtAddr(heap_bottom), VirtAddr(new_brk as usize))
        };
        if result {
            inner.program_brk = new_brk as usize;
            Some(old_break)
        } else {
            None
        }
    }

    /// set the priority of the process
    pub fn set_priority(&self, prio: isize) {
        assert!(prio > 1, "priority should be larger than 1");
        self.inner_exclusive_access().priority = prio
    }


    /// Alloc memory
    pub fn alloc_memory(&self, start: usize, len: usize, port: usize) -> isize {
        if start % PAGE_SIZE != 0 {
            return -1;
        }

        if port & !0x7 != 0 || port & 0x7 == 0 {
            return -1;
        }

        let start_va = VirtAddr::from(start);
        let end_va = VirtAddr::from(start + len);

        let mut inner = self.inner.exclusive_access();
    

        if inner.memory_set.is_allocated(start_va, end_va) {
            return -1;
        }

        let permission = MapPermission::from_bits((port as u8) << 1).unwrap() | MapPermission::U;

        inner.memory_set.insert_framed_area(start_va, end_va, permission);
        0

    }

    /// Dealloc memory
    pub fn dealloc(&self, start: usize, len: usize) -> isize {
        if start % PAGE_SIZE != 0 {
            return -1;
        }

        let start_va = VirtAddr::from(start);
        let end_va = VirtAddr::from(start + len);

        if !start_va.aligned() {
            return -1;
        }

        if !end_va.aligned() {
            return -1;
        }

        let mut inner = self.inner.exclusive_access();
        inner.memory_set.remove_framed_area(start_va, end_va);
        0
    }

    /// get the task status
    pub fn get_task_status(&self) -> TaskStatus {
        self.inner_exclusive_access().task_status
    }

    /// get first dispatched time
    pub fn get_first_dispatched_time(&self) -> usize {
        self.inner_exclusive_access().task_info.first_dispatched_time
    }

    /// get task syscall times
    pub fn get_task_syscall_times(&self) -> [u32; MAX_SYSCALL_NUM]  {
        let times =  self.inner_exclusive_access().task_info.syscall_times.clone();
        let mut syscall_times = [0; MAX_SYSCALL_NUM];
        for (syscall_id, time) in times {
            syscall_times[syscall_id] = time as u32;
        }
        syscall_times
    }

    /// add task syscall times
    pub fn add_task_syscall_times(&self, syscall_id: usize) {
        let mut inner = self.inner_exclusive_access();
        let times = &mut inner.task_info.syscall_times;
        *times.entry(syscall_id).or_default() += 1;
    }
    
    /// add task syscall info
    pub fn add_task_syscall_info(&self, syscall_info: SyscallInfo) {
        let mut inner = self.inner_exclusive_access();
        inner.task_info.syscall_list.push(syscall_info);
    }

}

#[derive(Clone, Debug)]
pub struct TaskInfo {
    pub is_first_time_dispatched: bool,
    /// The first dispatched time of the task
    pub first_dispatched_time: usize,
    /// System call times, the index is the syscall number, and the value is the call times
    pub syscall_times: BTreeMap<usize, u32>,
    /// The called syscall list of the task
    pub syscall_list: Vec<SyscallInfo>,
}

impl TaskInfo {
    pub fn default() -> Self {
        TaskInfo {
            is_first_time_dispatched: true,
            first_dispatched_time: 0,
            syscall_times: BTreeMap::new(),
            syscall_list: Vec::new(),
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


#[derive(Copy, Clone, PartialEq)]
/// task status: UnInit, Ready, Running, Exited
pub enum TaskStatus {
    /// uninitialized
    /// (only for the task that has not been added to the scheduler)
    UnInit,
    /// ready to run
    Ready,
    /// running
    Running,
    /// exited
    Zombie,
}
