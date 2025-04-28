# lab1 report
 It's the **first time** i use markdown to finish a report!
## 实现的功能-sys_trace函数
### 功能1
    - trace_request为0时，读取id(作为\*const u8)处一个字节的无符号整数，并作为返回值.
    - 实现思路：\*解引用.
### 功能2
    - trace_request为1，则将data写入至id\*(作为const u8)的地址处，返回值为0.
    - 实现思路：\*解引用，然后赋值.
### 功能3
#### 最初的思路
    - 创建一个全局数组syscall_times,大小为SYS_CALL_NUM=411(比syscall_trace的系统调用号大1).这样系统调用号就可以作为索引.
    - 进入syscall函数后，根据参数syscall_id修改数组中的对应项.
    - 问题：测例执行到第一次断言syscall_write的调用次数时，由于之前的调试信息也会调用该系统调用，同时syscall_times数组为全局即所有任务共享。故无法显示所希望的数据.
    - 解决：在讨论群的聊天记录里搜索了关键词"write",发现有群友遇到过类似的问题;有一位同学提到其实现方法是修改TaskControlBlock。我恍然大悟，因为这样计数数组就非所有任务共享，而是每个task均有一个计数数组。
#### 修改思路
    - 删除了大部分原有数据结构。在TaskControlBlock中增加字段syscall_count:[usize;SYS_CALL_NUM]
    - 在TaskManager的impl块中增加两个function，get_syscall_count用于获取syscall_count，inc_syscall_count用于修改syscall_count
    - 在TaskManager的impl块*外*增加两个function:syscall_count_get,syscall_count_inc，包裹上述两个函数。
    - 在syscall函数中调用syscall_count_inc;在sys_trace函数中分支3调用syscall_count_get。

## 简答作业
**it seems that bold is invalid for chinese**
(1)
-版本：[rustsbi] Implementation     : RustSBI-QEMU Version 0.2.0-alpha.2
-运行结果：我不太了解该如何运行那些测例，所以我在ci-user目录下运行 make test CHAPTER=2 并没有panic，但有如下输出，我觉得好像是和测例对应的。
```
    [kernel] PageFault in application, bad addr = 0x0, bad instruction = 0x804003a4, kernel killed it.
    [kernel] IllegalInstruction in application, kernel killed it.
    [kernel] IllegalInstruction in application, kernel killed it.
    [kernel] Panicked at src/syscall/fs.rs:11 called `Result::unwrap()` on an `Err` value: Utf8Error { valid_up_to: 3, error_len: Some(1) }
```
(2)trap.S
    1.刚进入__restore时，sp代表的system stack，即指向内核栈（栈顶）。
      __restore的两种使用情景：
        - 正常的上下文切换
        - 系统第一次由系统态进入用户态
    2.特殊处理了以下三个寄存器

        | name | function |
        |:---:|:---:|
        | `sstatus` | SPP 等字段给出 Trap 发生之前 CPU 处在哪个特权级（S/U）等信息 |
        | `sepc` | 当 Trap 是一个异常的时候，记录 Trap 发生之前执行的最后一条指令的地址 |
        | `sscratch` | 指向用户栈（栈顶）*和sp交换后  |
    
        - sstatus保存了trap发生之前cpu所处的特权级别，这样在执行sret后，cpu可以自动切换会U-mode，从而保证正确的访问权限。
        - sepc用于返回正确的位置，使能够按照原来的控制流/顺序继续执行.
        - sscratch，保存用户栈栈顶指针，这样执行完处理程序之后可以恢复原有的用户栈.
    3.
        - 跳过了x2，因为L45已经把x2的内容（用户栈栈顶）加载到t2中，并恢复到了sscratch中，故此处直接跳过即可。
        - 跳过了x4，因为x4是tp（thread pointer）线程指针寄存器，实验指导书上写到："除非我们手动出于一些特殊用途使用它，否则一般也不会被用到"。但具体为什么，我还是不了解。（__alltraps中也并未处理x4，此处对应忽略）
    4.
        -该指令之后，sp中的值为用户栈栈顶，sscratch中的值为内核栈栈顶
    5. 
        - 状态切换发生在sret 
        - sret会跳转到sepc保存的返回地址，从中断发生的下一条指令继续执行，即从内核态恢复到用户态。
    6.
        - 该指令之后，sp中的值为内核栈栈顶，sscratch中的值为用户栈栈顶
    7.
        - 从U态进入S态，是从用户调用ecall/发生异常之后硬件自动切换的，早于trap.S的第一条指令。