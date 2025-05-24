//! File and filesystem-related syscalls

use easy_fs::block_cache_sync_all;

use crate::fs::{open_file, OpenFlags, Stat};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer};
use crate::task::{current_task, current_user_token};

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        let res=file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize;
        res
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        trace!("kernel: sys_read .. file.read");
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    trace!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    block_cache_sync_all();
    0
}

/// YOUR JOB: Implement fstat.
pub fn sys_fstat(_fd: usize, _st: *mut Stat) -> isize {
    trace!(
        "kernel:pid[{}] sys_fstat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let task = current_task().unwrap();
    let inner=task.inner_exclusive_access();
    if _fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file ) = &inner.fd_table[_fd] {
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        let stat=file.fstat();
        let token =current_user_token();
        let target_ppages =translated_byte_buffer(token, _st as *const u8, core::mem::size_of::<Stat>());
        
        let bytes:&[u8]= unsafe{
            core::slice::from_raw_parts(
                &stat as *const _ as usize as *const u8,
                core::mem::size_of::<Stat>(),
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
    } else {
        //println!("no such file!");
        -1
    }
}

/// YOUR JOB: Implement linkat.
pub fn sys_linkat(_old_name: *const u8, _new_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_linkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    //通过_old_name找到原来的目录项指向的磁盘块
    //在父目录(此处为root_inode)建立新的目录项，并把目录项指向的磁盘块id设置为同一个？
    let token = current_user_token();
    let old_name=translated_str(token, _old_name);
    let new_name=translated_str(token, _new_name);
    //old name duplicate with the new name
    if old_name==new_name {
        -1
    }else{
        if let Some(app_inode)=open_file(old_name.as_str(), OpenFlags::RDONLY){
            app_inode.link(&new_name)
        }else {
            -1  //源文件不存在
        }
    }
}

/// YOUR JOB: Implement unlinkat.
pub fn sys_unlinkat(_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_unlinkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let name=translated_str(token, _name);
    if let Some(app_inode)=open_file(name.as_str(), OpenFlags::RDONLY){
        app_inode.unlink(&name)
    }else {
        -1  //源文件不存在
    }
}
