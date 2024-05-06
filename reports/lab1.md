# Lab1 实验报告

## 实现功能

在TaskControlBlock中添加了task_info字段,其类型为`TaskInfo`, 用于保存以下已实现的功能

- 系统调用次数
    
    - 使用BTreeMap<usize, usize> 来保存任务系统调用次数。

- 系统调用列表
     - 定义了`SyscallInfo`, 用于存放任务调用流程的系统调用详情，有`syscall_id` 和 `syscall_name`两个字段。
    - 增大内核堆内存大小到32MB,主要是vec中存放的syscall_info分配的堆内存不足
       >定义syscall_name最大为25个字符，即25byte, 加上 `syscall_id` 字段的大小usize，约定为64bit = 8byte, 即一个`SyscallInfo` 实例大小为 33byte，对齐。为了好计算我使用40byte * 500 = 20000byte = 20480byte = 20MB
- 任务第一次被调度的时刻

## 简答作业

### 1题

使用的SBI是项目clone下来的`rustsbin-qemu.bin`文件

共性：都是通过异常Exception,来触发Tarp，进而进行特权级的切换，跳转至`stvec`CSR寄存器中的地址去执行，即全局符号__alltraps处，执行栈分配和保存一系列寄存器状态后，通过伪指令call 调用 trap_handler函数。函数根据Trap异常不同进行不同的异常处理。这三个程序都调用了`exit_current_and_run_next`来结束应用同时调用下一个应用。

- ch2b_bad_address.rs 尝试往0x0处写入数据0, 触发异常`Store/AMO access fault`
- ch2b_bad_instructions.rs 尝试执行S模式指令`sret`, 触发异常`IllegalInstruction`
- ch2b_bad_register.rs  尝试读取S模式下`sstatus`寄存器，触发异常`IllegalInstruction`

### 2题

#### 1. L40

进入__restore时，有三种情况：

1. 从run_first_task开始执行，第一个任务首次调度，从__switch的最后一条指令ret进入。
此时a0的值是boot stack上的地址(即_unused: TaskContext的地址)。

2. 非第一个任务，但同样是首次调度，由run_next_task触发，同样从__switch的最后一条指令ret进入。此时a0的值是TaskManager中前一个任务的TaskContext的地址。

3. 从__alltraps进入，在call trap_handler返回之后继续向下执行进入__restore，此时a0是trap_handler返回的指针，与前一条指令mv a0, sp传入的地址没有区别，因为trap_handler直接返回了原引用。

__restore的两种使用场景：

1. 从S模式切换至U模式(sret指令)，对应到前面两种情况，区别只是进入时a0指向的地址区域不同。
2. 在U模式下触发trap进入S模式，保存状态并执行trap_handler之后恢复状态，对应到第三种情况。

与第二章的不同在于第三章的__restore开头没有mv sp, a0指令，因为sp已经在__switch后半段设置为内核栈上的TrapContext的指针，只需要直接跳转至__restore即可，无需通过a0传递。



#### 2. L43-L48 

特殊处理了：`sstatus` `sepc` `sscratch` 这三个寄存器

- 将`t0`寄存器中触发trap中断异常的原始特权级状态恢复到 `sstatus` 寄存器中

- 将`t1`寄存器中trap中断异常下一条指令,恢复到 `sepc` 寄存器中

- 将`t2`寄存器中用户栈指针恢复到`sscratch` 寄存器中

意义：能让处于S模式下的OS处理完Trap异常中断后，能恢复U模式，且同时保存着U模式下的上下文环境，使得程序能从S模式下跳转到M模式后继续执行。

#### 3. L50-56

`x2`是`sp`栈指针，预留内存位置， 会在`sret`执行之前被恢复,所以最开始处理trap时可以不着急处理。

根据手册：`x4`是`tp`指针，目前还没涉及线程相关，用途是指向线程独有的数据。

#### 4. L60

```asm
csrrw sp, sscratch, sp
```

将`sscratch`中的旧值赋给`sp`，将`sp`的新值赋给`sscratch`，交换`sp`和`sscratch`的值

即：
- `sp`是user stack的栈顶
- `sscratch`是kernel stack的栈顶

#### 5.

`__restore`最后一条指令`sret`将会从S模式回到U模式。

执行 sret 指令会导致处理器从 S 模式切换回到之前保存的 U 模式的状态，包括用户程序的程序计数器和其他相关的寄存器状态。


#### 6.

该指令交换sscratch和sp的值，交换后：

- `sscratch`是user stack的栈顶
- `sp`是kernel stack的栈顶

#### 7.

系统调用，`ecall`

`ecall` 指令的功能是触发异常，然后由异常处理机制来进行状态切换。
当执行 "ecall" 指令时，处理器会生成一个异常，并跳转到特定的异常处理程序`trap.S`，分配栈同时保存当前U模式下的Trap上下文环境，进入S模式下执行。

## 荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

    无

2. 此外，我也参考了 以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

    - Rust的类型在内存中的布局：[Rust Language Cheat Sheet](https://cheats.rs/)
    - 有关RISC-V相关的特权级和非特权指令，主要来自RISC-V官方手册
    - 代码实现并未参考任何已有资料，代码实现由我个人完成

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。