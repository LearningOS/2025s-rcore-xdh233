//! Process management syscalls
//!
use alloc::sync::Arc;
use riscv::addr::{BitField};

use crate::{
    config::PAGE_SIZE, fs::{open_file, OpenFlags}, mm::{translated_byte_buffer, translated_refmut, translated_str, MapPermission, VirtAddr}, task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next,
    }, timer::get_time_us
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    //trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    //trace!("kernel: sys_waitpid");
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_get_time NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let us =get_time_us();
    let time_val=TimeVal{
        sec: us/1_000_000,
        usec: us%1_000_000,
    };
    let token =current_user_token();
    let target_ppages =translated_byte_buffer(token, _ts as *const u8, core::mem::size_of::<TimeVal>());
    
    let bytes:&[u8]= unsafe{
        core::slice::from_raw_parts(
            &time_val as *const _ as usize as *const u8,
            core::mem::size_of::<TimeVal>(),
        )
    };
    let mut offset=0;
    for page in target_ppages{
        let len=page.len().min(bytes.len()-offset);
        page[..len].copy_from_slice(&bytes[offset..offset+len]);
        offset+=len;
        if offset >= bytes.len(){
            break;
        }
    }
    0
}

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_mmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    //检查是否合法
    if _start & (PAGE_SIZE - 1) !=0 || _port & !0x7 !=0 || _port & 0x7 ==0{
        return -1;
    }

    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();

    let mut start =_start;
    //("start={:?}",start);
    let end=start+_len;
    while start< end{
        match inner.memory_set.translate(VirtAddr::from(start).floor()){
            Some(_)=>{
                //("{:?}already mapped.",start);
                return -1;
            }
            None => {
                start+=PAGE_SIZE;
            }
        }
    }
    let mut perm=MapPermission::empty() | MapPermission::U;
    if _port.get_bit(0) {   perm|=MapPermission::R;    }
    if _port.get_bit(1) {   perm|=MapPermission::W;    }
    if _port.get_bit(2) {   perm|=MapPermission::X;    }
    inner.memory_set.insert_framed_area(_start.into(), end.into(), perm);
    //println!("start={:?},end={:?},permission={:?}",_start,end,perm);
    0
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_munmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    //先检查_start是否有效
    if _start & (PAGE_SIZE - 1) !=0 {
        return -1;
    }

    let mut start=_start;
    let end=_start+_len;

    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();

    //检查是否未被映射
    while start < end {
        match inner.memory_set.translate(VirtAddr::from(start).floor()){
            Some(_) => {
                start+=PAGE_SIZE;
            }
            None => {
                println!("None");
                return -1;
            }
        }
    }
    inner.memory_set.remove_area_with_start_vpn(VirtAddr::from(_start).floor());
    0
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(_path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token=current_user_token();
    let path=translated_str(token, _path);
    if let Some(app_inode)=open_file(path.as_str(), OpenFlags::RDONLY){
        let all_data=app_inode.read_all();
        let task=current_task().unwrap();
        let new_task=task.spawn(all_data.as_slice());
        let new_pid=new_task.pid.0;

        let trap_cx=task.inner_exclusive_access().get_trap_cx();
         trap_cx.x[10] = 0;
        // add new task to scheduler
        add_task(new_task);
        new_pid as isize
    }else{
        return -1
    }
}

// YOUR JOB: Set task priority.
pub fn sys_set_priority(_prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    if _prio >= 2 {
        let task=current_task().unwrap();
        let mut inner = task.inner_exclusive_access();
        inner.prio=_prio;
        _prio
    }else{
        -1
    }
}
