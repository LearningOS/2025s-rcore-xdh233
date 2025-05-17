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

use crate::config::PAGE_SIZE;
use crate::loader::{get_app_data, get_num_app};
use crate::mm::{MapPermission, VirtAddr};
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use alloc::vec::Vec;
use lazy_static::*;
use riscv::addr::BitField;
use switch::__switch;
pub use task::{TaskControlBlock, TaskStatus};

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

/// The task manager inner in 'UPSafeCell'
struct TaskManagerInner {
    /// task list
    tasks: Vec<TaskControlBlock>,
    /// id of current `Running` task
    current_task: usize,
}

lazy_static! {
    /// a `TaskManager` global instance through lazy_static!
    pub static ref TASK_MANAGER: TaskManager = {
        println!("init TASK_MANAGER");
        let num_app = get_num_app();
        println!("num_app = {}", num_app);
        let mut tasks: Vec<TaskControlBlock> = Vec::new();
        for i in 0..num_app {
            tasks.push(TaskControlBlock::new(get_app_data(i), i));
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
    /// But in ch4, we load apps statically, so the first task is a real app.
    fn run_first_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let next_task = &mut inner.tasks[0];
        next_task.task_status = TaskStatus::Running;
        let next_task_cx_ptr = &next_task.task_cx as *const TaskContext;
        drop(inner);
        let mut _unused = TaskContext::zero_init();
        // before this, we should drop local variables that must be dropped manually
        unsafe {
            __switch(&mut _unused as *mut _, next_task_cx_ptr);
        }
        panic!("unreachable in run_first_task!");
    }

    /// Change the status of current `Running` task into `Ready`.
    fn mark_current_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].task_status = TaskStatus::Ready;
    }

    /// Change the status of current `Running` task into `Exited`.
    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].task_status = TaskStatus::Exited;
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

    /// Get the current 'Running' task's token.
    fn get_current_token(&self) -> usize {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_user_token()
    }

    /// Get the current 'Running' task's trap contexts.
    fn get_current_trap_cx(&self) -> &'static mut TrapContext {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_trap_cx()
    }

    /// Get the current 'Running' task's memory set.
    pub fn read_from_va(&self,va:VirtAddr) ->isize{
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        let memory_set=inner.tasks[cur].get_memory_set();
        
        if let Some(pte)=memory_set.translate(va.floor()){   
            if !pte.is_valid() {
                return -1; // 无效页表项直接返回
            }
            if !pte.readable() || !pte.user() {
                return -1; // 再检查权限
            }else{
                return pte.ppn().get_bytes_array()[va.page_offset()] as isize;
            }
        }
        else{
            return -1;      //none
        }
    }
    /// Get the current 'Running' task's memory set.
    pub fn write_to_va(&self,va:VirtAddr,data:u8) ->isize{
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        let memory_set=inner.tasks[cur].get_memory_set();
        
        if let Some(pte)=memory_set.translate(va.floor()){
            if !pte.is_valid()|| !pte.writable() || !pte.user(){
                return -1;
            }else{
                pte.ppn().get_bytes_array()[va.page_offset()]=data;
                return 0;
            }
        }
        else{
            return -1;      //none
        }
    }
    /// wirte_to_va_timeval 
    pub fn write_to_va_timeval<T>(&self,va:VirtAddr,data:T) ->isize{
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        let memory_set=inner.tasks[cur].get_memory_set();
        
        let size = core::mem::size_of::<T>();
        let bytes: &[u8] = unsafe {
            core::slice::from_raw_parts(
            &data as *const T as *const u8,
        core::mem::size_of::<T>(),
            )
        };
        
        if let Some(pte)=memory_set.translate(va.floor()) {
            //println!("first");
            if !pte.is_valid()|| !pte.writable() || !pte.user() {
                return -1;
            }else if (va.page_offset()+size)>PAGE_SIZE {
                //跨页了
                let first_len=PAGE_SIZE-va.page_offset();                
                //unsafe {
                    let first_dst=&mut pte.ppn().get_bytes_array()[va.page_offset()..PAGE_SIZE];
                // 第一页写入
                    //core::ptr::copy_nonoverlapping(bytes.as_ptr(), first_dst, first_len);
                    first_dst.copy_from_slice(&bytes[va.page_offset()..PAGE_SIZE]);
                //}
                // 第二页写入 不是连续的物理页 而是虚拟地址连续 所以应该是va的下一页对应的物理页
                
                //let second_len=size-first_len;
                if let Some(next_pte) = memory_set.translate(VirtAddr::from(va.0 as usize + PAGE_SIZE).floor().into()) {
                    //println!("second");
                    if !next_pte.is_valid() || !next_pte.writable() || !next_pte.user() {
                        return -1;
                    }
                    //unsafe{
                    let second_dst =&mut next_pte.ppn().get_bytes_array()[..bytes.len()-first_len];
                        second_dst.copy_from_slice(&bytes[first_len..]);
                    //}
                } else {
                    return -1;
                }
                0
            }else{
                //根据size进行写入
                //unsafe {
                    let first_dst=&mut pte.ppn().get_bytes_array()[va.page_offset()..va.page_offset()+size];
                // 直接写入
                    first_dst.copy_from_slice(bytes);
                //}
                0
            }
        }
        else{
            -1    //none
        }
    }
    /// map physAddr to _start
    pub fn map_len_to_start(&self,_start:usize,_len:usize,_port:usize)->isize{
        let end=_start+_len;
        let mut start=_start;
        let mut perm = MapPermission::empty();
        if _port.get_bit(0){ perm |= MapPermission::R;}
        if _port.get_bit(1){ perm |= MapPermission::W;}
        if _port.get_bit(2){ perm |= MapPermission::X;}
        perm |= MapPermission::U;//！！！！！！气死我了，果然指导书上的每一个提示都不是白提示的！！！！

        println!("{:?}",perm);
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        let memory_set=inner.tasks[cur].get_memory_set_mut();
        //let vpn_range:VpnRange=VpnRange::new(VirtPageNum::from(start),VirtPageNum::from(end));

        while start < end {
            match memory_set.translate(VirtAddr::from(start).floor()){
                Some(_) => {
                    println!("already mapped.");
                    return -1;
                }
                None => {
                    //println!("before");
                    //memory_set.insert_framed_area(VirtAddr::from(start),VirtAddr::from(start+PAGE_SIZE),perm);
                    // println!(
                    //     "Mapping {:#x} to {:#x} with permission {:?}",
                    //     start,
                    //     start+PAGE_SIZE,
                    //     perm
                    // );
                    start+=PAGE_SIZE;
                    println!("start:{:?}",start);
                }
            }
        }
        //之前的做法是把每一页都作为一个mapArea，对insert_frmaed_area理解不到位。
        memory_set.insert_framed_area(VirtAddr::from(_start),VirtAddr::from(end),perm);
        0
    }
     /// unmap physAddr to _start
    pub fn unmap_len_to_start(&self,_start:usize,_len:usize)->isize{
        let end=_start+_len;
        let mut start=_start;

        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        let memory_set=inner.tasks[cur].get_memory_set_mut();

        while start < end {
            match memory_set.translate(VirtAddr::from(start).floor()){
                Some(_) => {
                    //memory_set.shrink_to(VirtAddr::from(end-PAGE_SIZE).floor().into(),VirtAddr::from(end-PAGE_SIZE).floor().into());
                    start+=PAGE_SIZE;
                    //println!("end")
                }
                None => {return -1;}
            }
        }
        memory_set.shrink_to(VirtAddr::from(_start).floor().into(),VirtAddr::from(_start).floor().into());
        0
    }
    /// Change the current 'Running' task's program break
    pub fn change_current_program_brk(&self, size: i32) -> Option<usize> {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].change_program_brk(size)
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

     /// get syscall count
    pub fn get_syscall_count(&self,syscall_id: usize)-> usize{
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].syscall_count[syscall_id]
    }
    /// increase syscall count
    pub fn inc_syscall_count(&self, syscall_id: usize) {
        let mut inner = self.inner.exclusive_access(); // 获取可变访问
        let current = inner.current_task;
        inner.tasks[current].syscall_count[syscall_id] += 1;
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

/// Get the current 'Running' task's token.
pub fn current_user_token() -> usize {
    TASK_MANAGER.get_current_token()
}

/// Get the current 'Running' task's trap contexts.
pub fn current_trap_cx() -> &'static mut TrapContext {
    TASK_MANAGER.get_current_trap_cx()
}

/// Change the current 'Running' task's program break
pub fn change_program_brk(size: i32) -> Option<usize> {
    TASK_MANAGER.change_current_program_brk(size)
}

/// read from va
pub fn read_from_va(id:VirtAddr) ->isize{
    TASK_MANAGER.read_from_va(id)
}
/// write to va
pub fn write_to_va(id:VirtAddr,data: u8) -> isize{
    TASK_MANAGER.write_to_va(id,data)
}
/// write to va(but type= timeval) actually i use 泛型，嘻嘻
pub fn write_to_va_timeval<T>(va:VirtAddr,data:T) ->isize{
    TASK_MANAGER.write_to_va_timeval(va,data)
}
/// map a physAddr with a size of len to a virtAddr
pub fn map_len_to_start(_start:usize,_len:usize,_port:usize) ->isize{
    TASK_MANAGER.map_len_to_start(_start,_len,_port)
}
/// map a physAddr with a size of len to a virtAddr
pub fn unmap_len_to_start(_start:usize,_len:usize) ->isize{
    TASK_MANAGER.unmap_len_to_start(_start,_len)
}
///wrap the get syscall_count function.
pub fn syscall_count_get(syscall_id:usize)-> usize{
    TASK_MANAGER.get_syscall_count(syscall_id)
}
/// wrap the inc_sysycall_count function.though i dont know why,i just imitate the upper functions
pub fn syscall_count_inc(syscall_id:usize){
    TASK_MANAGER.inc_syscall_count(syscall_id);
}