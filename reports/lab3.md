# chapter5 报告
## 各功能实现思路
### system_get_time
    - 获得当前进程的token以访问memory_set
    - 通过 translated_byte_buffer()方法获得目标位置的切片向量。这里的目标位置指的是_ts指向的位置。在ch4中我就希望能利用这样一个方法去写入物理页，因为对于跨页的处理会更方便一些，然而ch4没有这个方法，所以我还是用get_bytes_array的方法手动处理跨页的情况。
    - 但是整体思路和ch4一样，都是把要写入的time_val当做一个&[u8]，这样就可以对translated_byte_buffer()返回的容器中的每个切片进行copy_from_slice(),跨页问题也是自然解决了。

### mmap
    - 几乎和ch4的实现方法没什么区别
    - 先检查_start和_port是否合法
    - 再获得当前任务的pcb，然后查页表确定[_start,_start+len)未被映射
        - 未被映射体现在vpn对应的translate返回值为None
        - 这里我有疑问，就是在ch4和ch5中我都是在find_pte方法中添加了一行
        ```
            if i == 2 {
                //result = Some(pte);
                if pte.is_valid() { result =  Some(pte) } else { result =None }; // 仅返回有效项 为什么改了这行就ok了？？
                break;
            }
        ```
        - 也就是在查询到三级列表的时候，仅在is_valid的时候才返回Some(pte) 否则返回None。为什么要改这个，是因为我在测试mmap相关测例时，发现我写的mmap无法实现连续分配，把[start,start+len)分配完[start+len,len*2)就无法分配，会打印我自己给的调试信息“already mapped”，也就是已映射-即查询start+len的位置返回的是Some(_)。
        理论上应当返回None。
        然后我就去查translate的实现，发现很奇怪，因为在map的时候会把PTEflag | PTEflags:V，所以按理说只要被映射了，初始状态应该就是有效的……但是我还是改了这一处代码，就通过了。
### munmap
    - 整体实现思路和mmap类似，因为两者是对应的
    - 先检查_start是否合法
    - 获取当前PCB，查页表检查要取消映射的虚拟地址范围是否之前未被映射。未被映射返回-1
    - 已被映射则调用memory_set的remove_area_with_start_vpn
### spawn
    - 不知道为什么看到spawn=fork+exec的提示我就完全不想动脑子了，最后几乎也如提示所言从fork和exec的代码块中东拼西凑就凑出来了。
    - 分成sys_spawn和在task.rs中添加的spawn方法
#### sys_spawn
    - 要先从_path中获取应用的名称，保存在path中，然后像exec那样利用get_app_data_by_name(path)得到elf二进制数据保存在data中；如果失败就说明应用名称错误，返回-1
    - 成功说明应用名称存在，继续。因为spawn本质上还是要new一个子进程挂在当前进程下，所以task.rs中实现的spawn要和fork类似，且返回这个新的子进程。
    - 又因为sys_spawn也是要求把子进程pid的值返回给父进程，所以需要获取子进程的pid——new_task.pid.0
    - 除此之外，子进程要返回的是0，所以获得子进程的trap上下文，设置.x[10]=0
    - 并把子进程加入就绪队列
#### spawn (task.rs中)
    - 利用from_elf通过传入的elf数据得到新进程的memory_set,user_sp,entry_point
    - 获取/创建构建PCB所需要的各个字段的值即可，然后再创建子进程的PCB。
    - 建立双向的parent-children关系
    - 修改trap上下文，这里仿照exec即可。
    - 最后返回PCB
### set_priority
    - 不用写stride的话这个还是很简单的
    - 不过我在fork、spawn的时候都把prio*2了，因为我们linux操作系统课上有类似的实现，我习惯性地添加了此处。不过那个是把时间片减半，但道理是一样的。

## 问答作业
### 1 
    - 实际轮不到p1执行，因为p2的stride变成4了，还是小于p1的stride，所以还是p2执行
### 2
    假设当前有k个进程，下一个被选择执行的进程的stride一定是最小的，设为STRIDE_MIN，进程号记为k0；stride最大的进程记为k1，其stride记为STRIDE_MAX
    当它被运行后，pass<=BigStride/2，所以
    k0.stride=STRIDE_MIN+pass<=STRIDE+BigStride/2
    在k0运行期间，剩余进程的stride都不会改变。
    k0运行完毕后，新的最大的stride可能是原来的k1.stride=STRIDE_MAX，也有可能是STRIDE+pass。
    此时stride最小的进程 不会小于原来的STRIDE_MIN.
    即
    STRIDE_MIN_new < STRIDE_MIN+pass <= STRIDE_MIN+BigStride/2 <=STRIDE_MAX
    则，根据后两项，即证STRIDE_MAX-STRIDE_MIN>=BigStride/2
    
**i dont know if it's right.i hate proving problems.**
### 3
    ```
    use core::cmp::Ordering;

    struct Stride(u64);

    impl PartialOrd for Stride {
        fn partial_cmp(&self, other: &Self) ->  Option<Ordering> {
            let a=self.0;
            let b=other.0;
            //我感觉就是要判断二者的差值是否小于等于BigStride/2，如果大于，说明结果和比较结果恰恰相反。
            // 用无符号减法判断大小
            if a < b {
                if (b-a)>BigStride/2{
                    return                 Some(Ordering::Greater)
                //假设BigStride是预定义的
                }else{
                    return Some(Ordering::Less)
                }
            } else {
                if (a-b)>BigStride/2{
                    (Ordering::Less)
                }else{
                    Some(Ordering::Greater)
                }
            }
        }
    }

    impl PartialEq for Stride {
        fn eq(&self, other: &Self) -> bool {
            false
        }
    }
    ```
## overover,happyhappy